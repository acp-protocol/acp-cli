//! @acp:module "Python Extractor"
//! @acp:summary "Symbol extraction for Python source files"
//! @acp:domain cli
//! @acp:layer parsing

use tree_sitter::{Language, Tree, Node};
use crate::error::Result;
use super::{LanguageExtractor, node_text};
use crate::ast::{ExtractedSymbol, Import, ImportedName, FunctionCall, Parameter, SymbolKind, Visibility};

/// Python language extractor
pub struct PythonExtractor;

impl LanguageExtractor for PythonExtractor {
    fn language(&self) -> Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn name(&self) -> &'static str {
        "python"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["py", "pyi"]
    }

    fn extract_symbols(&self, tree: &Tree, source: &str) -> Result<Vec<ExtractedSymbol>> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        self.extract_symbols_recursive(&root, source, &mut symbols, None);
        Ok(symbols)
    }

    fn extract_imports(&self, tree: &Tree, source: &str) -> Result<Vec<Import>> {
        let mut imports = Vec::new();
        let root = tree.root_node();
        self.extract_imports_recursive(&root, source, &mut imports);
        Ok(imports)
    }

    fn extract_calls(&self, tree: &Tree, source: &str, current_function: Option<&str>) -> Result<Vec<FunctionCall>> {
        let mut calls = Vec::new();
        let root = tree.root_node();
        self.extract_calls_recursive(&root, source, &mut calls, current_function);
        Ok(calls)
    }

    fn extract_doc_comment(&self, node: &Node, source: &str) -> Option<String> {
        // Look for docstring (first expression statement in function/class body)
        let body = node.child_by_field_name("body")?;
        let mut cursor = body.walk();
        // Only check the first child - docstring must be first statement
        let first_child = body.children(&mut cursor).next()?;
        if first_child.kind() == "expression_statement" {
            if let Some(string) = first_child.child(0) {
                if string.kind() == "string" {
                    let text = node_text(&string, source);
                    return Some(Self::clean_docstring(text));
                }
            }
        }
        None
    }
}

impl PythonExtractor {
    fn extract_symbols_recursive(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        match node.kind() {
            "function_definition" => {
                if let Some(sym) = self.extract_function(node, source, parent) {
                    symbols.push(sym);
                    // Don't recurse into function body for nested functions
                    // (they'll be handled when we visit them)
                }
            }

            "class_definition" => {
                if let Some(sym) = self.extract_class(node, source, parent) {
                    let class_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract class methods
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_class_members(&body, source, symbols, Some(&class_name));
                    }
                    return; // Don't recurse further
                }
            }

            "decorated_definition" => {
                // Handle decorated functions/classes
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "function_definition" || child.kind() == "class_definition" {
                        self.extract_symbols_recursive(&child, source, symbols, parent);
                    }
                }
                return;
            }

            _ => {}
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_symbols_recursive(&child, source, symbols, parent);
        }
    }

    fn extract_function(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name.clone(),
            SymbolKind::Function,
            node.start_position().row + 1,
            node.end_position().row + 1,
        )
        .with_columns(node.start_position().column, node.end_position().column);

        // Check if this is async
        let text = node_text(node, source);
        if text.starts_with("async") {
            sym = sym.async_fn();
        }

        // Check visibility (Python convention: _name is private, __name is very private)
        if name.starts_with("__") && !name.ends_with("__") {
            sym.visibility = Visibility::Private;
        } else if name.starts_with('_') {
            sym.visibility = Visibility::Protected;
        } else {
            sym = sym.exported();
        }

        // Extract parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        // Extract return type annotation
        if let Some(ret_type) = node.child_by_field_name("return_type") {
            sym.return_type = Some(node_text(&ret_type, source).trim_start_matches("->").trim().to_string());
        }

        // Extract docstring
        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
            sym.kind = SymbolKind::Method;
        }

        sym.signature = Some(self.build_function_signature(node, source));

        Some(sym)
    }

    fn extract_class(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name.clone(),
            SymbolKind::Class,
            node.start_position().row + 1,
            node.end_position().row + 1,
        )
        .with_columns(node.start_position().column, node.end_position().column);

        // Python classes starting with _ are considered internal
        if name.starts_with('_') {
            sym.visibility = Visibility::Protected;
        } else {
            sym = sym.exported();
        }

        // Extract docstring
        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_class_members(
        &self,
        body: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        class_name: Option<&str>,
    ) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "function_definition" => {
                    if let Some(sym) = self.extract_function(&child, source, class_name) {
                        symbols.push(sym);
                    }
                }
                "decorated_definition" => {
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() == "function_definition" {
                            if let Some(mut sym) = self.extract_function(&inner, source, class_name) {
                                // Check for @staticmethod or @classmethod
                                let deco_text = node_text(&child, source);
                                if deco_text.contains("@staticmethod") {
                                    sym = sym.static_fn();
                                }
                                symbols.push(sym);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn extract_parameters(&self, params: &Node, source: &str, sym: &mut ExtractedSymbol) {
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    let name = node_text(&child, source);
                    // Skip 'self' and 'cls'
                    if name != "self" && name != "cls" {
                        sym.add_parameter(Parameter {
                            name: name.to_string(),
                            type_info: None,
                            default_value: None,
                            is_rest: false,
                            is_optional: false,
                        });
                    }
                }
                "typed_parameter" => {
                    let name = child.child_by_field_name("name")
                        .map(|n| node_text(&n, source).to_string())
                        .unwrap_or_default();

                    if name != "self" && name != "cls" {
                        let type_info = child.child_by_field_name("type")
                            .map(|n| node_text(&n, source).to_string());

                        sym.add_parameter(Parameter {
                            name,
                            type_info,
                            default_value: None,
                            is_rest: false,
                            is_optional: false,
                        });
                    }
                }
                "default_parameter" | "typed_default_parameter" => {
                    let name = child.child_by_field_name("name")
                        .map(|n| node_text(&n, source).to_string())
                        .unwrap_or_default();

                    if name != "self" && name != "cls" {
                        let type_info = child.child_by_field_name("type")
                            .map(|n| node_text(&n, source).to_string());
                        let default_value = child.child_by_field_name("value")
                            .map(|n| node_text(&n, source).to_string());

                        sym.add_parameter(Parameter {
                            name,
                            type_info,
                            default_value,
                            is_rest: false,
                            is_optional: true,
                        });
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    let text = node_text(&child, source);
                    let name = text.trim_start_matches('*').to_string();
                    let is_kwargs = text.starts_with("**");

                    sym.add_parameter(Parameter {
                        name,
                        type_info: None,
                        default_value: None,
                        is_rest: !is_kwargs,
                        is_optional: true,
                    });
                }
                _ => {}
            }
        }
    }

    fn extract_imports_recursive(&self, node: &Node, source: &str, imports: &mut Vec<Import>) {
        match node.kind() {
            "import_statement" => {
                if let Some(import) = self.parse_import(node, source) {
                    imports.push(import);
                }
            }
            "import_from_statement" => {
                if let Some(import) = self.parse_from_import(node, source) {
                    imports.push(import);
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_imports_recursive(&child, source, imports);
        }
    }

    fn parse_import(&self, node: &Node, source: &str) -> Option<Import> {
        let mut import = Import {
            source: String::new(),
            names: Vec::new(),
            is_default: false,
            is_namespace: false,
            line: node.start_position().row + 1,
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" => {
                    let name = node_text(&child, source).to_string();
                    import.source = name.clone();
                    import.names.push(ImportedName {
                        name,
                        alias: None,
                    });
                }
                "aliased_import" => {
                    let name = child.child_by_field_name("name")
                        .map(|n| node_text(&n, source).to_string())
                        .unwrap_or_default();
                    let alias = child.child_by_field_name("alias")
                        .map(|n| node_text(&n, source).to_string());

                    import.source = name.clone();
                    import.names.push(ImportedName { name, alias });
                }
                _ => {}
            }
        }

        Some(import)
    }

    fn parse_from_import(&self, node: &Node, source: &str) -> Option<Import> {
        let module = node.child_by_field_name("module_name")
            .map(|n| node_text(&n, source).to_string())
            .unwrap_or_default();

        let mut import = Import {
            source: module,
            names: Vec::new(),
            is_default: false,
            is_namespace: false,
            line: node.start_position().row + 1,
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "wildcard_import" => {
                    import.is_namespace = true;
                    import.names.push(ImportedName {
                        name: "*".to_string(),
                        alias: None,
                    });
                }
                "dotted_name" | "identifier" => {
                    import.names.push(ImportedName {
                        name: node_text(&child, source).to_string(),
                        alias: None,
                    });
                }
                "aliased_import" => {
                    let name = child.child_by_field_name("name")
                        .map(|n| node_text(&n, source).to_string())
                        .unwrap_or_default();
                    let alias = child.child_by_field_name("alias")
                        .map(|n| node_text(&n, source).to_string());

                    import.names.push(ImportedName { name, alias });
                }
                _ => {}
            }
        }

        Some(import)
    }

    fn extract_calls_recursive(
        &self,
        node: &Node,
        source: &str,
        calls: &mut Vec<FunctionCall>,
        current_function: Option<&str>,
    ) {
        if node.kind() == "call" {
            if let Some(call) = self.parse_call(node, source, current_function) {
                calls.push(call);
            }
        }

        let func_name = if node.kind() == "function_definition" {
            node.child_by_field_name("name")
                .map(|n| node_text(&n, source))
        } else {
            None
        };

        let current = func_name.map(String::from).or_else(|| current_function.map(String::from));

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_calls_recursive(&child, source, calls, current.as_deref());
        }
    }

    fn parse_call(&self, node: &Node, source: &str, current_function: Option<&str>) -> Option<FunctionCall> {
        let function = node.child_by_field_name("function")?;

        let (callee, is_method, receiver) = match function.kind() {
            "attribute" => {
                let object = function.child_by_field_name("object")
                    .map(|n| node_text(&n, source).to_string());
                let attr = function.child_by_field_name("attribute")
                    .map(|n| node_text(&n, source).to_string())?;
                (attr, true, object)
            }
            "identifier" => {
                (node_text(&function, source).to_string(), false, None)
            }
            _ => return None,
        };

        Some(FunctionCall {
            caller: current_function.unwrap_or("<module>").to_string(),
            callee,
            line: node.start_position().row + 1,
            is_method,
            receiver,
        })
    }

    fn build_function_signature(&self, node: &Node, source: &str) -> String {
        let async_kw = if node_text(node, source).starts_with("async") {
            "async "
        } else {
            ""
        };

        let name = node.child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or("unknown");

        let params = node.child_by_field_name("parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("()");

        let return_type = node.child_by_field_name("return_type")
            .map(|n| format!(" {}", node_text(&n, source)))
            .unwrap_or_default();

        format!("{}def {}{}{}",
            async_kw,
            name,
            params,
            return_type
        )
    }

    fn clean_docstring(text: &str) -> String {
        // Remove quotes
        let text = text
            .trim_start_matches("\"\"\"")
            .trim_start_matches("'''")
            .trim_end_matches("\"\"\"")
            .trim_end_matches("'''")
            .trim();

        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_py(source: &str) -> (Tree, String) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        (tree, source.to_string())
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
def greet(name: str) -> str:
    """Greet someone."""
    return f"Hello, {name}!"
"#;
        let (tree, src) = parse_py(source);
        let extractor = PythonExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "greet");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
class UserService:
    """A service for managing users."""

    def __init__(self, name: str):
        self.name = name

    def greet(self) -> str:
        return f"Hello, {self.name}!"

    @staticmethod
    def create():
        return UserService("default")
"#;
        let (tree, src) = parse_py(source);
        let extractor = PythonExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "__init__" && s.kind == SymbolKind::Method));
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Method));
        assert!(symbols.iter().any(|s| s.name == "create" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_extract_async_function() {
        let source = r#"
async def fetch_data(url: str) -> dict:
    """Fetch data from URL."""
    pass
"#;
        let (tree, src) = parse_py(source);
        let extractor = PythonExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "fetch_data");
        assert!(symbols[0].is_async);
    }
}

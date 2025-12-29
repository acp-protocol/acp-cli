//! @acp:module "JavaScript Extractor"
//! @acp:summary "Symbol extraction for JavaScript source files"
//! @acp:domain cli
//! @acp:layer parsing

use super::{node_text, LanguageExtractor};
use crate::ast::{
    ExtractedSymbol, FunctionCall, Import, ImportedName, Parameter, SymbolKind, Visibility,
};
use crate::error::Result;
use tree_sitter::{Language, Node, Tree};

/// JavaScript language extractor
pub struct JavaScriptExtractor;

impl LanguageExtractor for JavaScriptExtractor {
    fn language(&self) -> Language {
        tree_sitter_javascript::LANGUAGE.into()
    }

    fn name(&self) -> &'static str {
        "javascript"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["js", "jsx", "mjs", "cjs"]
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

    fn extract_calls(
        &self,
        tree: &Tree,
        source: &str,
        current_function: Option<&str>,
    ) -> Result<Vec<FunctionCall>> {
        let mut calls = Vec::new();
        let root = tree.root_node();
        self.extract_calls_recursive(&root, source, &mut calls, current_function);
        Ok(calls)
    }

    fn extract_doc_comment(&self, node: &Node, source: &str) -> Option<String> {
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "comment" {
                let comment = node_text(&prev, source);
                if comment.starts_with("/**") {
                    return Some(Self::clean_jsdoc(comment));
                }
                if let Some(rest) = comment.strip_prefix("//") {
                    return Some(rest.trim().to_string());
                }
            }
        }
        None
    }
}

impl JavaScriptExtractor {
    fn extract_symbols_recursive(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        match node.kind() {
            "function_declaration" => {
                if let Some(sym) = self.extract_function(node, source, parent) {
                    symbols.push(sym);
                }
            }

            "lexical_declaration" | "variable_declaration" => {
                self.extract_variable_symbols(node, source, symbols, parent);
            }

            "class_declaration" => {
                if let Some(sym) = self.extract_class(node, source, parent) {
                    let class_name = sym.name.clone();
                    symbols.push(sym);

                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_class_members(&body, source, symbols, Some(&class_name));
                    }
                }
            }

            "export_statement" => {
                self.extract_export_symbols(node, source, symbols, parent);
            }

            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_symbols_recursive(&child, source, symbols, parent);
        }
    }

    fn extract_function(
        &self,
        node: &Node,
        source: &str,
        parent: Option<&str>,
    ) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Function,
            node.start_position().row + 1,
            node.end_position().row + 1,
        )
        .with_columns(node.start_position().column, node.end_position().column);

        let text = node_text(node, source);
        if text.starts_with("async") {
            sym = sym.async_fn();
        }

        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        sym.signature = Some(self.build_function_signature(node, source));

        Some(sym)
    }

    fn extract_variable_symbols(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let (Some(name_node), Some(value)) = (
                    child.child_by_field_name("name"),
                    child.child_by_field_name("value"),
                ) {
                    let name = node_text(&name_node, source);
                    let value_kind = value.kind();

                    if value_kind == "arrow_function" || value_kind == "function_expression" {
                        let mut sym = ExtractedSymbol::new(
                            name.to_string(),
                            SymbolKind::Function,
                            node.start_position().row + 1,
                            node.end_position().row + 1,
                        );

                        let text = node_text(&value, source);
                        if text.starts_with("async") {
                            sym = sym.async_fn();
                        }

                        if let Some(params) = value.child_by_field_name("parameters") {
                            self.extract_parameters(&params, source, &mut sym);
                        }

                        sym.doc_comment = self.extract_doc_comment(node, source);

                        if let Some(p) = parent {
                            sym = sym.with_parent(p);
                        }

                        if let Some(parent_node) = node.parent() {
                            if parent_node.kind() == "export_statement" {
                                sym = sym.exported();
                            }
                        }

                        symbols.push(sym);
                    } else {
                        let kind = if node_text(node, source).starts_with("const") {
                            SymbolKind::Constant
                        } else {
                            SymbolKind::Variable
                        };

                        let mut sym = ExtractedSymbol::new(
                            name.to_string(),
                            kind,
                            node.start_position().row + 1,
                            node.end_position().row + 1,
                        );

                        if let Some(p) = parent {
                            sym = sym.with_parent(p);
                        }

                        symbols.push(sym);
                    }
                }
            }
        }
    }

    fn extract_class(
        &self,
        node: &Node,
        source: &str,
        parent: Option<&str>,
    ) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Class,
            node.start_position().row + 1,
            node.end_position().row + 1,
        )
        .with_columns(node.start_position().column, node.end_position().column);

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
            if child.kind() == "method_definition" {
                if let Some(sym) = self.extract_method(&child, source, class_name) {
                    symbols.push(sym);
                }
            }
        }
    }

    fn extract_method(
        &self,
        node: &Node,
        source: &str,
        class_name: Option<&str>,
    ) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        // Check for private (# prefix) before passing name to new
        let is_private = name.starts_with('#');

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Method,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        let text = node_text(node, source);
        if text.contains("static") {
            sym = sym.static_fn();
        }
        if text.contains("async") {
            sym = sym.async_fn();
        }

        // JavaScript doesn't have visibility modifiers by default
        // Private fields start with #
        if is_private {
            sym.visibility = Visibility::Private;
        }

        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = class_name {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_export_symbols(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "function_declaration" => {
                    if let Some(mut sym) = self.extract_function(&child, source, parent) {
                        sym = sym.exported();
                        symbols.push(sym);
                    }
                }
                "class_declaration" => {
                    if let Some(mut sym) = self.extract_class(&child, source, parent) {
                        sym = sym.exported();
                        let class_name = sym.name.clone();
                        symbols.push(sym);

                        if let Some(body) = child.child_by_field_name("body") {
                            self.extract_class_members(&body, source, symbols, Some(&class_name));
                        }
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    self.extract_variable_symbols(&child, source, symbols, parent);
                    if let Some(last) = symbols.last_mut() {
                        last.exported = true;
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
                    sym.add_parameter(Parameter {
                        name: node_text(&child, source).to_string(),
                        type_info: None,
                        default_value: None,
                        is_rest: false,
                        is_optional: false,
                    });
                }
                "assignment_pattern" => {
                    let name = child
                        .child_by_field_name("left")
                        .map(|n| node_text(&n, source).to_string())
                        .unwrap_or_default();
                    let default_value = child
                        .child_by_field_name("right")
                        .map(|n| node_text(&n, source).to_string());

                    sym.add_parameter(Parameter {
                        name,
                        type_info: None,
                        default_value,
                        is_rest: false,
                        is_optional: true,
                    });
                }
                "rest_pattern" => {
                    let name = node_text(&child, source)
                        .trim_start_matches("...")
                        .to_string();
                    sym.add_parameter(Parameter {
                        name,
                        type_info: None,
                        default_value: None,
                        is_rest: true,
                        is_optional: false,
                    });
                }
                _ => {}
            }
        }
    }

    fn extract_imports_recursive(&self, node: &Node, source: &str, imports: &mut Vec<Import>) {
        if node.kind() == "import_statement" {
            if let Some(import) = self.parse_import(node, source) {
                imports.push(import);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_imports_recursive(&child, source, imports);
        }
    }

    fn parse_import(&self, node: &Node, source: &str) -> Option<Import> {
        let source_node = node.child_by_field_name("source")?;
        let source_path = node_text(&source_node, source)
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();

        let mut import = Import {
            source: source_path,
            names: Vec::new(),
            is_default: false,
            is_namespace: false,
            line: node.start_position().row + 1,
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "import_clause" {
                self.parse_import_clause(&child, source, &mut import);
            }
        }

        Some(import)
    }

    fn parse_import_clause(&self, clause: &Node, source: &str, import: &mut Import) {
        let mut cursor = clause.walk();
        for child in clause.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    import.is_default = true;
                    import.names.push(ImportedName {
                        name: "default".to_string(),
                        alias: Some(node_text(&child, source).to_string()),
                    });
                }
                "namespace_import" => {
                    import.is_namespace = true;
                    if let Some(name_node) = child.child_by_field_name("name") {
                        import.names.push(ImportedName {
                            name: "*".to_string(),
                            alias: Some(node_text(&name_node, source).to_string()),
                        });
                    }
                }
                "named_imports" => {
                    self.parse_named_imports(&child, source, import);
                }
                _ => {}
            }
        }
    }

    fn parse_named_imports(&self, node: &Node, source: &str, import: &mut Import) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "import_specifier" {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_default();

                let alias = child
                    .child_by_field_name("alias")
                    .map(|n| node_text(&n, source).to_string());

                import.names.push(ImportedName { name, alias });
            }
        }
    }

    fn extract_calls_recursive(
        &self,
        node: &Node,
        source: &str,
        calls: &mut Vec<FunctionCall>,
        current_function: Option<&str>,
    ) {
        if node.kind() == "call_expression" {
            if let Some(call) = self.parse_call(node, source, current_function) {
                calls.push(call);
            }
        }

        let func_name = match node.kind() {
            "function_declaration" | "method_definition" => node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source)),
            _ => None,
        };

        let current = func_name
            .map(String::from)
            .or_else(|| current_function.map(String::from));

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_calls_recursive(&child, source, calls, current.as_deref());
        }
    }

    fn parse_call(
        &self,
        node: &Node,
        source: &str,
        current_function: Option<&str>,
    ) -> Option<FunctionCall> {
        let function = node.child_by_field_name("function")?;

        let (callee, is_method, receiver) = match function.kind() {
            "member_expression" => {
                let object = function
                    .child_by_field_name("object")
                    .map(|n| node_text(&n, source).to_string());
                let property = function
                    .child_by_field_name("property")
                    .map(|n| node_text(&n, source).to_string())?;
                (property, true, object)
            }
            "identifier" => (node_text(&function, source).to_string(), false, None),
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
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or("anonymous");

        let params = node
            .child_by_field_name("parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("()");

        format!("function {}{}", name, params)
    }

    fn clean_jsdoc(comment: &str) -> String {
        comment
            .trim_start_matches("/**")
            .trim_end_matches("*/")
            .lines()
            .map(|line| line.trim().trim_start_matches('*').trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_js(source: &str) -> (Tree, String) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        (tree, source.to_string())
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
function greet(name) {
    return `Hello, ${name}!`;
}
"#;
        let (tree, src) = parse_js(source);
        let extractor = JavaScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "greet");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
class UserService {
    constructor(name) {
        this.name = name;
    }

    greet() {
        return `Hello, ${this.name}!`;
    }
}
"#;
        let (tree, src) = parse_js(source);
        let extractor = JavaScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols
            .iter()
            .any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
        assert!(symbols
            .iter()
            .any(|s| s.name == "greet" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_extract_arrow_function() {
        let source = r#"
const add = (a, b) => a + b;
"#;
        let (tree, src) = parse_js(source);
        let extractor = JavaScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "add");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }
}

//! @acp:module "Go Extractor"
//! @acp:summary "Symbol extraction for Go source files"
//! @acp:domain cli
//! @acp:layer parsing

use tree_sitter::{Language, Tree, Node};
use crate::error::Result;
use super::{LanguageExtractor, node_text};
use crate::ast::{ExtractedSymbol, Import, FunctionCall, Parameter, SymbolKind, Visibility};

/// Go language extractor
pub struct GoExtractor;

impl LanguageExtractor for GoExtractor {
    fn language(&self) -> Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn name(&self) -> &'static str {
        "go"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
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
        // Look for comment groups before this node
        let mut comments = Vec::new();
        let mut current = node.prev_sibling();

        while let Some(prev) = current {
            if prev.kind() == "comment" {
                let comment = node_text(&prev, source);
                comments.push(comment.trim_start_matches("//").trim().to_string());
                current = prev.prev_sibling();
            } else {
                break;
            }
        }

        if comments.is_empty() {
            None
        } else {
            comments.reverse();
            Some(comments.join("\n"))
        }
    }
}

impl GoExtractor {
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

            "method_declaration" => {
                if let Some(sym) = self.extract_method(node, source) {
                    symbols.push(sym);
                }
            }

            "type_declaration" => {
                self.extract_type_declaration(node, source, symbols, parent);
            }

            "const_declaration" | "var_declaration" => {
                self.extract_var_declaration(node, source, symbols, parent);
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

        // Go: Exported if first letter is uppercase
        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            sym = sym.exported();
            sym.visibility = Visibility::Public;
        } else {
            sym.visibility = Visibility::Private;
        }

        // Extract parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        // Extract return type
        if let Some(result) = node.child_by_field_name("result") {
            sym.return_type = Some(node_text(&result, source).to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        sym.signature = Some(self.build_function_signature(node, source));

        Some(sym)
    }

    fn extract_method(&self, node: &Node, source: &str) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name.clone(),
            SymbolKind::Method,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        // Go: Exported if first letter is uppercase
        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            sym = sym.exported();
            sym.visibility = Visibility::Public;
        } else {
            sym.visibility = Visibility::Private;
        }

        // Get receiver type as parent
        if let Some(receiver) = node.child_by_field_name("receiver") {
            let receiver_text = node_text(&receiver, source);
            // Extract type name from receiver (e.g., "(s *Server)" -> "Server")
            let type_name = receiver_text
                .trim_matches(|c| c == '(' || c == ')')
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches('*');
            sym = sym.with_parent(type_name);
        }

        // Extract parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        // Extract return type
        if let Some(result) = node.child_by_field_name("result") {
            sym.return_type = Some(node_text(&result, source).to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);
        sym.signature = Some(self.build_method_signature(node, source));

        Some(sym)
    }

    fn extract_type_declaration(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_spec" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();

                    // Determine the kind based on the type
                    let type_node = child.child_by_field_name("type");
                    let kind = type_node.map(|t| match t.kind() {
                        "struct_type" => SymbolKind::Struct,
                        "interface_type" => SymbolKind::Interface,
                        _ => SymbolKind::TypeAlias,
                    }).unwrap_or(SymbolKind::TypeAlias);

                    let mut sym = ExtractedSymbol::new(
                        name.clone(),
                        kind,
                        child.start_position().row + 1,
                        child.end_position().row + 1,
                    );

                    // Go: Exported if first letter is uppercase
                    if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        sym = sym.exported();
                        sym.visibility = Visibility::Public;
                    } else {
                        sym.visibility = Visibility::Private;
                    }

                    sym.doc_comment = self.extract_doc_comment(&child, source);

                    if let Some(p) = parent {
                        sym = sym.with_parent(p);
                    }

                    symbols.push(sym);

                    // Extract struct fields
                    if kind == SymbolKind::Struct {
                        if let Some(type_node) = type_node {
                            self.extract_struct_fields(&type_node, source, symbols, Some(&name));
                        }
                    }

                    // Extract interface methods
                    if kind == SymbolKind::Interface {
                        if let Some(type_node) = type_node {
                            self.extract_interface_methods(&type_node, source, symbols, Some(&name));
                        }
                    }
                }
            }
        }
    }

    fn extract_struct_fields(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        struct_name: Option<&str>,
    ) {
        // Go struct_type has field_declaration_list as direct child (not a "body" field)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "field_declaration_list" {
                let mut field_cursor = child.walk();
                for field in child.children(&mut field_cursor) {
                    if field.kind() == "field_declaration" {
                        self.extract_single_field(&field, source, symbols, struct_name);
                    }
                }
            } else if child.kind() == "field_declaration" {
                // Direct field declaration
                self.extract_single_field(&child, source, symbols, struct_name);
            }
        }
    }

    fn extract_single_field(
        &self,
        field: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        struct_name: Option<&str>,
    ) {
        let mut field_cursor = field.walk();
        for field_child in field.children(&mut field_cursor) {
            if field_child.kind() == "field_identifier" {
                let name = node_text(&field_child, source).to_string();

                let mut sym = ExtractedSymbol::new(
                    name.clone(),
                    SymbolKind::Field,
                    field.start_position().row + 1,
                    field.end_position().row + 1,
                );

                // Go: Exported if first letter is uppercase
                if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    sym = sym.exported();
                    sym.visibility = Visibility::Public;
                } else {
                    sym.visibility = Visibility::Private;
                }

                if let Some(type_node) = field.child_by_field_name("type") {
                    sym.type_info = Some(node_text(&type_node, source).to_string());
                }

                if let Some(p) = struct_name {
                    sym = sym.with_parent(p);
                }

                symbols.push(sym);
            }
        }
    }

    fn extract_interface_methods(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        interface_name: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "method_spec" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();

                    let mut sym = ExtractedSymbol::new(
                        name.clone(),
                        SymbolKind::Method,
                        child.start_position().row + 1,
                        child.end_position().row + 1,
                    );

                    if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        sym = sym.exported();
                        sym.visibility = Visibility::Public;
                    } else {
                        sym.visibility = Visibility::Private;
                    }

                    if let Some(params) = child.child_by_field_name("parameters") {
                        self.extract_parameters(&params, source, &mut sym);
                    }

                    if let Some(result) = child.child_by_field_name("result") {
                        sym.return_type = Some(node_text(&result, source).to_string());
                    }

                    if let Some(p) = interface_name {
                        sym = sym.with_parent(p);
                    }

                    symbols.push(sym);
                }
            }
        }
    }

    fn extract_var_declaration(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        let is_const = node.kind() == "const_declaration";

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "const_spec" || child.kind() == "var_spec" {
                let mut name_cursor = child.walk();
                for name_child in child.children(&mut name_cursor) {
                    if name_child.kind() == "identifier" {
                        let name = node_text(&name_child, source).to_string();

                        let mut sym = ExtractedSymbol::new(
                            name.clone(),
                            if is_const { SymbolKind::Constant } else { SymbolKind::Variable },
                            child.start_position().row + 1,
                            child.end_position().row + 1,
                        );

                        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                            sym = sym.exported();
                            sym.visibility = Visibility::Public;
                        } else {
                            sym.visibility = Visibility::Private;
                        }

                        if let Some(type_node) = child.child_by_field_name("type") {
                            sym.type_info = Some(node_text(&type_node, source).to_string());
                        }

                        if let Some(p) = parent {
                            sym = sym.with_parent(p);
                        }

                        symbols.push(sym);
                    }
                }
            }
        }
    }

    fn extract_parameters(&self, params: &Node, source: &str, sym: &mut ExtractedSymbol) {
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                let name = child.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_default();

                let type_info = child.child_by_field_name("type")
                    .map(|n| node_text(&n, source).to_string());

                // Handle variadic parameters
                let is_rest = type_info.as_ref().map(|t| t.starts_with("...")).unwrap_or(false);

                sym.add_parameter(Parameter {
                    name,
                    type_info,
                    default_value: None,
                    is_rest,
                    is_optional: false,
                });
            }
        }
    }

    fn extract_imports_recursive(&self, node: &Node, source: &str, imports: &mut Vec<Import>) {
        if node.kind() == "import_declaration" {
            self.parse_import_declaration(node, source, imports);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_imports_recursive(&child, source, imports);
        }
    }

    fn parse_import_declaration(&self, node: &Node, source: &str, imports: &mut Vec<Import>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "import_spec" || child.kind() == "import_spec_list" {
                self.parse_import_spec(&child, source, imports);
            }
        }
    }

    fn parse_import_spec(&self, node: &Node, source: &str, imports: &mut Vec<Import>) {
        if node.kind() == "import_spec_list" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "import_spec" {
                    self.parse_import_spec(&child, source, imports);
                }
            }
            return;
        }

        let path = node.child_by_field_name("path")
            .map(|n| node_text(&n, source).trim_matches('"').to_string())
            .unwrap_or_default();

        let alias = node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string());

        let is_dot_import = alias.as_ref().map(|a| a == ".").unwrap_or(false);

        imports.push(Import {
            source: path,
            names: Vec::new(),
            is_default: false,
            is_namespace: is_dot_import,
            line: node.start_position().row + 1,
        });
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
            "function_declaration" | "method_declaration" => {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source))
            }
            _ => None,
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
            "selector_expression" => {
                let operand = function.child_by_field_name("operand")
                    .map(|n| node_text(&n, source).to_string());
                let field = function.child_by_field_name("field")
                    .map(|n| node_text(&n, source).to_string())?;
                (field, true, operand)
            }
            "identifier" => {
                (node_text(&function, source).to_string(), false, None)
            }
            _ => return None,
        };

        Some(FunctionCall {
            caller: current_function.unwrap_or("<package>").to_string(),
            callee,
            line: node.start_position().row + 1,
            is_method,
            receiver,
        })
    }

    fn build_function_signature(&self, node: &Node, source: &str) -> String {
        let name = node.child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or("unknown");

        let params = node.child_by_field_name("parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("()");

        let result = node.child_by_field_name("result")
            .map(|n| format!(" {}", node_text(&n, source)))
            .unwrap_or_default();

        format!("func {}{}{}",
            name,
            params,
            result
        )
    }

    fn build_method_signature(&self, node: &Node, source: &str) -> String {
        let receiver = node.child_by_field_name("receiver")
            .map(|n| format!("{} ", node_text(&n, source)))
            .unwrap_or_default();

        let name = node.child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or("unknown");

        let params = node.child_by_field_name("parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("()");

        let result = node.child_by_field_name("result")
            .map(|n| format!(" {}", node_text(&n, source)))
            .unwrap_or_default();

        format!("func {}{}{}{}",
            receiver,
            name,
            params,
            result
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_go(source: &str) -> (Tree, String) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        (tree, source.to_string())
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
package main

func Greet(name string) string {
    return "Hello, " + name + "!"
}
"#;
        let (tree, src) = parse_go(source);
        let extractor = GoExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Greet" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
package main

type User struct {
    Name string
    age  int
}
"#;
        let (tree, src) = parse_go(source);
        let extractor = GoExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "User" && s.kind == SymbolKind::Struct));
        assert!(symbols.iter().any(|s| s.name == "Name" && s.kind == SymbolKind::Field && s.exported));
        assert!(symbols.iter().any(|s| s.name == "age" && s.kind == SymbolKind::Field && !s.exported));
    }

    #[test]
    fn test_extract_method() {
        let source = r#"
package main

func (u *User) Greet() string {
    return "Hello, " + u.Name + "!"
}
"#;
        let (tree, src) = parse_go(source);
        let extractor = GoExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Greet" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_extract_interface() {
        let source = r#"
package main

type Greeter interface {
    Greet() string
    Farewell() string
}
"#;
        let (tree, src) = parse_go(source);
        let extractor = GoExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Greeter" && s.kind == SymbolKind::Interface));
    }
}

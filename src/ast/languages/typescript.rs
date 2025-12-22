//! @acp:module "TypeScript Extractor"
//! @acp:summary "Symbol extraction for TypeScript source files"
//! @acp:domain cli
//! @acp:layer parsing

use tree_sitter::{Language, Tree, Node};
use crate::error::Result;
use super::{LanguageExtractor, node_text};
use crate::ast::{ExtractedSymbol, Import, ImportedName, FunctionCall, Parameter, SymbolKind, Visibility};

/// TypeScript language extractor
pub struct TypeScriptExtractor;

impl LanguageExtractor for TypeScriptExtractor {
    fn language(&self) -> Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn name(&self) -> &'static str {
        "typescript"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx"]
    }

    fn extract_symbols(&self, tree: &Tree, source: &str) -> Result<Vec<ExtractedSymbol>> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        self.extract_symbols_recursive(&root, source, &mut symbols, None);

        // Second pass: mark symbols that are exported via `export { name1, name2 }` clauses
        let exported_names = self.find_named_exports(&root, source);
        for sym in &mut symbols {
            if exported_names.contains(&sym.name) {
                sym.exported = true;
            }
        }

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
        // Look for comment nodes before this node
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "comment" {
                let comment = node_text(&prev, source);
                // Check for JSDoc style comments
                if comment.starts_with("/**") {
                    return Some(Self::clean_jsdoc(comment));
                }
                // Or regular // comments
                if let Some(rest) = comment.strip_prefix("//") {
                    return Some(rest.trim().to_string());
                }
            }
        }
        None
    }
}

impl TypeScriptExtractor {
    fn extract_symbols_recursive(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        match node.kind() {
            // Function declarations
            "function_declaration" => {
                if let Some(sym) = self.extract_function(node, source, parent) {
                    symbols.push(sym);
                }
            }

            // Arrow functions assigned to const/let
            "lexical_declaration" | "variable_declaration" => {
                self.extract_variable_symbols(node, source, symbols, parent);
            }

            // Class declarations
            "class_declaration" => {
                if let Some(sym) = self.extract_class(node, source, parent) {
                    let class_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract class body
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_class_members(&body, source, symbols, Some(&class_name));
                    }
                }
            }

            // Interface declarations
            "interface_declaration" => {
                if let Some(sym) = self.extract_interface(node, source, parent) {
                    symbols.push(sym);
                }
            }

            // Type alias declarations
            "type_alias_declaration" => {
                if let Some(sym) = self.extract_type_alias(node, source, parent) {
                    symbols.push(sym);
                }
            }

            // Enum declarations
            "enum_declaration" => {
                if let Some(sym) = self.extract_enum(node, source, parent) {
                    symbols.push(sym);
                }
            }

            // Export statements
            "export_statement" => {
                self.extract_export_symbols(node, source, symbols, parent);
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
            name,
            SymbolKind::Function,
            node.start_position().row + 1,
            node.end_position().row + 1,
        )
        .with_columns(node.start_position().column, node.end_position().column);

        // Check for async
        let text = node_text(node, source);
        if text.starts_with("async") {
            sym = sym.async_fn();
        }

        // Extract parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        // Extract return type
        if let Some(ret_type) = node.child_by_field_name("return_type") {
            sym.return_type = Some(node_text(&ret_type, source).trim_start_matches(':').trim().to_string());
        }

        // Extract doc comment
        sym.doc_comment = self.extract_doc_comment(node, source);

        // Set parent
        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        // Build signature
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

                    // Check if this is an arrow function or function expression
                    if value_kind == "arrow_function" || value_kind == "function_expression" {
                        let mut sym = ExtractedSymbol::new(
                            name.to_string(),
                            SymbolKind::Function,
                            node.start_position().row + 1,
                            node.end_position().row + 1,
                        );

                        // Check for async
                        let text = node_text(&value, source);
                        if text.starts_with("async") {
                            sym = sym.async_fn();
                        }

                        // Extract parameters
                        if let Some(params) = value.child_by_field_name("parameters") {
                            self.extract_parameters(&params, source, &mut sym);
                        }

                        // Extract return type
                        if let Some(ret_type) = value.child_by_field_name("return_type") {
                            sym.return_type = Some(
                                node_text(&ret_type, source).trim_start_matches(':').trim().to_string()
                            );
                        }

                        sym.doc_comment = self.extract_doc_comment(node, source);

                        if let Some(p) = parent {
                            sym = sym.with_parent(p);
                        }

                        // Check if exported
                        if let Some(parent_node) = node.parent() {
                            if parent_node.kind() == "export_statement" {
                                sym = sym.exported();
                            }
                        }

                        symbols.push(sym);
                    } else {
                        // Regular variable/constant
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

                        // Extract type annotation
                        if let Some(type_ann) = child.child_by_field_name("type") {
                            sym.type_info = Some(
                                node_text(&type_ann, source).trim_start_matches(':').trim().to_string()
                            );
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

    fn extract_class(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Class,
            node.start_position().row + 1,
            node.end_position().row + 1,
        )
        .with_columns(node.start_position().column, node.end_position().column);

        // Extract generics
        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut sym);
        }

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
                "method_definition" | "public_field_definition" => {
                    if let Some(sym) = self.extract_method(&child, source, class_name) {
                        symbols.push(sym);
                    }
                }
                "property_signature" => {
                    if let Some(sym) = self.extract_property(&child, source, class_name) {
                        symbols.push(sym);
                    }
                }
                _ => {}
            }
        }
    }

    fn extract_method(&self, node: &Node, source: &str, class_name: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Method,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        // Check visibility modifiers
        let text = node_text(node, source);
        if text.contains("private") {
            sym.visibility = Visibility::Private;
        } else if text.contains("protected") {
            sym.visibility = Visibility::Protected;
        }

        // Check for static
        if text.contains("static") {
            sym = sym.static_fn();
        }

        // Check for async
        if text.contains("async") {
            sym = sym.async_fn();
        }

        // Extract parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        // Extract return type
        if let Some(ret_type) = node.child_by_field_name("return_type") {
            sym.return_type = Some(node_text(&ret_type, source).trim_start_matches(':').trim().to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = class_name {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_property(&self, node: &Node, source: &str, class_name: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Property,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        // Extract type
        if let Some(type_node) = node.child_by_field_name("type") {
            sym.type_info = Some(node_text(&type_node, source).trim_start_matches(':').trim().to_string());
        }

        if let Some(p) = class_name {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_interface(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Interface,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        // Extract generics
        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut sym);
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_type_alias(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::TypeAlias,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        // Extract the type value
        if let Some(type_value) = node.child_by_field_name("value") {
            sym.type_info = Some(node_text(&type_value, source).to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_enum(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Enum,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
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
                "interface_declaration" => {
                    if let Some(mut sym) = self.extract_interface(&child, source, parent) {
                        sym = sym.exported();
                        symbols.push(sym);
                    }
                }
                "type_alias_declaration" => {
                    if let Some(mut sym) = self.extract_type_alias(&child, source, parent) {
                        sym = sym.exported();
                        symbols.push(sym);
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    self.extract_variable_symbols(&child, source, symbols, parent);
                    // Mark as exported
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
                "required_parameter" | "optional_parameter" => {
                    let is_optional = child.kind() == "optional_parameter";

                    let name = child.child_by_field_name("pattern")
                        .or_else(|| child.child_by_field_name("name"))
                        .map(|n| node_text(&n, source).to_string())
                        .unwrap_or_default();

                    let type_info = child.child_by_field_name("type")
                        .map(|n| node_text(&n, source).trim_start_matches(':').trim().to_string());

                    let default_value = child.child_by_field_name("value")
                        .map(|n| node_text(&n, source).to_string());

                    sym.add_parameter(Parameter {
                        name,
                        type_info,
                        default_value,
                        is_rest: false,
                        is_optional,
                    });
                }
                "rest_parameter" => {
                    let name = child.child_by_field_name("pattern")
                        .or_else(|| child.child_by_field_name("name"))
                        .map(|n| node_text(&n, source).trim_start_matches("...").to_string())
                        .unwrap_or_default();

                    let type_info = child.child_by_field_name("type")
                        .map(|n| node_text(&n, source).trim_start_matches(':').trim().to_string());

                    sym.add_parameter(Parameter {
                        name,
                        type_info,
                        default_value: None,
                        is_rest: true,
                        is_optional: false,
                    });
                }
                _ => {}
            }
        }
    }

    fn extract_generics(&self, type_params: &Node, source: &str, sym: &mut ExtractedSymbol) {
        let mut cursor = type_params.walk();
        for child in type_params.children(&mut cursor) {
            if child.kind() == "type_parameter" {
                if let Some(name) = child.child_by_field_name("name") {
                    sym.add_generic(node_text(&name, source));
                }
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

        // Parse import clause
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
                    // Default import
                    import.is_default = true;
                    import.names.push(ImportedName {
                        name: "default".to_string(),
                        alias: Some(node_text(&child, source).to_string()),
                    });
                }
                "namespace_import" => {
                    // import * as foo
                    import.is_namespace = true;
                    if let Some(name_node) = child.child_by_field_name("name") {
                        import.names.push(ImportedName {
                            name: "*".to_string(),
                            alias: Some(node_text(&name_node, source).to_string()),
                        });
                    }
                }
                "named_imports" => {
                    // import { foo, bar as baz }
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
                let name = child.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_default();

                let alias = child.child_by_field_name("alias")
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

        // Track current function for nested calls
        let func_name = match node.kind() {
            "function_declaration" | "method_definition" => {
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
            "member_expression" => {
                // Method call: obj.method()
                let object = function.child_by_field_name("object")
                    .map(|n| node_text(&n, source).to_string());
                let property = function.child_by_field_name("property")
                    .map(|n| node_text(&n, source).to_string())?;
                (property, true, object)
            }
            "identifier" => {
                // Direct function call: foo()
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
        let name = node.child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or("anonymous");

        let params = node.child_by_field_name("parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("()");

        let return_type = node.child_by_field_name("return_type")
            .map(|n| node_text(&n, source))
            .unwrap_or("");

        format!("function {}{}{}",
            name,
            params,
            return_type
        )
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

    /// Find all names exported via `export { name1, name2 }` clauses
    fn find_named_exports(&self, node: &Node, source: &str) -> std::collections::HashSet<String> {
        let mut exports = std::collections::HashSet::new();
        self.collect_named_exports(node, source, &mut exports);
        exports
    }

    fn collect_named_exports(
        &self,
        node: &Node,
        source: &str,
        exports: &mut std::collections::HashSet<String>,
    ) {
        // Handle export statements with export_clause (e.g., `export { Button, Card as CardComponent }`)
        if node.kind() == "export_statement" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "export_clause" {
                    self.parse_export_clause(&child, source, exports);
                }
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_named_exports(&child, source, exports);
        }
    }

    fn parse_export_clause(
        &self,
        node: &Node,
        source: &str,
        exports: &mut std::collections::HashSet<String>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // export_specifier: name, or name as alias
            if child.kind() == "export_specifier" {
                // Get the local name (the original identifier, not the alias)
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();
                    exports.insert(name);
                } else {
                    // Fallback: get the first identifier
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        if inner_child.kind() == "identifier" {
                            let name = node_text(&inner_child, source).to_string();
                            exports.insert(name);
                            break;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ts(source: &str) -> (Tree, String) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        (tree, source.to_string())
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
        let (tree, src) = parse_ts(source);
        let extractor = TypeScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "greet");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].parameters.len(), 1);
        assert_eq!(symbols[0].parameters[0].name, "name");
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
class UserService {
    private name: string;

    constructor(name: string) {
        this.name = name;
    }

    public greet(): string {
        return `Hello, ${this.name}!`;
    }
}
"#;
        let (tree, src) = parse_ts(source);
        let extractor = TypeScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        // Should have: class, constructor, greet method
        assert!(symbols.iter().any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_extract_interface() {
        let source = r#"
interface User<T> {
    name: string;
    age: number;
    data: T;
}
"#;
        let (tree, src) = parse_ts(source);
        let extractor = TypeScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::Interface);
        assert!(symbols[0].generics.contains(&"T".to_string()));
    }

    #[test]
    fn test_extract_arrow_function() {
        let source = r#"
const add = (a: number, b: number): number => a + b;
"#;
        let (tree, src) = parse_ts(source);
        let extractor = TypeScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "add");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].parameters.len(), 2);
    }

    #[test]
    fn test_extract_imports() {
        let source = r#"
import { foo, bar as baz } from './module';
import * as utils from 'utils';
import defaultExport from './default';
"#;
        let (tree, src) = parse_ts(source);
        let extractor = TypeScriptExtractor;
        let imports = extractor.extract_imports(&tree, &src).unwrap();

        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].source, "./module");
        assert_eq!(imports[1].source, "utils");
        assert!(imports[1].is_namespace);
        assert_eq!(imports[2].source, "./default");
        assert!(imports[2].is_default);
    }

    #[test]
    fn test_named_export_clause() {
        // Test that symbols exported via `export { name }` are marked as exported
        let source = r#"
function Button() {
    return <button>Click me</button>;
}

const buttonVariants = {};

export { Button, buttonVariants };
"#;
        let (tree, src) = parse_ts(source);
        let extractor = TypeScriptExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        // Button should be marked as exported
        let button = symbols.iter().find(|s| s.name == "Button").expect("Button not found");
        assert!(button.exported, "Button should be marked as exported");

        // buttonVariants should be marked as exported
        let variants = symbols.iter().find(|s| s.name == "buttonVariants").expect("buttonVariants not found");
        assert!(variants.exported, "buttonVariants should be marked as exported");
    }
}

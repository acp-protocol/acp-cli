//! @acp:module "Rust Extractor"
//! @acp:summary "Symbol extraction for Rust source files"
//! @acp:domain cli
//! @acp:layer parsing

use tree_sitter::{Language, Tree, Node};
use crate::error::Result;
use super::{LanguageExtractor, node_text};
use crate::ast::{ExtractedSymbol, Import, ImportedName, FunctionCall, Parameter, SymbolKind, Visibility};

/// Rust language extractor
pub struct RustExtractor;

impl LanguageExtractor for RustExtractor {
    fn language(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn name(&self) -> &'static str {
        "rust"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
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
        // Look for doc comments (///, //!, /** */)
        let mut doc_lines = Vec::new();
        let mut current = node.prev_sibling();

        while let Some(prev) = current {
            if prev.kind() == "line_comment" {
                let comment = node_text(&prev, source);
                if comment.starts_with("///") || comment.starts_with("//!") {
                    doc_lines.push(comment[3..].trim().to_string());
                    current = prev.prev_sibling();
                    continue;
                }
            } else if prev.kind() == "block_comment" {
                let comment = node_text(&prev, source);
                if comment.starts_with("/**") || comment.starts_with("/*!") {
                    return Some(Self::clean_block_doc(comment));
                }
            }
            break;
        }

        if doc_lines.is_empty() {
            None
        } else {
            doc_lines.reverse();
            Some(doc_lines.join("\n"))
        }
    }
}

impl RustExtractor {
    fn extract_symbols_recursive(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        match node.kind() {
            "function_item" => {
                if let Some(sym) = self.extract_function(node, source, parent) {
                    symbols.push(sym);
                }
            }

            "struct_item" => {
                if let Some(sym) = self.extract_struct(node, source, parent) {
                    let struct_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract struct fields
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_struct_fields(&body, source, symbols, Some(&struct_name));
                    }
                }
            }

            "enum_item" => {
                if let Some(sym) = self.extract_enum(node, source, parent) {
                    let enum_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract enum variants
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_enum_variants(&body, source, symbols, Some(&enum_name));
                    }
                }
            }

            "trait_item" => {
                if let Some(sym) = self.extract_trait(node, source, parent) {
                    let trait_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract trait methods
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_trait_body(&body, source, symbols, Some(&trait_name));
                    }
                }
            }

            "impl_item" => {
                self.extract_impl_block(node, source, symbols);
            }

            "type_item" => {
                if let Some(sym) = self.extract_type_alias(node, source, parent) {
                    symbols.push(sym);
                }
            }

            "const_item" => {
                if let Some(sym) = self.extract_const(node, source, parent) {
                    symbols.push(sym);
                }
            }

            "static_item" => {
                if let Some(sym) = self.extract_static(node, source, parent) {
                    symbols.push(sym);
                }
            }

            "mod_item" => {
                if let Some(sym) = self.extract_module(node, source, parent) {
                    let mod_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract module contents
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_symbols_recursive(&body, source, symbols, Some(&mod_name));
                    }
                }
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

        // Check visibility
        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        // Check for async
        let text = node_text(node, source);
        if text.contains("async fn") {
            sym = sym.async_fn();
        }

        // Extract generics
        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut sym);
        }

        // Extract parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        // Extract return type
        if let Some(ret_type) = node.child_by_field_name("return_type") {
            sym.return_type = Some(node_text(&ret_type, source).trim_start_matches("->").trim().to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        sym.signature = Some(self.build_function_signature(node, source));

        Some(sym)
    }

    fn extract_struct(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Struct,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut sym);
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_struct_fields(
        &self,
        body: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        struct_name: Option<&str>,
    ) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "field_declaration" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();

                    let mut sym = ExtractedSymbol::new(
                        name,
                        SymbolKind::Field,
                        child.start_position().row + 1,
                        child.end_position().row + 1,
                    );

                    sym.visibility = self.extract_visibility(&child, source);

                    if let Some(type_node) = child.child_by_field_name("type") {
                        sym.type_info = Some(node_text(&type_node, source).to_string());
                    }

                    if let Some(p) = struct_name {
                        sym = sym.with_parent(p);
                    }

                    symbols.push(sym);
                }
            }
        }
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

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut sym);
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_enum_variants(
        &self,
        body: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        enum_name: Option<&str>,
    ) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "enum_variant" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();

                    let mut sym = ExtractedSymbol::new(
                        name,
                        SymbolKind::EnumVariant,
                        child.start_position().row + 1,
                        child.end_position().row + 1,
                    );

                    if let Some(p) = enum_name {
                        sym = sym.with_parent(p);
                    }

                    symbols.push(sym);
                }
            }
        }
    }

    fn extract_trait(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Trait,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut sym);
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_trait_body(
        &self,
        body: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        trait_name: Option<&str>,
    ) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "function_signature_item" || child.kind() == "function_item" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();

                    let mut sym = ExtractedSymbol::new(
                        name,
                        SymbolKind::Method,
                        child.start_position().row + 1,
                        child.end_position().row + 1,
                    );

                    if let Some(params) = child.child_by_field_name("parameters") {
                        self.extract_parameters(&params, source, &mut sym);
                    }

                    if let Some(ret_type) = child.child_by_field_name("return_type") {
                        sym.return_type = Some(node_text(&ret_type, source).trim_start_matches("->").trim().to_string());
                    }

                    sym.doc_comment = self.extract_doc_comment(&child, source);

                    if let Some(p) = trait_name {
                        sym = sym.with_parent(p);
                    }

                    symbols.push(sym);
                }
            }
        }
    }

    fn extract_impl_block(&self, node: &Node, source: &str, symbols: &mut Vec<ExtractedSymbol>) {
        // Get the type being implemented
        let type_name = node.child_by_field_name("type")
            .map(|n| node_text(&n, source).to_string())
            .unwrap_or_default();

        // Check if this is a trait impl
        let trait_name = node.child_by_field_name("trait")
            .map(|n| node_text(&n, source).to_string());

        let impl_name = if let Some(ref trait_n) = trait_name {
            format!("{} for {}", trait_n, type_name)
        } else {
            type_name.clone()
        };

        // Create impl block symbol
        let mut impl_sym = ExtractedSymbol::new(
            impl_name.clone(),
            SymbolKind::Impl,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut impl_sym);
        }

        symbols.push(impl_sym);

        // Extract methods in the impl block
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "function_item" {
                    if let Some(mut sym) = self.extract_function(&child, source, Some(&type_name)) {
                        // Check if it's a method (has self parameter)
                        if sym.parameters.iter().any(|p| p.name.contains("self")) {
                            sym.kind = SymbolKind::Method;
                        }
                        symbols.push(sym);
                    }
                }
            }
        }
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

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        if let Some(type_node) = node.child_by_field_name("type") {
            sym.type_info = Some(node_text(&type_node, source).to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_const(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Constant,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        if let Some(type_node) = node.child_by_field_name("type") {
            sym.type_info = Some(node_text(&type_node, source).to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_static(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Variable,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        sym.is_static = true;

        if let Some(type_node) = node.child_by_field_name("type") {
            sym.type_info = Some(node_text(&type_node, source).to_string());
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_module(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Module,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_visibility(&self, node: &Node, source: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let text = node_text(&child, source);
                if text == "pub" {
                    return Visibility::Public;
                } else if text.contains("pub(crate)") {
                    return Visibility::Crate;
                } else if text.contains("pub(super)") || text.contains("pub(self)") {
                    return Visibility::Internal;
                }
            }
        }
        Visibility::Private
    }

    fn extract_parameters(&self, params: &Node, source: &str, sym: &mut ExtractedSymbol) {
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            match child.kind() {
                "parameter" => {
                    let pattern = child.child_by_field_name("pattern")
                        .map(|n| node_text(&n, source).to_string())
                        .unwrap_or_default();

                    let type_info = child.child_by_field_name("type")
                        .map(|n| node_text(&n, source).to_string());

                    sym.add_parameter(Parameter {
                        name: pattern,
                        type_info,
                        default_value: None,
                        is_rest: false,
                        is_optional: false,
                    });
                }
                "self_parameter" => {
                    sym.add_parameter(Parameter {
                        name: node_text(&child, source).to_string(),
                        type_info: Some("Self".to_string()),
                        default_value: None,
                        is_rest: false,
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
            match child.kind() {
                "type_identifier" | "lifetime" => {
                    sym.add_generic(node_text(&child, source));
                }
                "constrained_type_parameter" => {
                    if let Some(name) = child.child_by_field_name("left") {
                        sym.add_generic(node_text(&name, source));
                    }
                }
                _ => {}
            }
        }
    }

    fn extract_imports_recursive(&self, node: &Node, source: &str, imports: &mut Vec<Import>) {
        if node.kind() == "use_declaration" {
            if let Some(import) = self.parse_use(node, source) {
                imports.push(import);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_imports_recursive(&child, source, imports);
        }
    }

    fn parse_use(&self, node: &Node, source: &str) -> Option<Import> {
        let argument = node.child_by_field_name("argument")?;

        let mut import = Import {
            source: String::new(),
            names: Vec::new(),
            is_default: false,
            is_namespace: false,
            line: node.start_position().row + 1,
        };

        self.parse_use_path(&argument, source, &mut import, String::new());

        Some(import)
    }

    fn parse_use_path(&self, node: &Node, source: &str, import: &mut Import, prefix: String) {
        match node.kind() {
            "scoped_identifier" => {
                let path = node.child_by_field_name("path")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_default();
                let name = node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_default();

                let full_path = if prefix.is_empty() {
                    path
                } else {
                    format!("{}::{}", prefix, path)
                };

                import.source = full_path;
                import.names.push(ImportedName {
                    name,
                    alias: None,
                });
            }
            "use_as_clause" => {
                let path = node.child_by_field_name("path")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_default();
                let alias = node.child_by_field_name("alias")
                    .map(|n| node_text(&n, source).to_string());

                import.source = prefix;
                import.names.push(ImportedName {
                    name: path,
                    alias,
                });
            }
            "use_wildcard" => {
                import.source = prefix;
                import.is_namespace = true;
                import.names.push(ImportedName {
                    name: "*".to_string(),
                    alias: None,
                });
            }
            "use_list" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.parse_use_path(&child, source, import, prefix.clone());
                }
            }
            "identifier" => {
                let name = node_text(node, source).to_string();
                if import.source.is_empty() {
                    import.source = prefix;
                }
                import.names.push(ImportedName {
                    name,
                    alias: None,
                });
            }
            _ => {}
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

        let func_name = if node.kind() == "function_item" {
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
            "field_expression" => {
                let value = function.child_by_field_name("value")
                    .map(|n| node_text(&n, source).to_string());
                let field = function.child_by_field_name("field")
                    .map(|n| node_text(&n, source).to_string())?;
                (field, true, value)
            }
            "scoped_identifier" => {
                let name = function.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_else(|| node_text(&function, source).to_string());
                (name, false, None)
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
        let vis = node.children(&mut node.walk())
            .find(|c| c.kind() == "visibility_modifier")
            .map(|n| format!("{} ", node_text(&n, source)))
            .unwrap_or_default();

        let async_kw = if node_text(node, source).contains("async fn") {
            "async "
        } else {
            ""
        };

        let name = node.child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or("unknown");

        let generics = node.child_by_field_name("type_parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("");

        let params = node.child_by_field_name("parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("()");

        let return_type = node.child_by_field_name("return_type")
            .map(|n| format!(" {}", node_text(&n, source)))
            .unwrap_or_default();

        format!("{}{}fn {}{}{}{}",
            vis,
            async_kw,
            name,
            generics,
            params,
            return_type
        )
    }

    fn clean_block_doc(comment: &str) -> String {
        comment
            .trim_start_matches("/**")
            .trim_start_matches("/*!")
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

    fn parse_rs(source: &str) -> (Tree, String) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        (tree, source.to_string())
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;
        let (tree, src) = parse_rs(source);
        let extractor = RustExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "greet");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert!(symbols[0].exported);
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
pub struct User {
    pub name: String,
    age: u32,
}
"#;
        let (tree, src) = parse_rs(source);
        let extractor = RustExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "User" && s.kind == SymbolKind::Struct));
        assert!(symbols.iter().any(|s| s.name == "name" && s.kind == SymbolKind::Field));
        assert!(symbols.iter().any(|s| s.name == "age" && s.kind == SymbolKind::Field));
    }

    #[test]
    fn test_extract_impl() {
        let source = r#"
impl User {
    pub fn new(name: String) -> Self {
        Self { name, age: 0 }
    }

    pub fn greet(&self) -> String {
        format!("Hello, {}!", self.name)
    }
}
"#;
        let (tree, src) = parse_rs(source);
        let extractor = RustExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "User" && s.kind == SymbolKind::Impl));
        assert!(symbols.iter().any(|s| s.name == "new" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_extract_trait() {
        let source = r#"
pub trait Greeter {
    fn greet(&self) -> String;
    fn farewell(&self) -> String {
        "Goodbye!".to_string()
    }
}
"#;
        let (tree, src) = parse_rs(source);
        let extractor = RustExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Greeter" && s.kind == SymbolKind::Trait));
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
pub enum Status {
    Active,
    Inactive,
    Pending(String),
}
"#;
        let (tree, src) = parse_rs(source);
        let extractor = RustExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Status" && s.kind == SymbolKind::Enum));
        assert!(symbols.iter().any(|s| s.name == "Active" && s.kind == SymbolKind::EnumVariant));
    }
}

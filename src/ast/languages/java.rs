//! @acp:module "Java Extractor"
//! @acp:summary "Symbol extraction for Java source files"
//! @acp:domain cli
//! @acp:layer parsing

use tree_sitter::{Language, Tree, Node};
use crate::error::Result;
use super::{LanguageExtractor, node_text};
use crate::ast::{ExtractedSymbol, Import, ImportedName, FunctionCall, Parameter, SymbolKind, Visibility};

/// Java language extractor
pub struct JavaExtractor;

impl LanguageExtractor for JavaExtractor {
    fn language(&self) -> Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn name(&self) -> &'static str {
        "java"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["java"]
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
        // Look for Javadoc comments
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "block_comment" || prev.kind() == "line_comment" {
                let comment = node_text(&prev, source);
                if comment.starts_with("/**") {
                    return Some(Self::clean_javadoc(comment));
                }
            }
        }
        None
    }
}

impl JavaExtractor {
    fn extract_symbols_recursive(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        match node.kind() {
            "class_declaration" => {
                if let Some(sym) = self.extract_class(node, source, parent) {
                    let class_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract class body
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_class_members(&body, source, symbols, Some(&class_name));
                    }
                    return;
                }
            }

            "interface_declaration" => {
                if let Some(sym) = self.extract_interface(node, source, parent) {
                    let interface_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract interface body
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_interface_members(&body, source, symbols, Some(&interface_name));
                    }
                    return;
                }
            }

            "enum_declaration" => {
                if let Some(sym) = self.extract_enum(node, source, parent) {
                    let enum_name = sym.name.clone();
                    symbols.push(sym);

                    // Extract enum body
                    if let Some(body) = node.child_by_field_name("body") {
                        self.extract_enum_constants(&body, source, symbols, Some(&enum_name));
                    }
                    return;
                }
            }

            "method_declaration" | "constructor_declaration" => {
                if let Some(sym) = self.extract_method(node, source, parent) {
                    symbols.push(sym);
                }
            }

            "field_declaration" => {
                self.extract_fields(node, source, symbols, parent);
            }

            _ => {}
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_symbols_recursive(&child, source, symbols, parent);
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

        // Extract modifiers for visibility
        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        // Check for static
        let text = node_text(node, source);
        if text.contains("static") {
            sym = sym.static_fn();
        }

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

    fn extract_interface(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Interface,
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

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        Some(sym)
    }

    fn extract_method(&self, node: &Node, source: &str, parent: Option<&str>) -> Option<ExtractedSymbol> {
        let is_constructor = node.kind() == "constructor_declaration";

        let name = if is_constructor {
            // Constructor name is same as class name
            parent.map(String::from)?
        } else {
            let name_node = node.child_by_field_name("name")?;
            node_text(&name_node, source).to_string()
        };

        let mut sym = ExtractedSymbol::new(
            name,
            SymbolKind::Method,
            node.start_position().row + 1,
            node.end_position().row + 1,
        );

        sym.visibility = self.extract_visibility(node, source);
        if matches!(sym.visibility, Visibility::Public) {
            sym = sym.exported();
        }

        // Check for static
        let text = node_text(node, source);
        if text.contains("static ") {
            sym = sym.static_fn();
        }

        // Extract generics
        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            self.extract_generics(&type_params, source, &mut sym);
        }

        // Extract parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_parameters(&params, source, &mut sym);
        }

        // Extract return type (not for constructors)
        if !is_constructor {
            if let Some(ret_type) = node.child_by_field_name("type") {
                sym.return_type = Some(node_text(&ret_type, source).to_string());
            }
        }

        sym.doc_comment = self.extract_doc_comment(node, source);

        if let Some(p) = parent {
            sym = sym.with_parent(p);
        }

        sym.signature = Some(self.build_method_signature(node, source, is_constructor));

        Some(sym)
    }

    fn extract_fields(
        &self,
        node: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        parent: Option<&str>,
    ) {
        let visibility = self.extract_visibility(node, source);
        let is_static = node_text(node, source).contains("static ");

        let type_node = node.child_by_field_name("type");
        let type_info = type_node.map(|n| node_text(&n, source).to_string());

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();

                    let mut sym = ExtractedSymbol::new(
                        name,
                        SymbolKind::Field,
                        child.start_position().row + 1,
                        child.end_position().row + 1,
                    );

                    sym.visibility = visibility;
                    sym.type_info = type_info.clone();

                    if is_static {
                        sym = sym.static_fn();
                    }

                    if let Some(p) = parent {
                        sym = sym.with_parent(p);
                    }

                    symbols.push(sym);
                }
            }
        }
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
                "method_declaration" | "constructor_declaration" => {
                    if let Some(sym) = self.extract_method(&child, source, class_name) {
                        symbols.push(sym);
                    }
                }
                "field_declaration" => {
                    self.extract_fields(&child, source, symbols, class_name);
                }
                "class_declaration" => {
                    // Nested class
                    if let Some(sym) = self.extract_class(&child, source, class_name) {
                        let nested_name = sym.name.clone();
                        symbols.push(sym);

                        if let Some(nested_body) = child.child_by_field_name("body") {
                            self.extract_class_members(&nested_body, source, symbols, Some(&nested_name));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn extract_interface_members(
        &self,
        body: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        interface_name: Option<&str>,
    ) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "method_declaration" {
                if let Some(sym) = self.extract_method(&child, source, interface_name) {
                    symbols.push(sym);
                }
            } else if child.kind() == "constant_declaration" {
                self.extract_fields(&child, source, symbols, interface_name);
            }
        }
    }

    fn extract_enum_constants(
        &self,
        body: &Node,
        source: &str,
        symbols: &mut Vec<ExtractedSymbol>,
        enum_name: Option<&str>,
    ) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "enum_constant" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();

                    let mut sym = ExtractedSymbol::new(
                        name,
                        SymbolKind::EnumVariant,
                        child.start_position().row + 1,
                        child.end_position().row + 1,
                    );

                    sym.visibility = Visibility::Public;
                    sym = sym.exported();

                    if let Some(p) = enum_name {
                        sym = sym.with_parent(p);
                    }

                    symbols.push(sym);
                }
            }
        }
    }

    fn extract_visibility(&self, node: &Node, source: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "modifiers" {
                let text = node_text(&child, source);
                if text.contains("public") {
                    return Visibility::Public;
                } else if text.contains("private") {
                    return Visibility::Private;
                } else if text.contains("protected") {
                    return Visibility::Protected;
                }
                return Visibility::Internal; // package-private
            }
        }
        Visibility::Internal // default is package-private
    }

    fn extract_parameters(&self, params: &Node, source: &str, sym: &mut ExtractedSymbol) {
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            if child.kind() == "formal_parameter" || child.kind() == "spread_parameter" {
                let is_rest = child.kind() == "spread_parameter";

                let name = child.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
                    .unwrap_or_default();

                let type_info = child.child_by_field_name("type")
                    .map(|n| node_text(&n, source).to_string());

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

    fn extract_generics(&self, type_params: &Node, source: &str, sym: &mut ExtractedSymbol) {
        let mut cursor = type_params.walk();
        for child in type_params.children(&mut cursor) {
            if child.kind() == "type_parameter" {
                if let Some(name) = child.child_by_field_name("name") {
                    sym.add_generic(node_text(&name, source));
                } else {
                    // Fallback to first identifier
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() == "type_identifier" || inner.kind() == "identifier" {
                            sym.add_generic(node_text(&inner, source));
                            break;
                        }
                    }
                }
            }
        }
    }

    fn extract_imports_recursive(&self, node: &Node, source: &str, imports: &mut Vec<Import>) {
        if node.kind() == "import_declaration" {
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
        let text = node_text(node, source);

        // Check for wildcard import
        let is_wildcard = text.contains(".*");
        let _is_static = text.contains("static ");

        let path = text
            .trim_start_matches("import ")
            .trim_start_matches("static ")
            .trim_end_matches(';')
            .trim()
            .trim_end_matches(".*")
            .to_string();

        let (source_path, name) = if is_wildcard {
            (path.clone(), "*".to_string())
        } else {
            // Split package.Class into package and class
            let parts: Vec<&str> = path.rsplitn(2, '.').collect();
            if parts.len() == 2 {
                (parts[1].to_string(), parts[0].to_string())
            } else {
                (String::new(), path)
            }
        };

        Some(Import {
            source: source_path,
            names: vec![ImportedName {
                name,
                alias: None,
            }],
            is_default: false,
            is_namespace: is_wildcard,
            line: node.start_position().row + 1,
        })
    }

    fn extract_calls_recursive(
        &self,
        node: &Node,
        source: &str,
        calls: &mut Vec<FunctionCall>,
        current_function: Option<&str>,
    ) {
        if node.kind() == "method_invocation" {
            if let Some(call) = self.parse_call(node, source, current_function) {
                calls.push(call);
            }
        }

        let func_name = match node.kind() {
            "method_declaration" | "constructor_declaration" => {
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
        let name = node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())?;

        let object = node.child_by_field_name("object")
            .map(|n| node_text(&n, source).to_string());

        Some(FunctionCall {
            caller: current_function.unwrap_or("<class>").to_string(),
            callee: name,
            line: node.start_position().row + 1,
            is_method: object.is_some(),
            receiver: object,
        })
    }

    fn build_method_signature(&self, node: &Node, source: &str, is_constructor: bool) -> String {
        let modifiers = node.children(&mut node.walk())
            .find(|c| c.kind() == "modifiers")
            .map(|n| format!("{} ", node_text(&n, source)))
            .unwrap_or_default();

        let return_type = if is_constructor {
            String::new()
        } else {
            node.child_by_field_name("type")
                .map(|n| format!("{} ", node_text(&n, source)))
                .unwrap_or_default()
        };

        let name = node.child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or("unknown");

        let params = node.child_by_field_name("parameters")
            .map(|n| node_text(&n, source))
            .unwrap_or("()");

        format!("{}{}{}{}",
            modifiers,
            return_type,
            name,
            params
        )
    }

    fn clean_javadoc(comment: &str) -> String {
        comment
            .trim_start_matches("/**")
            .trim_end_matches("*/")
            .lines()
            .map(|line| line.trim().trim_start_matches('*').trim())
            .filter(|line| !line.is_empty() && !line.starts_with('@'))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_java(source: &str) -> (Tree, String) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        (tree, source.to_string())
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
public class UserService {
    private String name;

    public UserService(String name) {
        this.name = name;
    }

    public String greet() {
        return "Hello, " + name + "!";
    }
}
"#;
        let (tree, src) = parse_java(source);
        let extractor = JavaExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "name" && s.kind == SymbolKind::Field));
        assert!(symbols.iter().any(|s| s.name == "UserService" && s.kind == SymbolKind::Method)); // constructor
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_extract_interface() {
        let source = r#"
public interface Greeter {
    String greet();
    String farewell();
}
"#;
        let (tree, src) = parse_java(source);
        let extractor = JavaExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Greeter" && s.kind == SymbolKind::Interface));
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
public enum Status {
    ACTIVE,
    INACTIVE,
    PENDING
}
"#;
        let (tree, src) = parse_java(source);
        let extractor = JavaExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Status" && s.kind == SymbolKind::Enum));
        assert!(symbols.iter().any(|s| s.name == "ACTIVE" && s.kind == SymbolKind::EnumVariant));
    }

    #[test]
    fn test_extract_generics() {
        let source = r#"
public class Container<T> {
    private T value;

    public T getValue() {
        return value;
    }
}
"#;
        let (tree, src) = parse_java(source);
        let extractor = JavaExtractor;
        let symbols = extractor.extract_symbols(&tree, &src).unwrap();

        let class = symbols.iter().find(|s| s.name == "Container").unwrap();
        assert!(class.generics.contains(&"T".to_string()));
    }
}

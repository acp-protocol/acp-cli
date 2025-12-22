//! @acp:module "AST Parsing"
//! @acp:summary "Tree-sitter based AST parsing for symbol extraction"
//! @acp:domain cli
//! @acp:layer parsing
//!
//! # AST Parsing
//!
//! Provides tree-sitter based AST parsing for extracting symbols from source code:
//! - Functions, methods, classes, structs
//! - Interfaces, traits, enums, type aliases
//! - Import/export statements
//! - Function calls for call graph analysis

pub mod parser;
pub mod languages;

pub use parser::AstParser;
pub use languages::{LanguageExtractor, get_extractor};

use serde::{Deserialize, Serialize};

/// Symbol kinds extracted from AST
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// A function declaration
    Function,
    /// A method (function inside a class/struct/impl)
    Method,
    /// A class declaration
    Class,
    /// A struct declaration
    Struct,
    /// An interface declaration
    Interface,
    /// A trait declaration (Rust)
    Trait,
    /// An enum declaration
    Enum,
    /// An enum variant/member
    EnumVariant,
    /// A constant declaration
    Constant,
    /// A variable declaration
    Variable,
    /// A type alias
    TypeAlias,
    /// A module declaration
    Module,
    /// A namespace declaration
    Namespace,
    /// A property (in class/object)
    Property,
    /// A field (in struct)
    Field,
    /// An impl block (Rust)
    Impl,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "function"),
            Self::Method => write!(f, "method"),
            Self::Class => write!(f, "class"),
            Self::Struct => write!(f, "struct"),
            Self::Interface => write!(f, "interface"),
            Self::Trait => write!(f, "trait"),
            Self::Enum => write!(f, "enum"),
            Self::EnumVariant => write!(f, "enum_variant"),
            Self::Constant => write!(f, "constant"),
            Self::Variable => write!(f, "variable"),
            Self::TypeAlias => write!(f, "type_alias"),
            Self::Module => write!(f, "module"),
            Self::Namespace => write!(f, "namespace"),
            Self::Property => write!(f, "property"),
            Self::Field => write!(f, "field"),
            Self::Impl => write!(f, "impl"),
        }
    }
}

/// Visibility of a symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Public visibility (default for most languages)
    #[default]
    Public,
    /// Private visibility
    Private,
    /// Protected visibility (class inheritance)
    Protected,
    /// Internal visibility (package/module level)
    Internal,
    /// Crate-level visibility (Rust pub(crate))
    Crate,
}

/// A function/method parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name
    pub name: String,
    /// Type annotation (if available)
    pub type_info: Option<String>,
    /// Default value (if available)
    pub default_value: Option<String>,
    /// Is this a rest/variadic parameter?
    pub is_rest: bool,
    /// Is this optional? (TypeScript)
    pub is_optional: bool,
}

/// A symbol extracted from AST parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedSymbol {
    /// Symbol name (e.g., "main", "UserService")
    pub name: String,
    /// Fully qualified name (e.g., "src/main.rs::main")
    pub qualified_name: Option<String>,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed)
    pub end_line: usize,
    /// Start column (0-indexed)
    pub start_col: usize,
    /// End column (0-indexed)
    pub end_col: usize,
    /// Function/method signature (for display)
    pub signature: Option<String>,
    /// Visibility modifier
    pub visibility: Visibility,
    /// Doc comment (if present)
    pub doc_comment: Option<String>,
    /// Parent symbol name (for nested symbols)
    pub parent: Option<String>,
    /// Type information (return type or declared type)
    pub type_info: Option<String>,
    /// Function/method parameters
    pub parameters: Vec<Parameter>,
    /// Return type (if available)
    pub return_type: Option<String>,
    /// Is this exported/public?
    pub exported: bool,
    /// Is this async?
    pub is_async: bool,
    /// Is this static?
    pub is_static: bool,
    /// Generic type parameters
    pub generics: Vec<String>,
}

impl ExtractedSymbol {
    /// Create a new ExtractedSymbol with required fields
    pub fn new(name: String, kind: SymbolKind, start_line: usize, end_line: usize) -> Self {
        Self {
            name,
            qualified_name: None,
            kind,
            start_line,
            end_line,
            start_col: 0,
            end_col: 0,
            signature: None,
            visibility: Visibility::default(),
            doc_comment: None,
            parent: None,
            type_info: None,
            parameters: Vec::new(),
            return_type: None,
            exported: false,
            is_async: false,
            is_static: false,
            generics: Vec::new(),
        }
    }

    /// Set the qualified name
    pub fn with_qualified_name(mut self, name: impl Into<String>) -> Self {
        self.qualified_name = Some(name.into());
        self
    }

    /// Set column positions
    pub fn with_columns(mut self, start_col: usize, end_col: usize) -> Self {
        self.start_col = start_col;
        self.end_col = end_col;
        self
    }

    /// Set the signature
    pub fn with_signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    /// Set visibility
    pub fn with_visibility(mut self, vis: Visibility) -> Self {
        self.visibility = vis;
        self
    }

    /// Set doc comment
    pub fn with_doc_comment(mut self, doc: impl Into<String>) -> Self {
        self.doc_comment = Some(doc.into());
        self
    }

    /// Set parent
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Set return type
    pub fn with_return_type(mut self, ret: impl Into<String>) -> Self {
        self.return_type = Some(ret.into());
        self
    }

    /// Mark as exported
    pub fn exported(mut self) -> Self {
        self.exported = true;
        self
    }

    /// Mark as async
    pub fn async_fn(mut self) -> Self {
        self.is_async = true;
        self
    }

    /// Mark as static
    pub fn static_fn(mut self) -> Self {
        self.is_static = true;
        self
    }

    /// Add a parameter
    pub fn add_parameter(&mut self, param: Parameter) {
        self.parameters.push(param);
    }

    /// Add a generic type parameter
    pub fn add_generic(&mut self, generic: impl Into<String>) {
        self.generics.push(generic.into());
    }
}

/// An import statement extracted from AST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    /// Source module path
    pub source: String,
    /// Imported names (empty for default/namespace imports)
    pub names: Vec<ImportedName>,
    /// Is this a default import?
    pub is_default: bool,
    /// Is this a namespace import (import * as x)?
    pub is_namespace: bool,
    /// Line number
    pub line: usize,
}

/// A single imported name
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedName {
    /// Original name
    pub name: String,
    /// Alias (if renamed)
    pub alias: Option<String>,
}

/// A function call for call graph analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function containing this call
    pub caller: String,
    /// Name of the function being called
    pub callee: String,
    /// Line number of the call
    pub line: usize,
    /// Is this a method call (x.foo())?
    pub is_method: bool,
    /// Receiver expression (for method calls)
    pub receiver: Option<String>,
}

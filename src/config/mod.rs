//! @acp:module "Configuration"
//! @acp:summary "Project configuration loading and defaults (schema-compliant)"
//! @acp:domain cli
//! @acp:layer config

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

fn default_config_schema() -> String {
    "https://acp-protocol.dev/schemas/v1/config.schema.json".to_string()
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// @acp:summary "Main ACP configuration structure (schema-compliant)"
/// @acp:lock normal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// JSON Schema URL for validation
    #[serde(rename = "$schema", default = "default_config_schema")]
    pub schema: String,

    /// ACP specification version
    #[serde(default = "default_version")]
    pub version: String,

    /// File patterns to include (glob syntax)
    #[serde(default = "default_include")]
    pub include: Vec<String>,

    /// File patterns to exclude (glob syntax)
    #[serde(default = "default_exclude")]
    pub exclude: Vec<String>,

    /// Error handling configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<ErrorHandling>,

    /// Constraint configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<ConstraintConfig>,

    /// Domain patterns for automatic classification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domains: Option<HashMap<String, DomainPatternConfig>>,

    /// Call graph generation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_graph: Option<CallGraphConfig>,

    /// Implementation limits
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<LimitsConfig>,

    // Internal CLI settings (not in schema but allowed as additional properties)
    /// Project root directory (internal)
    #[serde(default = "default_root", skip_serializing_if = "is_default_root")]
    pub root: PathBuf,

    /// Output paths configuration (internal)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputConfig>,
}

fn is_default_root(p: &std::path::Path) -> bool {
    p == std::path::Path::new(".")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: default_config_schema(),
            version: default_version(),
            include: default_include(),
            exclude: default_exclude(),
            error_handling: None,
            constraints: None,
            domains: None,
            call_graph: None,
            limits: None,
            root: default_root(),
            output: None,
        }
    }
}

impl Config {
    /// @acp:summary "Load config from .acp.config.json file"
    pub fn load<P: AsRef<std::path::Path>>(path: P) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// @acp:summary "Save config to a file"
    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> crate::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// @acp:summary "Load from default location or create default config"
    pub fn load_or_default() -> Self {
        Self::load(".acp.config.json").unwrap_or_default()
    }

    /// Get cache output path
    pub fn cache_path(&self) -> PathBuf {
        self.output.as_ref()
            .map(|o| o.cache.clone())
            .unwrap_or_else(default_cache_path)
    }

    /// Get vars output path
    pub fn vars_path(&self) -> PathBuf {
        self.output.as_ref()
            .map(|o| o.vars.clone())
            .unwrap_or_else(default_vars_path)
    }
}

fn default_root() -> PathBuf {
    PathBuf::from(".")
}

fn default_include() -> Vec<String> {
    vec![
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "**/*.js".to_string(),
        "**/*.jsx".to_string(),
        "**/*.rs".to_string(),
        "**/*.py".to_string(),
        "**/*.go".to_string(),
        "**/*.java".to_string(),
    ]
}

fn default_exclude() -> Vec<String> {
    vec![
        // Package managers
        "**/node_modules/**".to_string(),
        "**/vendor/**".to_string(),
        // Build outputs
        "**/dist/**".to_string(),
        "**/build/**".to_string(),
        "**/target/**".to_string(),
        "**/out/**".to_string(),
        // Framework-specific
        "**/.next/**".to_string(),       // Next.js
        "**/.nuxt/**".to_string(),       // Nuxt.js
        "**/.output/**".to_string(),     // Nitro/Nuxt 3
        "**/.svelte-kit/**".to_string(), // SvelteKit
        "**/.vite/**".to_string(),       // Vite
        "**/.turbo/**".to_string(),      // Turborepo
        // Cache/temp
        "**/.cache/**".to_string(),
        "**/coverage/**".to_string(),
        "**/__pycache__/**".to_string(),
        "**/.pytest_cache/**".to_string(),
        // VCS
        "**/.git/**".to_string(),
        // IDE
        "**/.idea/**".to_string(),
        "**/.vscode/**".to_string(),
    ]
}

/// @acp:summary "Error handling configuration (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandling {
    /// Error handling strictness mode
    #[serde(default = "default_strictness")]
    pub strictness: Strictness,

    /// Maximum number of errors before aborting (permissive mode only)
    #[serde(default = "default_max_errors")]
    pub max_errors: usize,

    /// Whether to automatically fix common errors
    #[serde(default)]
    pub auto_correct: bool,
}

fn default_strictness() -> Strictness {
    Strictness::Permissive
}

fn default_max_errors() -> usize {
    100
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strictness {
    Permissive,
    Strict,
}

/// @acp:summary "Constraint configuration (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintConfig {
    /// Default constraint values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<ConstraintDefaults>,

    /// Enable tracking of constraint violations
    #[serde(default)]
    pub track_violations: bool,

    /// Violation log file path
    #[serde(default = "default_audit_file")]
    pub audit_file: String,
}

fn default_audit_file() -> String {
    ".acp.violations.log".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintDefaults {
    /// Default lock level
    #[serde(default = "default_lock_level")]
    pub lock: LockLevel,

    /// Default style guide
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,

    /// Default AI behavior
    #[serde(default)]
    pub behavior: Behavior,
}

fn default_lock_level() -> LockLevel {
    LockLevel::Normal
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum LockLevel {
    Frozen,
    Restricted,
    ApprovalRequired,
    TestsRequired,
    DocsRequired,
    ReviewRequired,
    #[default]
    Normal,
    Experimental,
}


#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Behavior {
    Conservative,
    #[default]
    Balanced,
    Aggressive,
}

/// @acp:summary "Domain pattern configuration (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPatternConfig {
    /// Glob patterns for this domain
    pub patterns: Vec<String>,
}

/// @acp:summary "Call graph generation configuration (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphConfig {
    /// Include standard library calls
    #[serde(default)]
    pub include_stdlib: bool,

    /// Maximum call depth (null = unlimited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,

    /// Patterns to exclude from graph
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

/// @acp:summary "Implementation limits (schema-compliant)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    /// Maximum source file size in MB
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: usize,

    /// Maximum files in project
    #[serde(default = "default_max_files")]
    pub max_files: usize,

    /// Maximum annotations per file
    #[serde(default = "default_max_annotations")]
    pub max_annotations_per_file: usize,

    /// Maximum cache file size in MB
    #[serde(default = "default_max_cache_size")]
    pub max_cache_size_mb: usize,
}

fn default_max_file_size() -> usize {
    10
}

fn default_max_files() -> usize {
    100_000
}

fn default_max_annotations() -> usize {
    1000
}

fn default_max_cache_size() -> usize {
    100
}

/// @acp:summary "Output file path configuration (internal)"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Cache file output path
    #[serde(default = "default_cache_path")]
    pub cache: PathBuf,

    /// Vars file output path
    #[serde(default = "default_vars_path")]
    pub vars: PathBuf,

    /// Whether to also output SQLite database
    #[serde(default)]
    pub sqlite: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            cache: default_cache_path(),
            vars: default_vars_path(),
            sqlite: false,
        }
    }
}

fn default_cache_path() -> PathBuf {
    PathBuf::from(".acp/acp.cache.json")
}

fn default_vars_path() -> PathBuf {
    PathBuf::from(".acp/acp.vars.json")
}

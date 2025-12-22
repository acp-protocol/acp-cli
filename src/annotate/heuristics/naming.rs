//! @acp:module "Naming Heuristics"
//! @acp:summary "Infers annotations from identifier naming patterns"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Naming Heuristics
//!
//! Analyzes symbol names to infer appropriate annotations:
//! - Security patterns (auth, token, password, etc.)
//! - Data patterns (repository, store, database, etc.)
//! - Test patterns (test, spec, mock, etc.)
//! - Experimental patterns (wip, hack, temp, etc.)

use crate::ast::SymbolKind;

use crate::annotate::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Security-related naming patterns"
static SECURITY_PATTERNS: &[&str] = &[
    "auth", "authenticate", "authorization", "authorize",
    "token", "jwt", "session", "credential", "password",
    "secret", "key", "encrypt", "decrypt", "hash", "verify",
    "sign", "validate", "sanitize", "escape", "permission",
    "role", "access", "grant", "revoke", "login", "logout",
];

/// @acp:summary "Database/storage naming patterns"
static DATA_PATTERNS: &[&str] = &[
    "repository", "repo", "store", "storage", "database", "db",
    "query", "fetch", "save", "update", "delete", "insert",
    "cache", "persist", "serialize", "deserialize",
];

/// @acp:summary "Test-related naming patterns"
static TEST_PATTERNS: &[&str] = &[
    "test", "spec", "mock", "stub", "fake", "fixture",
];

/// @acp:summary "Experimental/temporary code patterns"
static EXPERIMENTAL_PATTERNS: &[&str] = &[
    "experimental", "beta", "alpha", "wip", "todo",
    "hack", "temp", "temporary", "draft",
];

/// @acp:summary "Handler/controller patterns"
static HANDLER_PATTERNS: &[&str] = &[
    "handler", "controller", "endpoint", "route",
];

/// @acp:summary "Service patterns"
static SERVICE_PATTERNS: &[&str] = &[
    "service", "svc", "manager", "provider",
];

/// @acp:summary "Middleware patterns"
static MIDDLEWARE_PATTERNS: &[&str] = &[
    "middleware", "interceptor", "filter", "guard",
];

/// @acp:summary "Infers annotations from identifier naming patterns"
/// @acp:lock normal
pub struct NamingHeuristics {
    /// Custom security patterns (in addition to defaults)
    custom_security_patterns: Vec<String>,
}

impl NamingHeuristics {
    /// @acp:summary "Creates a new naming heuristics analyzer"
    pub fn new() -> Self {
        Self {
            custom_security_patterns: Vec::new(),
        }
    }

    /// @acp:summary "Adds custom security patterns"
    pub fn with_security_patterns(mut self, patterns: Vec<String>) -> Self {
        self.custom_security_patterns = patterns;
        self
    }

    /// @acp:summary "Generates suggestions based on identifier name"
    pub fn suggest(&self, name: &str, line: usize) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        let name_lower = name.to_lowercase();

        // Security patterns → restricted lock + security domain
        if self.matches_security_pattern(&name_lower) {
            suggestions.push(
                Suggestion::lock(name, line, "restricted", SuggestionSource::Heuristic)
                    .with_confidence(0.8),
            );
            suggestions.push(
                Suggestion::ai_hint(name, line, "security-sensitive", SuggestionSource::Heuristic)
                    .with_confidence(0.8),
            );
            suggestions.push(
                Suggestion::domain(name, line, "security", SuggestionSource::Heuristic)
                    .with_confidence(0.7),
            );
        }

        // Data patterns → data domain + repository layer
        if self.matches_pattern(&name_lower, DATA_PATTERNS) {
            suggestions.push(
                Suggestion::domain(name, line, "data", SuggestionSource::Heuristic)
                    .with_confidence(0.7),
            );
            suggestions.push(
                Suggestion::layer(name, line, "repository", SuggestionSource::Heuristic)
                    .with_confidence(0.6),
            );
        }

        // Test patterns → tests domain + experimental lock
        if self.matches_pattern(&name_lower, TEST_PATTERNS) {
            suggestions.push(
                Suggestion::domain(name, line, "tests", SuggestionSource::Heuristic)
                    .with_confidence(0.9),
            );
        }

        // Experimental patterns → experimental stability
        if self.matches_pattern(&name_lower, EXPERIMENTAL_PATTERNS) {
            suggestions.push(
                Suggestion::new(
                    name,
                    line,
                    AnnotationType::Stability,
                    "experimental",
                    SuggestionSource::Heuristic,
                )
                .with_confidence(0.8),
            );
        }

        // Handler/controller patterns → handler layer
        if self.matches_pattern(&name_lower, HANDLER_PATTERNS) {
            suggestions.push(
                Suggestion::layer(name, line, "handler", SuggestionSource::Heuristic)
                    .with_confidence(0.7),
            );
        }

        // Service patterns → service layer
        if self.matches_pattern(&name_lower, SERVICE_PATTERNS) {
            suggestions.push(
                Suggestion::layer(name, line, "service", SuggestionSource::Heuristic)
                    .with_confidence(0.7),
            );
        }

        // Middleware patterns → middleware layer
        if self.matches_pattern(&name_lower, MIDDLEWARE_PATTERNS) {
            suggestions.push(
                Suggestion::layer(name, line, "middleware", SuggestionSource::Heuristic)
                    .with_confidence(0.7),
            );
        }

        suggestions
    }

    /// @acp:summary "Generates a summary from an identifier name"
    ///
    /// Splits camelCase/snake_case names and generates human-readable summaries.
    pub fn generate_summary(&self, name: &str, kind: Option<SymbolKind>) -> Option<String> {
        // Skip very short names
        if name.len() < 3 {
            return None;
        }

        let words = split_identifier(name);
        if words.is_empty() {
            return None;
        }

        let summary = match kind {
            Some(SymbolKind::Function) | Some(SymbolKind::Method) => {
                // "getUserById" → "Gets user by ID"
                let verb = &words[0];
                let rest: Vec<String> = words[1..]
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect();

                let verb_third_person = to_third_person(verb);
                if rest.is_empty() {
                    capitalize(&verb_third_person)
                } else {
                    format!("{} {}", capitalize(&verb_third_person), rest.join(" "))
                }
            }
            Some(SymbolKind::Class) | Some(SymbolKind::Struct) => {
                // "UserService" → "User service"
                words
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .pipe(|s| capitalize(&s))
            }
            Some(SymbolKind::Interface) | Some(SymbolKind::Trait) => {
                // "Serializable" → "Serializable interface/trait"
                let type_name = match kind {
                    Some(SymbolKind::Interface) => "interface",
                    Some(SymbolKind::Trait) => "trait",
                    _ => "type",
                };
                format!("{} {}", words.join(""), type_name)
            }
            _ => return None,
        };

        // Validate the generated summary
        if validate_summary(&summary) {
            Some(summary)
        } else {
            // Fallback to simple format
            Some(format!("{} {}", capitalize(&name.to_lowercase()), kind_to_string(kind)))
        }
    }

    /// @acp:summary "Checks if name matches security patterns"
    fn matches_security_pattern(&self, name_lower: &str) -> bool {
        SECURITY_PATTERNS.iter().any(|p| name_lower.contains(p))
            || self
                .custom_security_patterns
                .iter()
                .any(|p| name_lower.contains(&p.to_lowercase()))
    }

    /// @acp:summary "Checks if name matches any pattern in the list"
    fn matches_pattern(&self, name_lower: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|p| name_lower.contains(p))
    }
}

impl Default for NamingHeuristics {
    fn default() -> Self {
        Self::new()
    }
}

/// @acp:summary "Validates a generated summary for quality"
///
/// Checks for common issues like double pluralization, double spaces, etc.
fn validate_summary(summary: &str) -> bool {
    // Minimum length check
    if summary.len() < 5 {
        return false;
    }

    // Check for common issues
    let problems = [
        "eses ",  // Double pluralization like "Featureses"
        "sses ",  // Double pluralization
        "  ",     // Double spaces
    ];

    for problem in &problems {
        if summary.contains(problem) {
            return false;
        }
    }

    // Check for words ending in "eses" (double plural)
    for word in summary.split_whitespace() {
        if word.ends_with("eses") || word.ends_with("sses") {
            return false;
        }
    }

    true
}

/// @acp:summary "Converts symbol kind to readable string"
fn kind_to_string(kind: Option<SymbolKind>) -> &'static str {
    match kind {
        Some(SymbolKind::Function) => "function",
        Some(SymbolKind::Method) => "method",
        Some(SymbolKind::Class) => "class",
        Some(SymbolKind::Struct) => "struct",
        Some(SymbolKind::Interface) => "interface",
        Some(SymbolKind::Trait) => "trait",
        Some(SymbolKind::Enum) => "enum",
        Some(SymbolKind::EnumVariant) => "variant",
        Some(SymbolKind::Constant) => "constant",
        Some(SymbolKind::Variable) => "variable",
        Some(SymbolKind::Property) => "property",
        Some(SymbolKind::Field) => "field",
        Some(SymbolKind::TypeAlias) => "type",
        Some(SymbolKind::Module) => "module",
        Some(SymbolKind::Namespace) => "namespace",
        None | Some(_) => "symbol",
    }
}

/// @acp:summary "Splits an identifier into words"
///
/// Handles both camelCase and snake_case naming conventions.
fn split_identifier(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for (i, c) in name.chars().enumerate() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                words.push(current);
                current = String::new();
            }
        } else if c.is_uppercase() && i > 0 {
            if !current.is_empty() {
                words.push(current);
                current = String::new();
            }
            current.push(c);
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

/// @acp:summary "Converts a verb to third person singular"
fn to_third_person(verb: &str) -> String {
    let lower = verb.to_lowercase();
    match lower.as_str() {
        // Basic CRUD and accessors
        "get" => "Gets".to_string(),
        "set" => "Sets".to_string(),
        "create" => "Creates".to_string(),
        "delete" => "Deletes".to_string(),
        "update" => "Updates".to_string(),
        "insert" => "Inserts".to_string(),
        "remove" => "Removes".to_string(),
        "add" => "Adds".to_string(),
        "put" => "Puts".to_string(),
        "patch" => "Patches".to_string(),

        // Boolean checks
        "is" => "Checks if".to_string(),
        "has" => "Checks if has".to_string(),
        "can" => "Checks if can".to_string(),
        "should" => "Determines if should".to_string(),
        "will" => "Determines if will".to_string(),
        "does" => "Checks if does".to_string(),

        // Data operations
        "fetch" => "Fetches".to_string(),
        "load" => "Loads".to_string(),
        "save" => "Saves".to_string(),
        "store" => "Stores".to_string(),
        "cache" => "Caches".to_string(),
        "query" => "Queries".to_string(),
        "find" => "Finds".to_string(),
        "search" => "Searches".to_string(),
        "filter" => "Filters".to_string(),
        "sort" => "Sorts".to_string(),
        "map" => "Maps".to_string(),
        "reduce" => "Reduces".to_string(),
        "merge" => "Merges".to_string(),
        "split" => "Splits".to_string(),
        "join" => "Joins".to_string(),
        "concat" => "Concatenates".to_string(),
        "append" => "Appends".to_string(),
        "prepend" => "Prepends".to_string(),
        "pop" => "Pops".to_string(),
        "push" => "Pushes".to_string(),
        "shift" => "Shifts".to_string(),
        "unshift" => "Unshifts".to_string(),

        // Validation and parsing
        "validate" => "Validates".to_string(),
        "verify" => "Verifies".to_string(),
        "check" => "Checks".to_string(),
        "assert" => "Asserts".to_string(),
        "ensure" => "Ensures".to_string(),
        "parse" => "Parses".to_string(),
        "sanitize" => "Sanitizes".to_string(),
        "normalize" => "Normalizes".to_string(),
        "clean" => "Cleans".to_string(),

        // Conversion and transformation
        "convert" => "Converts".to_string(),
        "transform" => "Transforms".to_string(),
        "format" => "Formats".to_string(),
        "encode" => "Encodes".to_string(),
        "decode" => "Decodes".to_string(),
        "encrypt" => "Encrypts".to_string(),
        "decrypt" => "Decrypts".to_string(),
        "compress" => "Compresses".to_string(),
        "decompress" => "Decompresses".to_string(),
        "serialize" => "Serializes".to_string(),
        "deserialize" => "Deserializes".to_string(),
        "stringify" => "Stringifies".to_string(),
        "tokenize" => "Tokenizes".to_string(),

        // Processing and computation
        "process" => "Processes".to_string(),
        "handle" => "Handles".to_string(),
        "calculate" | "calc" => "Calculates".to_string(),
        "compute" => "Computes".to_string(),
        "evaluate" => "Evaluates".to_string(),
        "analyze" => "Analyzes".to_string(),
        "aggregate" => "Aggregates".to_string(),
        "extract" => "Extracts".to_string(),
        "generate" => "Generates".to_string(),
        "derive" => "Derives".to_string(),

        // Lifecycle and initialization
        "init" | "initialize" => "Initializes".to_string(),
        "setup" => "Sets up".to_string(),
        "teardown" => "Tears down".to_string(),
        "destroy" => "Destroys".to_string(),
        "dispose" => "Disposes".to_string(),
        "cleanup" => "Cleans up".to_string(),
        "reset" => "Resets".to_string(),
        "clear" => "Clears".to_string(),

        // State management
        "enable" => "Enables".to_string(),
        "disable" => "Disables".to_string(),
        "activate" => "Activates".to_string(),
        "deactivate" => "Deactivates".to_string(),
        "show" => "Shows".to_string(),
        "hide" => "Hides".to_string(),
        "toggle" => "Toggles".to_string(),
        "lock" => "Locks".to_string(),
        "unlock" => "Unlocks".to_string(),

        // I/O operations
        "read" => "Reads".to_string(),
        "write" => "Writes".to_string(),
        "open" => "Opens".to_string(),
        "close" => "Closes".to_string(),
        "send" => "Sends".to_string(),
        "receive" => "Receives".to_string(),
        "emit" => "Emits".to_string(),
        "broadcast" => "Broadcasts".to_string(),
        "publish" => "Publishes".to_string(),
        "subscribe" => "Subscribes".to_string(),
        "unsubscribe" => "Unsubscribes".to_string(),
        "listen" => "Listens".to_string(),
        "notify" => "Notifies".to_string(),

        // Connection and networking
        "connect" => "Connects".to_string(),
        "disconnect" => "Disconnects".to_string(),
        "request" => "Requests".to_string(),
        "respond" => "Responds".to_string(),
        "upload" => "Uploads".to_string(),
        "download" => "Downloads".to_string(),
        "sync" => "Syncs".to_string(),

        // Execution control
        "start" => "Starts".to_string(),
        "stop" => "Stops".to_string(),
        "run" => "Runs".to_string(),
        "execute" => "Executes".to_string(),
        "invoke" => "Invokes".to_string(),
        "call" => "Calls".to_string(),
        "trigger" => "Triggers".to_string(),
        "fire" => "Fires".to_string(),
        "dispatch" => "Dispatches".to_string(),
        "schedule" => "Schedules".to_string(),
        "spawn" => "Spawns".to_string(),
        "await" => "Awaits".to_string(),

        // Building and construction
        "build" => "Builds".to_string(),
        "make" => "Makes".to_string(),
        "construct" => "Constructs".to_string(),
        "assemble" => "Assembles".to_string(),
        "compile" => "Compiles".to_string(),
        "render" => "Renders".to_string(),
        "draw" => "Draws".to_string(),
        "paint" => "Paints".to_string(),

        // Registration and binding
        "register" => "Registers".to_string(),
        "unregister" => "Unregisters".to_string(),
        "bind" => "Binds".to_string(),
        "unbind" => "Unbinds".to_string(),
        "attach" => "Attaches".to_string(),
        "detach" => "Detaches".to_string(),
        "mount" => "Mounts".to_string(),
        "unmount" => "Unmounts".to_string(),
        "inject" => "Injects".to_string(),

        // Authentication
        "login" => "Logs in".to_string(),
        "logout" => "Logs out".to_string(),
        "authenticate" => "Authenticates".to_string(),
        "authorize" => "Authorizes".to_string(),
        "sign" => "Signs".to_string(),

        // Async and promises
        "resolve" => "Resolves".to_string(),
        "reject" => "Rejects".to_string(),
        "cancel" => "Cancels".to_string(),
        "abort" => "Aborts".to_string(),
        "retry" => "Retries".to_string(),

        // Discovery and scanning
        "discover" => "Discovers".to_string(),
        "scan" => "Scans".to_string(),
        "detect" => "Detects".to_string(),
        "inspect" => "Inspects".to_string(),
        "probe" => "Probes".to_string(),
        "index" => "Indexes".to_string(),

        // Logging and debugging
        "log" => "Logs".to_string(),
        "trace" => "Traces".to_string(),
        "debug" => "Debugs".to_string(),
        "warn" => "Warns".to_string(),
        "error" => "Errors".to_string(),
        "print" => "Prints".to_string(),
        "dump" => "Dumps".to_string(),

        // Application and configuration
        "apply" => "Applies".to_string(),
        "configure" => "Configures".to_string(),
        "customize" => "Customizes".to_string(),
        "adjust" => "Adjusts".to_string(),
        "modify" => "Modifies".to_string(),

        // Import/Export
        "import" => "Imports".to_string(),
        "export" => "Exports".to_string(),
        "reload" => "Reloads".to_string(),
        "refresh" => "Refreshes".to_string(),

        // Copying and cloning
        "copy" => "Copies".to_string(),
        "clone" => "Clones".to_string(),
        "duplicate" => "Duplicates".to_string(),
        "replicate" => "Replicates".to_string(),

        // Comparison
        "compare" => "Compares".to_string(),
        "diff" => "Diffs".to_string(),
        "match" => "Matches".to_string(),
        "equals" => "Checks equality".to_string(),

        // Navigation
        "navigate" => "Navigates".to_string(),
        "redirect" => "Redirects".to_string(),
        "route" => "Routes".to_string(),
        "visit" => "Visits".to_string(),

        // Testing
        "test" => "Tests".to_string(),
        "mock" => "Mocks".to_string(),
        "stub" => "Stubs".to_string(),
        "spy" => "Spies on".to_string(),

        // Version control
        "commit" => "Commits".to_string(),
        "revert" => "Reverts".to_string(),
        "rollback" => "Rolls back".to_string(),
        "checkout" => "Checks out".to_string(),
        "branch" => "Branches".to_string(),

        // UI interactions
        "click" => "Clicks".to_string(),
        "hover" => "Hovers".to_string(),
        "focus" => "Focuses".to_string(),
        "blur" => "Blurs".to_string(),
        "scroll" => "Scrolls".to_string(),
        "drag" => "Drags".to_string(),
        "drop" => "Drops".to_string(),
        "select" => "Selects".to_string(),
        "deselect" => "Deselects".to_string(),
        "expand" => "Expands".to_string(),
        "collapse" => "Collapses".to_string(),
        "submit" => "Submits".to_string(),

        // File operations
        "mkdir" => "Creates directory".to_string(),
        "rmdir" => "Removes directory".to_string(),
        "unlink" => "Unlinks".to_string(),
        "rename" => "Renames".to_string(),
        "move" => "Moves".to_string(),
        "chmod" => "Changes permissions".to_string(),
        "chown" => "Changes ownership".to_string(),
        "stat" => "Gets stats for".to_string(),
        "exists" => "Checks if exists".to_string(),

        _ => {
            // Default: add 's' or 'es' with proper capitalization
            let result = if lower.ends_with('x')
                || lower.ends_with("ch")
                || lower.ends_with("sh")
            {
                // Words ending in x, ch, sh → add 'es'
                format!("{}es", lower)
            } else if lower.ends_with('s') {
                // Words already ending in 's' - check if it looks like a verb
                // For words like "process" → "processes", but avoid double-s for already plural words
                if lower.ends_with("ss") || lower.ends_with("us") || lower.ends_with("is") {
                    format!("{}es", lower)
                } else {
                    // Likely already third person or noun-like, just capitalize
                    capitalize(&lower)
                }
            } else if lower.ends_with('y') && lower.len() > 1 {
                // Check for vowel + y pattern (display, play, etc.) → just add 's'
                let chars: Vec<char> = lower.chars().collect();
                let second_last = chars.get(chars.len() - 2);
                if matches!(second_last, Some('a') | Some('e') | Some('i') | Some('o') | Some('u')) {
                    format!("{}s", lower)
                } else {
                    // Consonant + y → change y to ies
                    format!("{}ies", &lower[..lower.len() - 1])
                }
            } else {
                format!("{}s", lower)
            };
            capitalize(&result)
        }
    }
}

/// @acp:summary "Capitalizes the first character of a string"
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

/// @acp:summary "Extension trait for pipe operations"
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R;
}

impl Pipe for String {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_identifier_camel_case() {
        assert_eq!(
            split_identifier("getUserById"),
            vec!["get", "User", "By", "Id"]
        );
    }

    #[test]
    fn test_split_identifier_snake_case() {
        assert_eq!(
            split_identifier("get_user_by_id"),
            vec!["get", "user", "by", "id"]
        );
    }

    #[test]
    fn test_to_third_person() {
        assert_eq!(to_third_person("get"), "Gets");
        assert_eq!(to_third_person("validate"), "Validates");
        assert_eq!(to_third_person("process"), "Processes");
        assert_eq!(to_third_person("apply"), "Applies");
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("hello"), "Hello");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn test_suggest_security_pattern() {
        let heuristics = NamingHeuristics::new();
        let suggestions = heuristics.suggest("validateToken", 10);

        let has_security_domain = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Domain && s.value == "security"
        });
        let has_restricted_lock = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Lock && s.value == "restricted"
        });

        assert!(has_security_domain);
        assert!(has_restricted_lock);
    }

    #[test]
    fn test_generate_summary_function() {
        let heuristics = NamingHeuristics::new();

        let summary = heuristics.generate_summary("getUserById", Some(SymbolKind::Function));
        assert_eq!(summary, Some("Gets user by id".to_string()));

        let summary = heuristics.generate_summary("validateToken", Some(SymbolKind::Function));
        assert_eq!(summary, Some("Validates token".to_string()));
    }

    #[test]
    fn test_generate_summary_class() {
        let heuristics = NamingHeuristics::new();

        let summary = heuristics.generate_summary("UserService", Some(SymbolKind::Class));
        assert_eq!(summary, Some("User service".to_string()));
    }
}

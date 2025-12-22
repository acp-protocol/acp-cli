//! @acp:module "Path Heuristics"
//! @acp:summary "Infers annotations from file path patterns"
//! @acp:domain cli
//! @acp:layer service
//! @acp:stability experimental
//!
//! # Path Heuristics
//!
//! Analyzes file paths to infer appropriate annotations:
//! - Domain inference from directory names (auth → authentication)
//! - Layer inference from directory structure (handlers → handler)
//! - Module naming from path components

use std::collections::HashMap;
use std::path::Path;

use crate::annotate::{AnnotationType, Suggestion, SuggestionSource};

/// @acp:summary "Common directory → domain mappings"
static PATH_DOMAIN_MAPPINGS: &[(&str, &str)] = &[
    ("auth", "authentication"),
    ("authentication", "authentication"),
    ("login", "authentication"),
    ("user", "users"),
    ("users", "users"),
    ("account", "users"),
    ("billing", "billing"),
    ("payment", "billing"),
    ("payments", "billing"),
    ("stripe", "billing"),
    ("api", "api"),
    ("rest", "api"),
    ("graphql", "api"),
    ("grpc", "api"),
    ("db", "database"),
    ("database", "database"),
    ("models", "database"),
    ("repository", "database"),
    ("repositories", "database"),
    ("cache", "caching"),
    ("redis", "caching"),
    ("queue", "messaging"),
    ("messaging", "messaging"),
    ("events", "messaging"),
    ("pubsub", "messaging"),
    ("util", "utilities"),
    ("utils", "utilities"),
    ("helpers", "utilities"),
    ("common", "utilities"),
    ("shared", "utilities"),
    ("test", "testing"),
    ("tests", "testing"),
    ("spec", "testing"),
    ("__tests__", "testing"),
    ("e2e", "testing"),
    ("integration", "testing"),
    ("config", "configuration"),
    ("settings", "configuration"),
    ("middleware", "middleware"),
    ("interceptor", "middleware"),
    ("handlers", "handlers"),
    ("controllers", "handlers"),
    ("services", "services"),
    ("domain", "domain"),
    ("core", "core"),
    ("lib", "library"),
    ("pkg", "library"),
    ("internal", "internal"),
    ("vendor", "vendor"),
    ("third_party", "vendor"),
    ("security", "security"),
    ("crypto", "security"),
    ("notifications", "notifications"),
    ("email", "notifications"),
    ("sms", "notifications"),
    ("analytics", "analytics"),
    ("metrics", "analytics"),
    ("monitoring", "monitoring"),
    ("logging", "monitoring"),
    ("admin", "administration"),
    ("dashboard", "administration"),
];

/// @acp:summary "Common directory → layer mappings"
static PATH_LAYER_MAPPINGS: &[(&str, &str)] = &[
    ("handlers", "handler"),
    ("controllers", "handler"),
    ("api", "handler"),
    ("routes", "handler"),
    ("endpoints", "handler"),
    ("services", "service"),
    ("usecases", "service"),
    ("application", "service"),
    ("repository", "repository"),
    ("repositories", "repository"),
    ("dao", "repository"),
    ("db", "repository"),
    ("models", "model"),
    ("entities", "model"),
    ("domain", "model"),
    ("middleware", "middleware"),
    ("interceptors", "middleware"),
    ("filters", "middleware"),
    ("guards", "middleware"),
    ("utils", "utility"),
    ("helpers", "utility"),
    ("lib", "utility"),
    ("common", "utility"),
    ("config", "config"),
    ("settings", "config"),
];

/// @acp:summary "Infers annotations from file path patterns"
/// @acp:lock normal
pub struct PathHeuristics {
    /// Custom domain mappings (path component → domain name)
    custom_domain_mappings: HashMap<String, String>,
}

impl PathHeuristics {
    /// @acp:summary "Creates a new path heuristics analyzer"
    pub fn new() -> Self {
        Self {
            custom_domain_mappings: HashMap::new(),
        }
    }

    /// @acp:summary "Adds custom domain mappings"
    pub fn with_domain_mappings(mut self, mappings: HashMap<String, String>) -> Self {
        self.custom_domain_mappings = mappings;
        self
    }

    /// @acp:summary "Generates suggestions based on file path"
    pub fn suggest(&self, file_path: &str, target: &str, line: usize) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        let path = Path::new(file_path);

        // Check each path component for domain/layer patterns
        for component in path.components() {
            let comp_str = component.as_os_str().to_string_lossy().to_lowercase();

            // Check custom mappings first
            if let Some(domain) = self.custom_domain_mappings.get(&comp_str) {
                suggestions.push(
                    Suggestion::domain(target, line, domain, SuggestionSource::Heuristic)
                        .with_confidence(0.8),
                );
            }

            // Check standard domain mappings
            for (pattern, domain) in PATH_DOMAIN_MAPPINGS.iter() {
                if comp_str == *pattern || comp_str.contains(pattern) {
                    suggestions.push(
                        Suggestion::domain(target, line, *domain, SuggestionSource::Heuristic)
                            .with_confidence(0.7),
                    );
                    break; // Only one domain per component
                }
            }

            // Check layer mappings
            for (pattern, layer) in PATH_LAYER_MAPPINGS.iter() {
                if comp_str == *pattern || comp_str.contains(pattern) {
                    suggestions.push(
                        Suggestion::layer(target, line, *layer, SuggestionSource::Heuristic)
                            .with_confidence(0.6),
                    );
                    break; // Only one layer per component
                }
            }
        }

        // Deduplicate suggestions by value
        let mut seen_domains = std::collections::HashSet::new();
        let mut seen_layers = std::collections::HashSet::new();

        suggestions.retain(|s| {
            match s.annotation_type {
                AnnotationType::Domain => seen_domains.insert(s.value.clone()),
                AnnotationType::Layer => seen_layers.insert(s.value.clone()),
                _ => true,
            }
        });

        suggestions
    }

    /// @acp:summary "Infers a module name from file path"
    ///
    /// Uses directory name + file name to generate a human-readable module name.
    pub fn infer_module_name(&self, file_path: &str) -> Option<String> {
        let path = Path::new(file_path);

        let file_stem = path.file_stem()?.to_string_lossy();

        // Skip generic file names
        let generic_names = ["index", "mod", "main", "lib", "__init__", "init"];
        if generic_names.contains(&file_stem.as_ref()) {
            // Use parent directory name instead
            let parent = path.parent()?.file_name()?.to_string_lossy();
            return Some(humanize_name(&parent));
        }

        Some(humanize_name(&file_stem))
    }

    /// @acp:summary "Checks if a path is in a test directory"
    pub fn is_test_path(&self, file_path: &str) -> bool {
        let path_lower = file_path.to_lowercase();
        let test_indicators = [
            "/test/",
            "/tests/",
            "/__tests__/",
            "/spec/",
            "/e2e/",
            ".test.",
            ".spec.",
            "_test.",
            "_spec.",
        ];

        test_indicators.iter().any(|ind| path_lower.contains(ind))
    }
}

impl Default for PathHeuristics {
    fn default() -> Self {
        Self::new()
    }
}

/// @acp:summary "Converts an identifier to a human-readable name"
fn humanize_name(name: &str) -> String {
    // Split on underscores, hyphens, and camelCase
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

    // Capitalize first word, lowercase the rest
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                let mut chars = w.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                }
            } else {
                w.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggest_domain_from_path() {
        let heuristics = PathHeuristics::new();

        let suggestions = heuristics.suggest("src/auth/service.ts", "AuthService", 10);
        let has_auth_domain = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Domain && s.value == "authentication"
        });
        assert!(has_auth_domain);

        let suggestions = heuristics.suggest("src/billing/payments.ts", "ProcessPayment", 10);
        let has_billing_domain = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Domain && s.value == "billing"
        });
        assert!(has_billing_domain);
    }

    #[test]
    fn test_suggest_layer_from_path() {
        let heuristics = PathHeuristics::new();

        let suggestions = heuristics.suggest("src/handlers/user.ts", "UserHandler", 10);
        let has_handler_layer = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Layer && s.value == "handler"
        });
        assert!(has_handler_layer);

        let suggestions = heuristics.suggest("src/services/auth.ts", "AuthService", 10);
        let has_service_layer = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Layer && s.value == "service"
        });
        assert!(has_service_layer);
    }

    #[test]
    fn test_infer_module_name() {
        let heuristics = PathHeuristics::new();

        assert_eq!(
            heuristics.infer_module_name("src/auth/session_service.ts"),
            Some("Session service".to_string())
        );

        assert_eq!(
            heuristics.infer_module_name("src/auth/index.ts"),
            Some("Auth".to_string())
        );
    }

    #[test]
    fn test_is_test_path() {
        let heuristics = PathHeuristics::new();

        assert!(heuristics.is_test_path("src/__tests__/auth.test.ts"));
        assert!(heuristics.is_test_path("tests/integration/user.spec.ts"));
        assert!(!heuristics.is_test_path("src/services/auth.ts"));
    }

    #[test]
    fn test_humanize_name() {
        assert_eq!(humanize_name("user_service"), "User service");
        assert_eq!(humanize_name("UserService"), "User service");
        assert_eq!(humanize_name("auth-handler"), "Auth handler");
    }

    #[test]
    fn test_custom_domain_mappings() {
        let mut mappings = HashMap::new();
        mappings.insert("checkout".to_string(), "commerce".to_string());

        let heuristics = PathHeuristics::new().with_domain_mappings(mappings);
        let suggestions = heuristics.suggest("src/checkout/cart.ts", "Cart", 10);

        let has_commerce = suggestions.iter().any(|s| {
            s.annotation_type == AnnotationType::Domain && s.value == "commerce"
        });
        assert!(has_commerce);
    }
}

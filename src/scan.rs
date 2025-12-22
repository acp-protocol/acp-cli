//! @acp:module "Scanner"
//! @acp:summary "Project scanning and auto-detection"
//! @acp:domain cli
//! @acp:layer service

use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

/// Detected project information
#[derive(Debug, Default)]
pub struct ProjectScan {
    pub languages: Vec<DetectedLanguage>,
    pub has_package_json: bool,
    pub has_cargo_toml: bool,
    pub has_pyproject_toml: bool,
    pub has_go_mod: bool,
    // pub mcp_available: bool,  // TODO: Detect MCP server availability
}

#[derive(Debug, Clone)]
pub struct DetectedLanguage {
    pub name: &'static str,
    pub patterns: Vec<&'static str>,
    pub file_count: usize,
}

/// Scan project directory to detect languages and configuration
pub fn scan_project<P: AsRef<Path>>(root: P) -> ProjectScan {
    let root = root.as_ref();
    let mut ext_counts: HashMap<String, usize> = HashMap::new();
    let mut scan = ProjectScan::default();

    for entry in WalkDir::new(root)
        .max_depth(10)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip common non-source directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if matches!(name, "node_modules" | "target" | "dist" | "build" | ".git" | "vendor" | "__pycache__") {
                continue;
            }
        }

        // Check for project files
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            match name {
                "package.json" => scan.has_package_json = true,
                "Cargo.toml" => scan.has_cargo_toml = true,
                "pyproject.toml" | "setup.py" => scan.has_pyproject_toml = true,
                "go.mod" => scan.has_go_mod = true,
                _ => {}
            }
        }

        // Count file extensions
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext = ext.to_lowercase();
                *ext_counts.entry(ext).or_insert(0) += 1;
            }
        }
    }

    // Map extensions to languages
    let lang_mappings: [(&str, &[&str], &[&str]); 6] = [
        ("TypeScript", &["ts", "tsx"], &["**/*.ts", "**/*.tsx"]),
        ("JavaScript", &["js", "jsx", "mjs"], &["**/*.js", "**/*.jsx", "**/*.mjs"]),
        ("Rust", &["rs"], &["**/*.rs"]),
        ("Python", &["py"], &["**/*.py"]),
        ("Go", &["go"], &["**/*.go"]),
        ("Java", &["java"], &["**/*.java"]),
    ];

    for (name, exts, patterns) in lang_mappings {
        let count: usize = exts.iter()
            .filter_map(|e| ext_counts.get(*e))
            .sum();

        if count > 0 {
            scan.languages.push(DetectedLanguage {
                name,
                patterns: patterns.to_vec(),
                file_count: count,
            });
        }
    }

    // Sort by file count descending
    scan.languages.sort_by(|a, b| b.file_count.cmp(&a.file_count));

    // TODO: MCP detection (commented out for future implementation)
    // scan.mcp_available = detect_mcp_server();

    scan
}

// TODO: Detect MCP server availability
// This will be used in the future to determine if we can use MCP for enhanced functionality
//
// fn detect_mcp_server() -> bool {
//     // Check for MCP server configuration
//     // Option 1: Check for claude_desktop_config.json
//     // let config_path = dirs::config_dir()
//     //     .map(|d| d.join("Claude").join("claude_desktop_config.json"));
//     // if let Some(path) = config_path {
//     //     if path.exists() {
//     //         // Parse and check for ACP MCP server
//     //         return true;
//     //     }
//     // }
//     //
//     // Option 2: Try to connect to MCP server
//     // Option 3: Check for .mcp.json in project root
//     //
//     false
// }

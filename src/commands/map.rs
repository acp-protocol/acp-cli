//! @acp:module "Map Command"
//! @acp:summary "Display directory/file structure with annotations (RFC-001)"
//! @acp:domain cli
//! @acp:layer service
//!
//! Implements `acp map <path>` command for hierarchical codebase navigation.

use std::collections::HashMap;
use std::path::Path;

use console::style;
use serde::Serialize;

use crate::cache::{Cache, FileEntry};
use crate::error::Result;

use super::output::{TreeRenderer, constraint_level_str};

/// Output format for map command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapFormat {
    #[default]
    Tree,
    Flat,
    Json,
}

/// Options for the map command
#[derive(Debug, Clone)]
pub struct MapOptions {
    pub depth: usize,
    pub show_inline: bool,
    pub format: MapFormat,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            depth: 3,
            show_inline: false,
            format: MapFormat::Tree,
        }
    }
}

/// A file node in the map tree
#[derive(Debug, Clone, Serialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub constraint_level: Option<String>,
    pub purpose: Option<String>,
    pub symbols: Vec<SymbolNode>,
    pub inline_issues: Vec<InlineIssue>,
}

/// A symbol within a file
#[derive(Debug, Clone, Serialize)]
pub struct SymbolNode {
    pub name: String,
    pub symbol_type: String,
    pub line: usize,
    pub is_frozen: bool,
}

/// An inline issue (hack, todo, fixme)
#[derive(Debug, Clone, Serialize)]
pub struct InlineIssue {
    pub file: String,
    pub line: usize,
    pub issue_type: String,
    pub message: String,
    pub expires: Option<String>,
}

/// Directory node in the map tree
#[derive(Debug, Clone, Serialize)]
pub struct DirectoryNode {
    pub name: String,
    pub path: String,
    pub files: Vec<FileNode>,
    pub subdirs: Vec<DirectoryNode>,
}

/// Builder for constructing the map tree from cache
pub struct MapBuilder<'a> {
    cache: &'a Cache,
    options: MapOptions,
}

impl<'a> MapBuilder<'a> {
    pub fn new(cache: &'a Cache, options: MapOptions) -> Self {
        Self { cache, options }
    }

    /// Build the directory tree for a given path
    pub fn build(&self, root_path: &Path) -> Result<DirectoryNode> {
        let root_str = root_path.to_string_lossy().to_string();
        let normalized_root = self.normalize_path(&root_str);

        // Group files by directory
        let mut dir_files: HashMap<String, Vec<&FileEntry>> = HashMap::new();

        for (path, file) in &self.cache.files {
            let normalized = self.normalize_path(path);

            // Check if file is under the root path
            if normalized.starts_with(&normalized_root) || normalized_root.is_empty() || normalized_root == "." {
                let dir = self.get_directory(&normalized);
                dir_files.entry(dir).or_default().push(file);
            }
        }

        // Build the tree recursively
        self.build_directory_node(&normalized_root, &dir_files, 0)
    }

    fn normalize_path(&self, path: &str) -> String {
        path.trim_start_matches("./").replace('\\', "/").to_string()
    }

    fn get_directory(&self, path: &str) -> String {
        Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn build_directory_node(
        &self,
        dir_path: &str,
        dir_files: &HashMap<String, Vec<&FileEntry>>,
        depth: usize,
    ) -> Result<DirectoryNode> {
        let name = Path::new(dir_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dir_path.to_string());

        let mut node = DirectoryNode {
            name,
            path: dir_path.to_string(),
            files: vec![],
            subdirs: vec![],
        };

        // Add files in this directory
        if let Some(files) = dir_files.get(dir_path) {
            for file in files {
                node.files.push(self.build_file_node(file));
            }
            // Sort files by name
            node.files.sort_by(|a, b| a.name.cmp(&b.name));
        }

        // Add subdirectories if within depth limit
        if depth < self.options.depth {
            let mut subdirs: Vec<String> = dir_files
                .keys()
                .filter(|d| {
                    if dir_path.is_empty() {
                        !d.contains('/')
                    } else {
                        d.starts_with(&format!("{}/", dir_path))
                            && d[dir_path.len() + 1..].split('/').count() == 1
                    }
                })
                .cloned()
                .collect();
            subdirs.sort();

            for subdir in subdirs {
                if let Ok(subnode) = self.build_directory_node(&subdir, dir_files, depth + 1) {
                    if !subnode.files.is_empty() || !subnode.subdirs.is_empty() {
                        node.subdirs.push(subnode);
                    }
                }
            }
        }

        Ok(node)
    }

    fn build_file_node(&self, file: &FileEntry) -> FileNode {
        let name = Path::new(&file.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.path.clone());

        // Get constraint level from cache
        let constraint_level = self.cache.constraints.as_ref().and_then(|c| {
            c.by_file.get(&file.path).and_then(|constraints| {
                constraints.mutation.as_ref().map(|m| constraint_level_str(&m.level).to_string())
            })
        });

        // Build symbol list
        let symbols: Vec<SymbolNode> = file.exports.iter().filter_map(|sym_name| {
            self.cache.symbols.get(sym_name).map(|sym| {
                let is_frozen = sym.constraints.as_ref()
                    .map(|c| c.level == "frozen")
                    .unwrap_or(false);
                SymbolNode {
                    name: sym.name.clone(),
                    symbol_type: format!("{:?}", sym.symbol_type).to_lowercase(),
                    line: sym.lines[0],
                    is_frozen,
                }
            })
        }).collect();

        // Build inline issues
        let inline_issues: Vec<InlineIssue> = if self.options.show_inline {
            file.inline.iter().map(|ann| {
                InlineIssue {
                    file: file.path.clone(),
                    line: ann.line,
                    issue_type: ann.annotation_type.clone(),
                    message: ann.directive.clone(),
                    expires: ann.expires.clone(),
                }
            }).collect()
        } else {
            vec![]
        };

        FileNode {
            name,
            path: file.path.clone(),
            constraint_level,
            purpose: file.purpose.clone(),
            symbols,
            inline_issues,
        }
    }

    /// Collect all inline issues across the tree
    pub fn collect_issues(&self, root_path: &Path) -> Vec<InlineIssue> {
        let root_str = root_path.to_string_lossy().to_string();
        let normalized_root = self.normalize_path(&root_str);

        let mut issues = vec![];

        for (path, file) in &self.cache.files {
            let normalized = self.normalize_path(path);
            if normalized.starts_with(&normalized_root) || normalized_root.is_empty() || normalized_root == "." {
                for ann in &file.inline {
                    issues.push(InlineIssue {
                        file: file.path.clone(),
                        line: ann.line,
                        issue_type: ann.annotation_type.clone(),
                        message: ann.directive.clone(),
                        expires: ann.expires.clone(),
                    });
                }
            }
        }

        // Sort by file and line
        issues.sort_by(|a, b| {
            a.file.cmp(&b.file).then(a.line.cmp(&b.line))
        });

        issues
    }
}

/// Render the map tree to stdout
pub fn render_map(node: &DirectoryNode, options: &MapOptions, all_issues: &[InlineIssue]) {
    match options.format {
        MapFormat::Tree => render_tree(node, options, all_issues),
        MapFormat::Flat => render_flat(node),
        MapFormat::Json => render_json(node, all_issues),
    }
}

fn render_tree(node: &DirectoryNode, options: &MapOptions, all_issues: &[InlineIssue]) {
    let renderer = TreeRenderer::default();

    // Print directory header
    println!("{}/", node.path);
    println!("{}", renderer.separator(60));
    println!();

    // Print files
    for file in &node.files {
        render_file_tree(file, &renderer, "");
    }

    // Print subdirectories
    for subdir in &node.subdirs {
        println!();
        println!("{}/", subdir.path);
        for file in &subdir.files {
            render_file_tree(file, &renderer, "  ");
        }
    }

    // Print active issues if enabled
    if options.show_inline && !all_issues.is_empty() {
        println!();
        println!("{}:", style("Active Issues").bold());
        for issue in all_issues {
            let expires_str = issue.expires.as_ref()
                .map(|e| format!(" expires {}", e))
                .unwrap_or_default();
            println!(
                "  {}:{} - @acp:{}{}",
                issue.file,
                issue.line,
                issue.issue_type,
                expires_str
            );
        }
    }
}

fn render_file_tree(file: &FileNode, renderer: &TreeRenderer, indent: &str) {
    // File header with constraint level
    let constraint_str = file.constraint_level.as_ref()
        .map(|l| format!(" ({})", l))
        .unwrap_or_default();

    println!("{}{}{}", indent, style(&file.name).bold(), constraint_str);

    // Purpose
    if let Some(ref purpose) = file.purpose {
        println!("{}  {}", indent, style(purpose).dim());
    }

    // Symbols
    let symbol_count = file.symbols.len();
    for (i, sym) in file.symbols.iter().enumerate() {
        let is_last = i == symbol_count - 1;
        let branch = if is_last { renderer.last_branch() } else { renderer.branch() };

        let frozen_marker = if sym.is_frozen { " [frozen]" } else { "" };
        println!(
            "{}  {} {} ({}:{}){}",
            indent,
            branch,
            sym.name,
            sym.symbol_type,
            sym.line,
            frozen_marker
        );
    }
}

fn render_flat(node: &DirectoryNode) {
    // Flat list of all files with their constraint levels
    render_flat_recursive(node, 0);
}

fn render_flat_recursive(node: &DirectoryNode, depth: usize) {
    let indent = "  ".repeat(depth);

    for file in &node.files {
        let constraint_str = file.constraint_level.as_ref()
            .map(|l| format!(" [{}]", l))
            .unwrap_or_default();
        println!("{}{}{}", indent, file.path, constraint_str);
    }

    for subdir in &node.subdirs {
        render_flat_recursive(subdir, depth);
    }
}

fn render_json(node: &DirectoryNode, issues: &[InlineIssue]) {
    #[derive(Serialize)]
    struct MapOutput<'a> {
        tree: &'a DirectoryNode,
        issues: &'a [InlineIssue],
    }

    let output = MapOutput { tree: node, issues };
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

/// Execute the map command
pub fn execute_map(cache: &Cache, path: &Path, options: MapOptions) -> Result<()> {
    let builder = MapBuilder::new(cache, options.clone());
    let tree = builder.build(path)?;
    let issues = if options.show_inline {
        builder.collect_issues(path)
    } else {
        vec![]
    };

    render_map(&tree, &options, &issues);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_options_default() {
        let opts = MapOptions::default();
        assert_eq!(opts.depth, 3);
        assert!(!opts.show_inline);
        assert_eq!(opts.format, MapFormat::Tree);
    }
}

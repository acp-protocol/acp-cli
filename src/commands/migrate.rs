//! @acp:module "Migrate Command"
//! @acp:summary "Add directive suffixes to existing annotations (RFC-001)"
//! @acp:domain cli
//! @acp:layer service
//!
//! Implements `acp migrate --add-directives` command for upgrading annotations.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use console::style;
use dialoguer::Confirm;
use regex::Regex;

use crate::cache::Cache;
use crate::error::Result;

/// Options for the migrate command
#[derive(Debug, Clone)]
pub struct MigrateOptions {
    pub paths: Vec<PathBuf>,
    pub dry_run: bool,
    pub interactive: bool,
    pub backup: bool,
}

impl Default for MigrateOptions {
    fn default() -> Self {
        Self {
            paths: vec![],
            dry_run: false,
            interactive: false,
            backup: true,
        }
    }
}

/// A single annotation migration
#[derive(Debug, Clone)]
pub struct AnnotationMigration {
    pub file: PathBuf,
    pub line: usize,
    pub original: String,
    pub migrated: String,
    pub annotation_type: String,
    pub annotation_value: String,
}

/// RFC-001 default directives for annotation types
pub struct DirectiveDefaults {
    defaults: HashMap<(String, String), String>,
    type_defaults: HashMap<String, String>,
}

impl DirectiveDefaults {
    pub fn new() -> Self {
        let mut defaults = HashMap::new();
        let mut type_defaults = HashMap::new();

        // Lock level defaults (RFC-001)
        defaults.insert(
            ("lock".to_string(), "frozen".to_string()),
            "MUST NOT modify this code under any circumstances".to_string(),
        );
        defaults.insert(
            ("lock".to_string(), "restricted".to_string()),
            "Explain proposed changes and wait for explicit approval".to_string(),
        );
        defaults.insert(
            ("lock".to_string(), "approval-required".to_string()),
            "Propose changes and request confirmation before applying".to_string(),
        );
        defaults.insert(
            ("lock".to_string(), "tests-required".to_string()),
            "All changes must include corresponding tests".to_string(),
        );
        defaults.insert(
            ("lock".to_string(), "docs-required".to_string()),
            "All changes must update documentation".to_string(),
        );
        defaults.insert(
            ("lock".to_string(), "review-required".to_string()),
            "Changes require code review before merging".to_string(),
        );
        defaults.insert(
            ("lock".to_string(), "normal".to_string()),
            "Safe to modify following project conventions".to_string(),
        );
        defaults.insert(
            ("lock".to_string(), "experimental".to_string()),
            "Experimental code - changes welcome but may be unstable".to_string(),
        );

        // Type-only defaults (for annotations without specific values)
        type_defaults.insert(
            "hack".to_string(),
            "Temporary workaround - check expiry before modifying".to_string(),
        );
        type_defaults.insert(
            "deprecated".to_string(),
            "Do not use or extend - see replacement annotation".to_string(),
        );
        type_defaults.insert(
            "todo".to_string(),
            "Pending work item - address before release".to_string(),
        );
        type_defaults.insert(
            "fixme".to_string(),
            "Known issue requiring fix - prioritize resolution".to_string(),
        );
        type_defaults.insert(
            "critical".to_string(),
            "Critical section - changes require extra review".to_string(),
        );
        type_defaults.insert(
            "perf".to_string(),
            "Performance-sensitive code - benchmark any changes".to_string(),
        );

        Self { defaults, type_defaults }
    }

    /// Get the default directive for an annotation type and value
    pub fn get(&self, annotation_type: &str, value: &str) -> Option<String> {
        // Try exact match first
        if let Some(directive) = self.defaults.get(&(annotation_type.to_string(), value.to_string())) {
            return Some(directive.clone());
        }

        // Try type-only default
        if let Some(directive) = self.type_defaults.get(annotation_type) {
            return Some(directive.clone());
        }

        // Special case for ref - include the URL
        if annotation_type == "ref" {
            return Some(format!("Consult {} before making changes", value));
        }

        None
    }
}

impl Default for DirectiveDefaults {
    fn default() -> Self {
        Self::new()
    }
}

/// Scanner for finding annotations without directives
pub struct MigrationScanner {
    /// Pattern to match @acp: annotations without directive suffix
    /// Captures: 1=type, 2=value (no trailing ` - `)
    pattern: Regex,
    defaults: DirectiveDefaults,
}

impl MigrationScanner {
    pub fn new() -> Self {
        // Match @acp:type with optional value
        // The directive check ` - ` is done separately in scan_file
        let pattern = Regex::new(
            r"@acp:([\w-]+)(?:\s+(.+))?"
        ).expect("Invalid regex pattern");

        Self {
            pattern,
            defaults: DirectiveDefaults::new(),
        }
    }

    /// Scan a file for annotations needing migration
    pub fn scan_file(&self, file_path: &Path) -> Result<Vec<AnnotationMigration>> {
        let content = fs::read_to_string(file_path)?;
        let mut migrations = vec![];

        for (line_num, line) in content.lines().enumerate() {
            // Skip if already has directive suffix
            if line.contains(" - ") && line.contains("@acp:") {
                continue;
            }

            if let Some(cap) = self.pattern.captures(line) {
                let annotation_type = cap.get(1).unwrap().as_str().to_string();
                let annotation_value = cap.get(2)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();

                // Get default directive
                if let Some(directive) = self.defaults.get(&annotation_type, &annotation_value) {
                    let original = line.to_string();

                    // Build migrated line by inserting directive
                    let migrated = if annotation_value.is_empty() {
                        line.replace(
                            &format!("@acp:{}", annotation_type),
                            &format!("@acp:{} - {}", annotation_type, directive),
                        )
                    } else {
                        // Insert ` - directive` after the value
                        let full_match = cap.get(0).unwrap().as_str();
                        let replacement = format!(
                            "@acp:{} {} - {}",
                            annotation_type,
                            annotation_value.trim(),
                            directive
                        );
                        line.replace(full_match.trim(), &replacement)
                    };

                    migrations.push(AnnotationMigration {
                        file: file_path.to_path_buf(),
                        line: line_num + 1,
                        original,
                        migrated,
                        annotation_type,
                        annotation_value,
                    });
                }
            }
        }

        Ok(migrations)
    }

    /// Scan all files in the cache
    pub fn scan_cache(&self, cache: &Cache, filter_paths: &[PathBuf]) -> Result<Vec<AnnotationMigration>> {
        let mut all_migrations = vec![];

        for path in cache.files.keys() {
            let file_path = PathBuf::from(path);

            // Apply path filter if specified
            if !filter_paths.is_empty() {
                let matches = filter_paths.iter().any(|p| {
                    file_path.starts_with(p) || path.starts_with(p.to_string_lossy().as_ref())
                });
                if !matches {
                    continue;
                }
            }

            // Skip if file doesn't exist (cache might be stale)
            if !file_path.exists() {
                continue;
            }

            match self.scan_file(&file_path) {
                Ok(migrations) => all_migrations.extend(migrations),
                Err(e) => {
                    eprintln!("Warning: Could not scan {}: {}", path, e);
                }
            }
        }

        // Sort by file and line
        all_migrations.sort_by(|a, b| {
            a.file.cmp(&b.file).then(a.line.cmp(&b.line))
        });

        Ok(all_migrations)
    }
}

impl Default for MigrationScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply migrations to source files
pub struct MigrationWriter {
    backup_dir: PathBuf,
}

impl MigrationWriter {
    pub fn new() -> Self {
        Self {
            backup_dir: PathBuf::from(".acp/backups"),
        }
    }

    /// Create backup of a file before modification
    fn backup_file(&self, file_path: &Path) -> Result<()> {
        fs::create_dir_all(&self.backup_dir)?;

        let backup_name = format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            file_path.file_name().unwrap().to_string_lossy()
        );
        let backup_path = self.backup_dir.join(backup_name);

        fs::copy(file_path, backup_path)?;
        Ok(())
    }

    /// Apply a set of migrations to a single file
    pub fn apply_migrations(
        &self,
        file_path: &Path,
        migrations: &[&AnnotationMigration],
        backup: bool,
    ) -> Result<()> {
        if migrations.is_empty() {
            return Ok(());
        }

        // Create backup if requested
        if backup {
            self.backup_file(file_path)?;
        }

        // Read file content
        let content = fs::read_to_string(file_path)?;
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Apply migrations in reverse order (to preserve line numbers)
        let mut sorted_migrations: Vec<_> = migrations.iter().collect();
        sorted_migrations.sort_by(|a, b| b.line.cmp(&a.line));

        for migration in sorted_migrations {
            let line_idx = migration.line - 1;
            if line_idx < lines.len() {
                lines[line_idx] = migration.migrated.clone();
            }
        }

        // Write back
        let new_content = lines.join("\n");
        fs::write(file_path, new_content)?;

        Ok(())
    }
}

impl Default for MigrationWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Print migration preview (dry-run output)
pub fn print_migration_preview(migrations: &[AnnotationMigration]) {
    if migrations.is_empty() {
        println!("{}", style("No annotations need migration.").green());
        return;
    }

    println!(
        "Would update {} annotations:\n",
        style(migrations.len()).bold()
    );

    let mut current_file: Option<&PathBuf> = None;

    for migration in migrations {
        // Print file header when file changes
        if current_file != Some(&migration.file) {
            if current_file.is_some() {
                println!();
            }
            println!("  {}:{}",
                style(migration.file.display()).cyan(),
                migration.line
            );
            current_file = Some(&migration.file);
        } else {
            println!("  {}:{}",
                style(migration.file.display()).cyan(),
                migration.line
            );
        }

        // Print diff
        println!("    {} {}", style("-").red(), migration.original.trim());
        println!("    {} {}", style("+").green(), migration.migrated.trim());
        println!();
    }

    println!("{}", style("Run without --dry-run to apply changes.").dim());
}

/// Execute the migrate command
pub fn execute_migrate(cache: &Cache, options: MigrateOptions) -> Result<()> {
    let scanner = MigrationScanner::new();
    let migrations = scanner.scan_cache(cache, &options.paths)?;

    if options.dry_run {
        print_migration_preview(&migrations);
        return Ok(());
    }

    if migrations.is_empty() {
        println!("{}", style("No annotations need migration.").green());
        return Ok(());
    }

    // Group migrations by file
    let mut by_file: HashMap<PathBuf, Vec<&AnnotationMigration>> = HashMap::new();
    for migration in &migrations {
        by_file.entry(migration.file.clone()).or_default().push(migration);
    }

    let writer = MigrationWriter::new();
    let mut applied_count = 0;
    let mut skipped_count = 0;

    for (file_path, file_migrations) in &by_file {
        // Interactive confirmation
        if options.interactive {
            println!("\n{} ({} annotations):",
                style(file_path.display()).cyan().bold(),
                file_migrations.len()
            );

            for m in file_migrations.iter() {
                println!("  Line {}: @acp:{} {}", m.line, m.annotation_type, m.annotation_value);
            }

            let confirmed = Confirm::new()
                .with_prompt("Apply these migrations?")
                .default(true)
                .interact()
                .map_err(|e| std::io::Error::other(e.to_string()))?;

            if !confirmed {
                skipped_count += file_migrations.len();
                continue;
            }
        }

        // Apply migrations
        match writer.apply_migrations(file_path, file_migrations, options.backup) {
            Ok(()) => {
                applied_count += file_migrations.len();
                println!(
                    "{} Updated {} ({} annotations)",
                    style("✓").green(),
                    file_path.display(),
                    file_migrations.len()
                );
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to update {}: {}",
                    style("✗").red(),
                    file_path.display(),
                    e
                );
                skipped_count += file_migrations.len();
            }
        }
    }

    println!();
    println!(
        "{} Applied {} migrations, skipped {}",
        style("Done.").bold(),
        style(applied_count).green(),
        style(skipped_count).yellow()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directive_defaults() {
        let defaults = DirectiveDefaults::new();

        assert_eq!(
            defaults.get("lock", "frozen"),
            Some("MUST NOT modify this code under any circumstances".to_string())
        );

        assert_eq!(
            defaults.get("hack", ""),
            Some("Temporary workaround - check expiry before modifying".to_string())
        );

        assert_eq!(
            defaults.get("ref", "https://docs.example.com"),
            Some("Consult https://docs.example.com before making changes".to_string())
        );
    }

    #[test]
    fn test_migration_scanner_pattern() {
        let scanner = MigrationScanner::new();

        // These should match (no directive)
        assert!(scanner.pattern.is_match("// @acp:lock frozen"));
        assert!(scanner.pattern.is_match("// @acp:hack"));

        // Verify capture groups
        let cap = scanner.pattern.captures("// @acp:lock frozen").unwrap();
        assert_eq!(cap.get(1).unwrap().as_str(), "lock");
        assert_eq!(cap.get(2).unwrap().as_str(), "frozen");
    }
}

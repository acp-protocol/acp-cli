//! @acp:module "Query"
//! @acp:summary "Programmatic cache access and querying (schema-compliant)"
//! @acp:domain cli
//! @acp:layer service
//!
//! Provides type-safe queries similar to jq but in Rust.

use crate::cache::{Cache, SymbolEntry, FileEntry, DomainEntry};

/// Query builder for cache
pub struct Query<'a> {
    cache: &'a Cache,
}

impl<'a> Query<'a> {
    pub fn new(cache: &'a Cache) -> Self {
        Self { cache }
    }

    /// Get symbol by name
    pub fn symbol(&self, name: &str) -> Option<&SymbolEntry> {
        self.cache.get_symbol(name)
    }

    /// Get file by path with cross-platform path normalization
    ///
    /// Accepts various path formats:
    /// - `src/file.ts` or `./src/file.ts`
    /// - Windows paths: `src\file.ts`
    /// - Paths with `..`: `src/../src/file.ts`
    pub fn file(&self, path: &str) -> Option<&FileEntry> {
        self.cache.get_file(path)
    }

    /// Get callers of a symbol
    pub fn callers(&self, symbol: &str) -> Vec<&str> {
        self.cache.get_callers(symbol)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get callees of a symbol
    pub fn callees(&self, symbol: &str) -> Vec<&str> {
        self.cache.get_callees(symbol)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get domain by name
    pub fn domain(&self, name: &str) -> Option<&DomainEntry> {
        self.cache.domains.get(name)
    }

    /// Get all domains
    pub fn domains(&self) -> impl Iterator<Item = &DomainEntry> {
        self.cache.domains.values()
    }

    /// Get files by domain
    pub fn files_in_domain(&self, domain: &str) -> Vec<&str> {
        self.cache.get_domain_files(domain)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get files by layer (from file entries)
    pub fn files_in_layer(&self, layer: &str) -> Vec<&str> {
        self.cache.files.values()
            .filter(|f| f.layer.as_deref() == Some(layer))
            .map(|f| f.path.as_str())
            .collect()
    }

    /// Search symbols by name pattern
    pub fn search_symbols(&self, pattern: &str) -> Vec<&SymbolEntry> {
        let p = pattern.to_lowercase();
        self.cache.symbols.values()
            .filter(|s| s.name.to_lowercase().contains(&p))
            .collect()
    }

    /// Get hotpath symbols (symbols with many callers)
    pub fn hotpaths(&self) -> impl Iterator<Item = &str> {
        // Compute hotpaths from call graph
        self.cache.graph.as_ref()
            .map(|g| {
                let mut callee_counts: Vec<(&String, usize)> = g.reverse.iter()
                    .map(|(k, v)| (k, v.len()))
                    .collect();
                callee_counts.sort_by(|a, b| b.1.cmp(&a.1));
                callee_counts.into_iter()
                    .take(10)
                    .map(|(k, _)| k.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
    }
}

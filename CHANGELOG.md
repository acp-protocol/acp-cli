# Changelog

All notable changes to the ACP CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.1] - 2025-12-30

### Fixed
- Fixed crates.io packaging: schema files from acp-spec submodule are now properly included via build.rs

## [0.4.0] - 2025-12-30

### Added
- File-level module annotations (`@acp:module`) now generated for all files
- Module name inference from file paths (e.g., `_auth.py` → "Auth")

### Fixed
- File-level gaps with `symbol_kind = None` now handled correctly by heuristics engine

## [0.3.0] - 2025-12-22

### Added
- RFC-0005: Annotation provenance tracking (`@acp:source`, `@acp:source-reviewed`, `@acp:source-id`)
- RFC-0008: Type annotations support in CLI
- Annotate/documentation config support for project customization
- Multi-language support: TypeScript, JavaScript, Python, Rust, Go, Java

### Changed
- Use acp-spec submodule as single source of truth for JSON schemas
- Improved error messages for validate command on non-JSON files

### Fixed
- Resolved `-c` option conflict in CLI
- Submodule checkout in CI workflows
- NPM OIDC support and provenance for publishing

### Infrastructure
- Trusted publishing (no token dependency)
- Node 24 upgrade for publishing support
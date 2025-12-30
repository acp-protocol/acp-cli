# Changelog

All notable changes to the ACP CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.1] - 2025-12-30

### Added
- **RFC-0004: Full primer implementation** with value-based section selection
  - 4-phase selection algorithm: required → conditional → safety → value-optimized
  - Multi-dimensional value scoring (safety, efficiency, accuracy, base)
  - 37 sections across 6 categories from `primer.defaults.json`
- New primer CLI options:
  - `-b, --budget` — Token budget (shorthand)
  - `-f, --format` — Output format (markdown, compact, json, text)
  - `-p, --preset` — Weight presets (safe, efficient, accurate, balanced)
  - `--include`, `--exclude` — Section inclusion/exclusion
  - `--categories` — Category filtering
  - `--no-dynamic` — Disable dynamic value modifiers
  - `--explain` — Show selection reasoning
  - `--list-sections` — List available sections
  - `--list-presets` — List weight presets
  - `--preview` — Preview selection without rendering
  - `--primer-config` — Custom primer configuration file
- Dynamic sections populated from cache (protected files, domains, hacks, attempts)
- Project customization via `.acp/primer.json`
- MCP/shell capability filtering

### Changed
- Primer tier names: survival, essential, operational, informed, complete, expert
- Primer now uses value-per-token optimization instead of fixed tier system

### Fixed
- `acp constraints` → `acp check` in primer.defaults.json (4 locations)
- MCP capability filtering now properly excludes shell-only sections
- Fixed crates.io packaging: primer.defaults.json now included via build.rs

## [0.5.0] - 2025-12-30 [YANKED]

Yanked due to missing primer.defaults.json in crates.io package.

## [0.4.2] - 2025-12-30

### Fixed
- Fixed crates.io packaging: checkout submodule and use --allow-dirty to include generated schemas

## [0.4.1] - 2025-12-30 [YANKED]

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
# ACP CLI 0.6.0 Release Notes

**Release Date:** 2025-12-31

## Overview

ACP CLI 0.6.0 implements RFC-0015 "Primer System Redesign: Accuracy-Focused, Context-Aware Bootstrap" - a comprehensive enhancement to the primer and context systems that improves AI agent accuracy by providing operation-specific context.

## Highlights

### Tiered Primer System

The primer command now uses a 4-tier system for automatic content selection based on token budget:

| Tier | Budget | CLI Tokens | Use Case |
|------|--------|------------|----------|
| Micro | <300 | ~250 | Essential safety constraints only |
| Minimal | 300-449 | ~400 | Core project context |
| Standard | 450-699 | ~600 | Balanced context with conventions |
| Full | ≥700 | ~1400 | Complete project understanding |

```bash
# Select tier automatically based on budget
acp primer --budget 500  # Standard tier
acp primer --budget 800  # Full tier
```

### New `acp context` Command

Operation-specific context for AI agents:

```bash
# Creating new files - get naming conventions and import style
acp context create

# Modifying files - see constraints and dependent files
acp context modify --file src/auth/login.ts

# Debugging - related files and symbols
acp context debug --file src/utils/helpers.ts

# Exploring - project overview and domains
acp context explore
```

### IDE Environment Detection

Automatic detection of IDE environments (Cursor, VS Code, Cline, JetBrains, Zed) with warnings when using `--standalone` flag unnecessarily:

```bash
# In Cursor (detected automatically)
acp primer --standalone
# Warning: --standalone flag used in Cursor environment...
```

Set `ACP_NO_IDE_DETECT=1` to disable IDE detection.

### MCP `acp_context` Tool

New MCP tool for operation-specific context:

```json
{
  "name": "acp_context",
  "arguments": {
    "operation": "modify",
    "file": "src/auth/login.ts"
  }
}
```

## New Features

### Naming Convention Detection

- Auto-detects file naming patterns per directory (≥70% confidence threshold)
- Identifies anti-patterns (similar conventions NOT used)
- Stored in cache as `conventions.file_naming`

### Import Tracking (Importers)

- Tracks which files import each module
- `imported_by` field in cache file entries
- Essential for `acp context modify` to show affected files

### Cache Enhancements

New fields in `.acp.cache.json`:

```json
{
  "stats": {
    "primary_language": "TypeScript",
    "languages": [
      { "name": "TypeScript", "files": 150, "percentage": 87 }
    ]
  },
  "conventions": {
    "file_naming": [
      { "directory": "src/routes", "pattern": "*.ts", "confidence": 0.95 }
    ],
    "imports": {
      "module_system": "esm",
      "path_style": "relative"
    }
  }
}
```

## Breaking Changes

### Minor Breaking Changes

1. **Cache schema v1.1.0**: Additive changes - existing caches remain valid
2. **Primer output format**: Now includes tier information in header
3. **Default budget behavior**: Budget now determines tier selection

### Migration

No migration required. All changes are backwards compatible:
- Existing caches will work without the new optional fields
- Primer command defaults maintain previous behavior

## Package Versions

- `acp-protocol`: 0.6.0
- `acp-mcp`: 0.2.0
- Schema version: 1.1.0

## Dependencies

No new external dependencies.

## Upgrading

```bash
# From cargo
cargo install acp-protocol

# From source
git pull
cargo build --release
```

## Documentation

- RFC-0015: Full specification at `acp-spec/rfcs/rfc-0015-primer-redesign.md`
- Updated CLI help: `acp primer --help`, `acp context --help`
- Updated README with tier table and IDE integration examples

## Contributors

Thanks to all contributors who made this release possible.

---

*Full changelog: [CHANGELOG.md](CHANGELOG.md)*

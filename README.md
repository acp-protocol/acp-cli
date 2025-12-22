# ACP CLI

Command-line interface for the [AI Context Protocol](../README.md) — index your codebase, generate variables, and manage AI behavioral constraints.

[![Crate](https://img.shields.io/crates/v/acp.svg)](https://crates.io/crates/acp)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)

---

## Installation

### Building from Source (Current)

```bash
# Clone the repository
git clone https://github.com/acp-protocol/acp-spec.git
cd acp-spec/cli

# Build release binary
cargo build --release

# Install to PATH
cargo install --path .
```

### Homebrew (Coming Soon)

```bash
brew tap acp-protocol/tap
brew install acp-cli
```

### npm (Coming Soon)

```bash
npm install -g @acp-protocol/cli
```

### Pre-built Binaries (Coming Soon)

Pre-built binaries for macOS, Linux, and Windows will be available on the [Releases](https://github.com/acp-protocol/acp-spec/releases) page.

---

## Quick Start

### 1. Index Your Codebase

```bash
cd your-project
acp index
```

This generates `.acp.cache.json` with your codebase structure, symbols, and constraints.

### 2. Generate Annotations (Optional)

```bash
# Preview annotation suggestions
acp annotate

# Apply annotations to files
acp annotate --apply
```

This analyzes your codebase and suggests ACP annotations based on doc comments and heuristics.

### 3. Generate Variables

```bash
acp index --vars
# Or separately:
acp vars
```

This creates `.acp.vars.json` with token-efficient variable definitions.

### 4. Query the Cache

```bash
# Show stats
acp query stats

# Look up a symbol
acp query symbol validateSession

# List domains
acp query domains
```

---

## Commands

### Global Options

```
-c, --config <path>    Config file path [default: .acp.config.json]
-v, --verbose          Enable verbose output
-h, --help             Print help
-V, --version          Print version
```

---

### `acp index`

Index the codebase and generate `.acp.cache.json`.

```bash
acp index [ROOT] [OPTIONS]

Arguments:
  ROOT    Root directory to index [default: .]

Options:
  -o, --output <path>    Output cache file [default: .acp.cache.json]
      --vars             Also generate vars file
```

**Examples:**

```bash
# Index current directory
acp index

# Index specific directory with vars
acp index ./src --vars

# Custom output path
acp index -o build/cache.json
```

---

### `acp annotate`

Generate ACP annotations from code analysis and documentation conversion.

```bash
acp annotate [PATH] [OPTIONS]

Arguments:
  PATH    Path to analyze (file or directory) [default: .]

Options:
      --apply                   Apply changes to files (default: preview only)
      --convert                 Convert-only mode: only use doc comment conversion
      --from <SOURCE>           Source documentation standard [default: auto]
                                Values: auto, jsdoc, tsdoc, docstring, rustdoc, godoc, javadoc
      --level <LEVEL>           Annotation generation level [default: standard]
                                Values: minimal, standard, full
      --format <FORMAT>         Output format [default: diff]
                                Values: diff, json, summary
      --filter <PATTERN>        Filter files by glob pattern
      --files-only              Only annotate files (skip symbols)
      --symbols-only            Only annotate symbols (skip file-level)
      --check                   Exit with error if coverage below threshold (CI mode)
      --min-coverage <PERCENT>  Minimum coverage threshold [default: 80]
  -j, --workers <N>             Number of parallel workers [default: CPU count]
```

**Annotation Levels:**

| Level | Includes |
|-------|----------|
| `minimal` | `@acp:summary` only |
| `standard` | summary, domain, layer, lock |
| `full` | All annotation types including stability, ai-hint |

**Examples:**

```bash
# Preview annotations for current directory
acp annotate

# Apply annotations to files
acp annotate --apply

# Convert only existing doc comments (no heuristics)
acp annotate --convert --apply

# Generate minimal annotations from JSDoc
acp annotate --from jsdoc --level minimal

# CI mode: fail if coverage below 90%
acp annotate --check --min-coverage 90

# JSON output with breakdown
acp annotate --format json

# Summary with statistics
acp annotate --format summary

# Filter to specific files
acp annotate --filter "src/auth/**/*.ts"

# Parallel processing with 4 workers
acp annotate -j 4

# Only file-level annotations
acp annotate --files-only --apply
```

**Output Formats:**

| Format | Description |
|--------|-------------|
| `diff` | Unified diff showing proposed changes |
| `json` | Structured JSON with suggestions, confidence scores, and breakdown |
| `summary` | Statistics including coverage, type/source breakdown |

---

### `acp vars`

Generate `.acp.vars.json` from an existing cache.

```bash
acp vars [OPTIONS]

Options:
  -c, --cache <path>     Cache file to read [default: .acp.cache.json]
  -o, --output <path>    Output vars file [default: .acp.vars.json]
```

**Example:**

```bash
acp vars -c build/cache.json -o build/vars.json
```

---

### `acp query`

Query the cache for symbols, files, and metadata.

```bash
acp query <SUBCOMMAND> [OPTIONS]

Options:
  -c, --cache <path>    Cache file [default: .acp.cache.json]

Subcommands:
  symbol <name>     Query a symbol by name
  file <path>       Query a file by path
  callers <symbol>  Get callers of a symbol
  callees <symbol>  Get callees of a symbol
  domains           List all domains
  domain <name>     Query a specific domain
  hotpaths          List frequently-called symbols
  stats             Show aggregate statistics
```

**Examples:**

```bash
# Get symbol info as JSON
acp query symbol validateSession

# See what calls a function
acp query callers handleRequest

# List all domains
acp query domains

# Show codebase statistics
acp query stats
```

---

### `acp expand`

Expand variable references in text.

```bash
acp expand [TEXT] [OPTIONS]

Arguments:
  TEXT    Text to expand (reads from stdin if omitted)

Options:
  -m, --mode <mode>     Expansion mode [default: annotated]
                        Values: none, summary, inline, annotated, block, interactive
      --vars <path>     Vars file [default: .acp.vars.json]
      --chains          Show inheritance chains
```

**Examples:**

```bash
# Expand inline
acp expand "Check \$SYM_VALIDATE_SESSION"

# Pipe from stdin
echo "See \$ARCH_AUTH_FLOW" | acp expand --mode block

# Show variable inheritance
acp expand "\$SYM_HANDLER" --chains
```

**Expansion Modes:**

| Mode | Description |
|------|-------------|
| `none` | Keep `$VAR` references as-is |
| `summary` | Replace with summary only |
| `inline` | Replace with full value |
| `annotated` | Show `**$VAR** → value` |
| `block` | Full formatted block |
| `interactive` | HTML-like markers for UI |

---

### `acp chain`

Show variable inheritance chain.

```bash
acp chain <NAME> [OPTIONS]

Arguments:
  NAME    Variable name (with or without $)

Options:
      --vars <path>    Vars file [default: .acp.vars.json]
      --tree           Display as tree
```

**Examples:**

```bash
# Show chain
acp chain SYM_AUTH_HANDLER

# Show as tree
acp chain $ARCH_PAYMENT --tree
```

---

### `acp attempt`

Manage troubleshooting attempts for debugging sessions.

```bash
acp attempt <SUBCOMMAND>

Subcommands:
  start <id>          Start a new attempt
  list                List attempts
  fail <id>           Mark attempt as failed
  verify <id>         Mark attempt as verified (success)
  revert <id>         Revert an attempt's changes
  cleanup             Clean up all failed attempts
  checkpoint <name>   Create a checkpoint
  checkpoints         List all checkpoints
  restore <name>      Restore to a checkpoint
```

**Attempt workflow:**

```bash
# Start debugging
acp attempt start auth-fix-001 -f "BUG-123" -d "Fixing 401 errors"

# If it fails
acp attempt fail auth-fix-001 --reason "Broke login flow"
acp attempt revert auth-fix-001

# If it works
acp attempt verify auth-fix-001

# Clean up all failed attempts
acp attempt cleanup
```

**Checkpoint workflow:**

```bash
# Create checkpoint before risky changes
acp attempt checkpoint before-refactor -f src/auth.ts -f src/session.ts

# List checkpoints
acp attempt checkpoints

# Restore if needed
acp attempt restore before-refactor
```

---

### `acp check`

Check guardrails for a file.

```bash
acp check <FILE> [OPTIONS]

Arguments:
  FILE    File to check

Options:
  -c, --cache <path>    Cache file [default: .acp.cache.json]
```

**Example:**

```bash
acp check src/auth/session.ts
```

**Output:**

```
✓ Guardrails check passed

Warnings:
  ⚠ [ai-careful] Extra caution required: security-critical code

Required Actions:
  → flag-for-review - Requires security review
```

---

### `acp revert`

Revert changes from an attempt or restore a checkpoint.

```bash
acp revert [OPTIONS]

Options:
      --attempt <id>        Attempt ID to revert
      --checkpoint <name>   Checkpoint name to restore
```

**Examples:**

```bash
# Revert a failed attempt
acp revert --attempt auth-fix-001

# Restore to checkpoint
acp revert --checkpoint before-refactor
```

---

### `acp watch`

Watch for file changes and update cache in real-time.

```bash
acp watch [ROOT]

Arguments:
  ROOT    Directory to watch [default: .]
```

**Example:**

```bash
acp watch ./src
```

---

### `acp validate`

Validate cache or vars files against the schema.

```bash
acp validate <FILE>

Arguments:
  FILE    File to validate (.acp.cache.json or .acp.vars.json)
```

**Examples:**

```bash
acp validate .acp.cache.json
acp validate .acp.vars.json
```

---

## Configuration

Create `.acp.config.json` in your project root:

```json
{
  "include": ["src/**/*", "lib/**/*"],
  "exclude": ["**/node_modules/**", "**/dist/**", "**/*.test.*"],
  "languages": ["typescript", "javascript", "rust", "python"],
  "output": {
    "cache": ".acp.cache.json",
    "vars": ".acp.vars.json"
  }
}
```

See the [config schema](../schemas/v1/config.schema.json) for all options.

---

## jq Quick Reference

Query the cache directly with jq:

```bash
# Check if you can modify a file
jq '.constraints.by_file["src/auth/session.ts"].mutation.level' .acp.cache.json

# Get all frozen files
jq '.constraints.by_lock_level.frozen' .acp.cache.json

# Find expired hacks
jq '.constraints.hacks | map(select(.expires < now | todate))' .acp.cache.json

# Get symbol info
jq '.symbols["validateSession"]' .acp.cache.json

# List all domains
jq '.domains | keys' .acp.cache.json

# Get files in a domain
jq '.domains.auth.files' .acp.cache.json

# Show codebase stats
jq '.stats' .acp.cache.json
```

---

## MCP Integration (Coming Soon)

MCP (Model Context Protocol) server integration is planned to provide AI tools with direct access to:

- **acp_constraints** — Check file constraints before modification
- **acp_query** — Query symbols, files, and domains
- **acp_debug** — Track debugging attempts
- **acp_vars** — Expand variable references

See the [roadmap](../docs/roadmap.md) for status.

---

## Key Annotations

| Annotation | Purpose | AI Behavior |
|------------|---------|-------------|
| `@acp:lock frozen` | Never modify | Refuses all changes |
| `@acp:lock restricted` | Explain first | Describes changes before making them |
| `@acp:lock approval-required` | Ask permission | Waits for explicit approval |
| `@acp:style <guide>` | Follow style guide | Uses specified conventions |
| `@acp:ref <url>` | Documentation reference | Can fetch and consult |
| `@acp:hack` | Temporary code | Tracks for cleanup |
| `@acp:debug-session` | Debug tracking | Logs attempts for reversal |

See the [Annotation Reference](../spec/chapters/annotations.md) for the complete list.

---

## Related Documentation

- [ACP Specification](../spec/ACP-1.0.md) — Complete protocol specification
- [Root README](../README.md) — Project overview and quick start
- [JSON Schemas](../schemas/) — Schema definitions for all file formats
- [Annotation Reference](../spec/chapters/annotations.md) — All annotation types

---

## License

MIT — see [LICENSE](../LICENSE)

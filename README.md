# ACP CLI

Command-line interface for the [AI Context Protocol](https://github.com/acp-protocol/acp-spec) — index your codebase, generate variables, and manage AI behavioral constraints.

[![Crate](https://img.shields.io/crates/v/acp-protocol.svg)](https://crates.io/crates/acp-protocol)
[![CI](https://github.com/acp-protocol/acp-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/acp-protocol/acp-cli/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

---

## Installation

### From crates.io (Recommended)

```bash
cargo install acp-protocol
```

This installs the `acp` binary to your Cargo bin directory.

### From Homebrew

```bash
brew tap acp-protocol/tap
brew install acp
```

### From npm

```bash
npm install -g @acp-protocol/acp
```

### Pre-built Binaries

Pre-built binaries for macOS, Linux, and Windows are available on the [Releases](https://github.com/acp-protocol/acp-cli/releases) page.

### Building from Source

```bash
git clone https://github.com/acp-protocol/acp-cli.git
cd acp-cli
cargo build --release
cargo install --path .
```

---

## Quick Start

### 1. Initialize Your Project

```bash
cd your-project
acp init
```

This creates `.acp.config.json` and optionally bootstraps AI tool configurations (CLAUDE.md, .cursorrules).

### 2. Index Your Codebase

```bash
acp index
```

This generates `.acp/acp.cache.json` with your codebase structure, symbols, and constraints.

### 3. Generate Annotations (Optional)

```bash
# Preview annotation suggestions
acp annotate

# Apply annotations to files
acp annotate --apply
```

This analyzes your codebase and suggests ACP annotations based on doc comments and heuristics.

### 4. Generate Variables

```bash
acp index --vars
# Or separately:
acp vars
```

This creates `.acp/acp.vars.json` with token-efficient variable definitions.

### 5. Query the Cache

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

### `acp init`

Initialize a new ACP project with configuration and optional AI tool bootstrapping.

```bash
acp init [OPTIONS]

Options:
  -f, --force              Force overwrite existing config
      --include <PATTERN>  File patterns to include (can specify multiple)
      --exclude <PATTERN>  File patterns to exclude (can specify multiple)
  -o, --output <PATH>      Config file output path [default: .acp.config.json]
      --no-bootstrap       Skip AI tool bootstrap (CLAUDE.md, .cursorrules, etc.)
  -y, --yes                Skip interactive prompts (use defaults + CLI args)
```

**Examples:**

```bash
# Interactive initialization
acp init

# Non-interactive with custom patterns
acp init -y --include "src/**/*" --exclude "**/test/**"

# Skip AI tool bootstrapping
acp init --no-bootstrap
```

---

### `acp install`

Install ACP plugins (daemon, MCP server).

```bash
acp install <TARGETS>... [OPTIONS]

Arguments:
  TARGETS    Plugins to install (daemon, mcp)

Options:
  -f, --force              Force reinstall
      --version <VERSION>  Specific version [default: latest]
      --list               List installed plugins
      --uninstall          Uninstall specified plugins
```

**Examples:**

```bash
# Install daemon and MCP server
acp install daemon mcp

# List installed plugins
acp install --list

# Uninstall a plugin
acp install daemon --uninstall
```

---

### `acp index`

Index the codebase and generate `.acp/acp.cache.json`.

```bash
acp index [ROOT] [OPTIONS]

Arguments:
  ROOT    Root directory to index [default: .]

Options:
  -o, --output <path>    Output cache file [default: .acp/acp.cache.json]
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
```

---

### `acp review`

Review auto-generated annotations (RFC-0003).

```bash
acp review <SUBCOMMAND> [OPTIONS]

Subcommands:
  list         List annotations needing review
  mark         Mark annotations as reviewed
  interactive  Interactive review mode

Options:
      --source <SOURCE>          Filter by source (explicit, converted, heuristic, refined, inferred)
      --confidence <EXPR>        Filter by confidence (e.g., "<0.7", ">=0.9")
      --cache <PATH>             Cache file path [default: .acp/acp.cache.json]
      --json                     Output as JSON
```

**Examples:**

```bash
# List low-confidence annotations
acp review list --confidence "<0.7"

# Interactive review mode
acp review interactive

# Mark specific annotations as reviewed
acp review mark --source heuristic
```

---

### `acp vars`

Generate `.acp/acp.vars.json` from an existing cache.

```bash
acp vars [OPTIONS]

Options:
  -c, --cache <path>     Cache file to read [default: .acp/acp.cache.json]
  -o, --output <path>    Output vars file [default: .acp/acp.vars.json]
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
  -c, --cache <path>    Cache file [default: .acp/acp.cache.json]

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
      --vars <path>     Vars file [default: .acp/acp.vars.json]
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
      --vars <path>    Vars file [default: .acp/acp.vars.json]
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

### `acp daemon`

Manage the ACP daemon (HTTP REST API).

```bash
acp daemon <SUBCOMMAND>

Subcommands:
  start   Start the ACP daemon
  stop    Stop the ACP daemon
  status  Check daemon status
  logs    Show daemon logs
```

**Examples:**

```bash
# Start the daemon
acp daemon start

# Check status
acp daemon status

# View logs
acp daemon logs
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
  -c, --cache <path>    Cache file [default: .acp/acp.cache.json]
```

**Example:**

```bash
acp check src/auth/session.ts
```

**Output:**

```
Guardrails check passed

Warnings:
  [ai-careful] Extra caution required: security-critical code

Required Actions:
  -> flag-for-review - Requires security review
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
  FILE    File to validate (.acp/acp.cache.json or .acp/acp.vars.json)
```

**Examples:**

```bash
acp validate .acp/acp.cache.json
acp validate .acp/acp.vars.json
```

---

### `acp primer`

Generate AI bootstrap primers with tiered content selection.

```bash
acp primer [OPTIONS]

Options:
      --budget <N>           Token budget [default: 200]
      --capabilities <caps>  Capabilities filter (comma-separated: shell,mcp)
      --json                 Output as JSON with metadata
  -c, --cache <path>         Cache file for project warnings [default: .acp/acp.cache.json]
```

**Tier Selection:**

| Remaining Budget | Tier | Content Depth |
|------------------|------|---------------|
| <80 tokens | minimal | Command + one-line purpose |
| 80-299 tokens | standard | + options, usage |
| 300+ tokens | full | + examples, patterns |

**Examples:**

```bash
# Standard primer (200 tokens)
acp primer

# Minimal primer
acp primer --budget 60

# Full primer with project warnings
acp primer --budget 500

# JSON output with metadata
acp primer --budget 200 --json

# Filter by capability
acp primer --capabilities shell
```

---

## Configuration

Create `.acp.config.json` in your project root (or run `acp init`):

```json
{
  "include": ["src/**/*", "lib/**/*"],
  "exclude": ["**/node_modules/**", "**/dist/**", "**/*.test.*"],
  "languages": ["typescript", "javascript", "rust", "python"],
  "output": {
    "cache": ".acp/acp.cache.json",
    "vars": ".acp/acp.vars.json"
  }
}
```

See the [config schema](https://github.com/acp-protocol/acp-spec/blob/main/schemas/v1/config.schema.json) for all options.

---

## jq Quick Reference

Query the cache directly with jq:

```bash
# Check if you can modify a file
jq '.constraints.by_file["src/auth/session.ts"].mutation.level' .acp/acp.cache.json

# Get all frozen files
jq '.constraints.by_lock_level.frozen' .acp/acp.cache.json

# Find expired hacks
jq '.constraints.hacks | map(select(.expires < now | todate))' .acp/acp.cache.json

# Get symbol info
jq '.symbols["validateSession"]' .acp/acp.cache.json

# List all domains
jq '.domains | keys' .acp/acp.cache.json

# Get files in a domain
jq '.domains.auth.files' .acp/acp.cache.json

# Show codebase stats
jq '.stats' .acp/acp.cache.json
```

---

## MCP Integration

The ACP MCP server provides AI tools with direct access to your codebase context through the [Model Context Protocol](https://modelcontextprotocol.io/).

Install and run via:

```bash
acp install mcp
```

Or install separately from [acp-mcp](https://github.com/acp-protocol/acp-mcp).

**Available MCP Tools:**

- **acp_query** — Query symbols, files, and domains
- **acp_constraints** — Check file constraints before modification
- **acp_primer** — Generate context primers
- **acp_expand** — Expand variable references

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

### Type Annotations (RFC-0008)

Optional type syntax for documenting parameters and return types:

```typescript
/**
 * @acp:fn "authenticate" - User authentication handler
 * @acp:template T extends User - User type for response
 * @acp:param {string} email - Valid email address
 * @acp:param {string} password - Password meeting requirements
 * @acp:param {AuthOptions} [options={}] - Optional auth settings
 * @acp:returns {Promise<T | null>} - User object or null if failed
 */
async function authenticate<T extends User>(
  email: string,
  password: string,
  options?: AuthOptions
): Promise<T | null> { }
```

**Syntax:**
- `@acp:param {Type} name - directive` — Parameter with type
- `@acp:param {Type} [name] - directive` — Optional parameter
- `@acp:param {Type} [name=default] - directive` — Parameter with default
- `@acp:returns {Type} - directive` — Return type
- `@acp:template T extends Constraint - directive` — Generic type parameter

Types are optional and stored in the cache's `type_info` field.

See the [Annotation Reference](https://github.com/acp-protocol/acp-spec/blob/main/spec/chapters/annotations.md) for the complete list.

---

## Related Documentation

- [ACP Specification](https://github.com/acp-protocol/acp-spec/blob/main/spec/ACP-1.0.md) — Complete protocol specification
- [JSON Schemas](https://github.com/acp-protocol/acp-spec/tree/main/schemas) — Schema definitions for all file formats
- [Annotation Reference](https://github.com/acp-protocol/acp-spec/blob/main/spec/chapters/annotations.md) — All annotation types

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## License

MIT — see [LICENSE](LICENSE)

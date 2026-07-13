# Configuration (`.codeindex.toml`)

Place a `.codeindex.toml` at the repo root to customise indexing, MCP output, security, and extraction. All fields are optional — without this file, Codebeacon uses sensible defaults.

## Full example

```toml
# Additional directories to skip during indexing
extra_ignore_dirs = ["my_build", "tmp"]

# Glob patterns for files to ignore
ignore_globs = ["**/*.generated.cs"]

# If set, only these languages are indexed (case-insensitive)
languages = ["rust", "go"]

# LSP worker pool size per language (default: 2)
lsp_concurrency = 4

# Seconds to spend enriching the heuristic index with LSP definition calls
# to discover additional dependency edges (default: 60, set 0 to disable)
lsp_enrich_timeout_secs = 30

[extractor]
mode = "auto"              # regex | tree-sitter | auto
parse_timeout_ms = 50
max_tree_sitter_bytes = 512000

[compact]
enabled = true             # false → legacy verbose MCP JSON (e.g. local LLMs)

[security]
enabled = false            # or: codebeacon serve --security
mode = "balanced"          # strict | balanced | permissive
z3_timeout_ms = 5000
block_on_unknown = false
# enabled_cwes = ["190", "131", "134"]   # empty → Z3 CWEs on, pattern CWEs off

[lsp]
# Override LSP binary per language (e.g. use OmniSharp instead of csharp-ls)
overrides = { csharp = "OmniSharp" }
```

## Top-level fields

| Field | Default | Description |
|-------|---------|-------------|
| `extra_ignore_dirs` | `[]` | Directory names to skip in addition to built-in ignores (`node_modules`, `target`, etc.) |
| `ignore_globs` | `[]` | Glob patterns for files to exclude |
| `languages` | `[]` (all) | Restrict indexing to listed languages: `rust`, `go`, `python`, `typescript`, `csharp` |
| `lsp_concurrency` | `2` | Concurrent LSP workers per language |
| `lsp_enrich_timeout_secs` | `60` | Background LSP enrichment budget; `0` disables |

## `[extractor]`

| Field | Default | Description |
|-------|---------|-------------|
| `mode` | `auto` | `regex`, `tree-sitter`, or `auto` (tree-sitter when built with `--features tree-sitter`) |
| `parse_timeout_ms` | `50` | Per-file parse budget; falls back to regex on timeout |
| `max_tree_sitter_bytes` | `512000` | Skip tree-sitter above this file size |

See [BENCHMARKS.md](BENCHMARKS.md) for regex vs tree-sitter performance notes.

## `[compact]`

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | When true, MCP index/graph tools return short JSON keys and path dictionary refs (`p1`, `s1`) |

Per-call override: pass `"compact": false` on any MCP tool. See [BENCHMARKS.md](BENCHMARKS.md#compact-dictionary-mode-default-on).

## `[security]`

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Enable Z3 security gate on MCP `write_file` / `edit_file` (or use `serve --security`) |
| `mode` | `balanced` | `strict`, `balanced`, or `permissive` |
| `z3_timeout_ms` | `5000` | Z3 solver timeout per fragment |
| `block_on_unknown` | `false` | Block when Z3 returns unknown |
| `enabled_cwes` | `[]` | Per-CWE toggle; empty means Z3 CWEs (190/131/191/369/680) on, pattern CWEs off |

`codebeacon verify` always runs verification regardless of `enabled`.

Full hook and client coverage matrix: [SECURITY_EDIT_PATHS.md](SECURITY_EDIT_PATHS.md).

## `[lsp]`

| Field | Default | Description |
|-------|---------|-------------|
| `overrides` | `{}` | Map language id → LSP binary name (e.g. `{ csharp = "OmniSharp" }`) |

LSP binaries are only required for `find_definition` and `find_references`. Indexing uses regex (or tree-sitter) without LSP.

## Workspace root resolution

Codebeacon resolves the project root in this order:

| Priority | Source |
|----------|--------|
| 1 | `--root` CLI flag |
| 2 | MCP `roots/list` request to the client |
| 3 | `CLAUDE_PROJECT_DIR` env var (Claude Code) |
| 4 | `CURSOR_WORKSPACE` env var (Cursor) |
| 5 | Process `cwd` (VS Code, Zed, Cline) |

For a directory containing multiple git repos, Codebeacon serves all of them as a workspace. For a single git repo (or a path inside one), it serves that repo alone.

See [INSTALL.md](INSTALL.md) for per-client setup and manual `--root` override.

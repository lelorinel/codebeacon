# Changelog

All notable changes to Codebeacon are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - 2026-07-13

### Added

#### Security (CWE-190)

- **`codebeacon verify`** — standalone CLI for hooks, CI, and pre-commit checks (`--content`, `--path`, `--json`; exit 0 = allow/warn, exit 1 = block).
- **MCP security gate** — `write_file` and `edit_file` run verification when `codebeacon serve --security` is enabled or `[security] enabled = true` in `.codeindex.toml`.
- **`verify_security` MCP tool** — check a code fragment without writing to disk (available when security is enabled).
- **Z3 formal verification** (optional `security-z3` Cargo feature) — SAT/UNSAT/Unknown outcomes for integer-overflow allocation sites; pattern-only fallback when the feature is off.
- **Policy modes** — `strict`, `balanced` (default), `permissive`; configurable Z3 timeout and `block_on_unknown`.
- **Hook examples** — Cursor (`.cursor/hooks/`), Claude Code (`.claude/hooks/`), OpenCode (`docs/opencode-security.example.jsonc`).
- **Docs** — [SECURITY_EDIT_PATHS.md](docs/SECURITY_EDIT_PATHS.md).

#### CLI & distribution

- **`codebeacon install` / `uninstall`** — one-command setup for Cursor, Claude, Codex, OpenCode, Hermes, agents, and VS Code (`--project`, `--security`, `--fs-tools`).
- **`codebeacon report`** — generates `CODEBEACON_REPORT.md` with package overview and dependency hotspots.
- **Graph CLI** — `query`, `path`, `explain`, `dependents`.
- **`codebeacon export mermaid`** — dependency graph as `.codebeacon/dep-graph.mmd`.
- **`codebeacon hook install` / `uninstall`** — git post-commit re-index hook.
- **Agent skill** — `assets/skill/` (installed via `codebeacon install`).
- **Demo repo** — `worked/simple-rust/`.
- **Docs** — [INSTALL.md](docs/INSTALL.md), [TEAM.md](docs/TEAM.md), [BENCHMARKS.md](docs/BENCHMARKS.md), [ROADMAP.md](docs/ROADMAP.md).

#### MCP tools

- `query_context` — keyword search over packages, symbols, and files.
- `shortest_path` — shortest import dependency chain between two files.
- `hotspots` / `get_hotspots` — top files by reverse-dependency count.
- `get_report` — returns `CODEBEACON_REPORT.md` (resource: `codebeacon://report`).
- `get_index_summary` — returns L0 `index.json` (resource: `codebeacon://index`).

#### Extraction (tree-sitter)

- Optional **`tree-sitter` Cargo feature** — AST-based symbol and import extraction for Rust, Go, Python, TypeScript/JavaScript, and C#.
- **`[extractor]` config** in `.codeindex.toml` — `mode` (`regex` | `tree-sitter` | `auto`), `parse_timeout_ms`, `max_tree_sitter_bytes`.
- Regex remains the default and automatic fallback on parse error, timeout, or oversized files.
- **C# `using` imports** — extracted for dependency graph edges (regex path improved as well).

### Changed

- Extractor pipeline refactored into `src/extract/` with a unified regex + tree-sitter path.
- README quickstart now includes `codebeacon install` and graph CLI examples.

### Build

```bash
# Default — regex extraction, pattern-only security
cargo build --release

# Optional tree-sitter extraction
cargo build --release --features tree-sitter

# Optional Z3 CWE-190 proofs (requires libz3)
cargo build --release --features security-z3

# Both
cargo build --release --features "tree-sitter,security-z3"
```

---

## [0.1.x] — prior releases

Hierarchical live index, LSP enrichment, multi-repo workspaces, core MCP tools (`get_context`, `drill_package`, `find_definition`, `find_references`, `get_dependents`, `init_workspace`), optional `--fs-tools`, and `.codeindex.toml` configuration.

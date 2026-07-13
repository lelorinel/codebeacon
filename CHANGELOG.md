# Changelog

All notable changes to Codebeacon are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.3.0] - 2026-07-13

### Added

#### Compact dictionary (token compression)

- **Compact MCP responses (default on)** — MCP tool outputs use short JSON keys and dictionary refs to reduce token usage:
  - Short keys: `packages` → `pk`, `hot_symbols` → `hs`, `purpose` → `p`, `files` → `f`, `signature` → `g`, `kind` → `k`, etc.
  - Path/symbol refs: `p1` → `src/auth.rs`, `s1` → symbol entry (deterministic, stable across rebuilds when possible).
  - `SymbolKind` abbreviations: `fn`, `st`, `en`, `tr`, `md`, `vr`, `ot`.
- **Affected MCP tools** — `get_context`, `drill_package`, `query_context`, `get_index_summary`, `find_definition`, `find_references`, `shortest_path`, `hotspots`, `get_dependents` (each accepts optional `compact` arg).
- **`[compact] enabled`** in `.codeindex.toml` (default `true`); per-call `"compact": false` returns legacy verbose JSON (useful for local LLMs).
- **`.codeindex/dict.json`** — persistent path/symbol map written on index rebuild (`rev` increments on full/catchup/incremental rebuild).
- **Session dictionary** — compact responses include `dict` block; new paths/symbols in `drill_package` etc. may add `dict_delta`.
- **Input resolution** — tool args accept full paths or dict refs (`file: "p1"` or `from: "p1"`).
- **`.codeindex/usage.json`** — local tool-usage counters (`drill_package`, `find_definition`, `find_references`, `query_context`) boost `hot_symbols` ordering when compact is enabled (no external telemetry).
- **CLI** — `codebeacon query --compact` (boolean; defaults from config when omitted).
- **Backward compatible** — `compact: false` or `[compact] enabled = false` preserves the original `RepoIndex` / `PackageDetail` JSON schema; security `verify` fragments unchanged.

```toml
# .codeindex.toml
[compact]
enabled = true   # false → all MCP responses use legacy verbose format
```

#### Security (CWE expansion)

- **Z3 checks (enabled by default)** — fragment-based verification via the same `verify_fragment` path as CWE-190 (hooks, MCP gate, `codebeacon verify`):
  - **CWE-131** — two-variable buffer size (`malloc(a * b)`, `calloc(n, m)`)
  - **CWE-191** — integer underflow in allocation (`malloc(n - k)`)
  - **CWE-369** — divide-by-zero in allocation (`malloc(total / count)`)
  - **CWE-680** — buffer copy size overflow (`memcpy` / `memset` with `n * size`)
  - **CWE-190 (extended)** — bit-shift allocation overflow (`malloc(n << k)`)
- **Pattern-only checks (opt-in via config)**:
  - **CWE-134** — format string (`printf(var)`)
  - **CWE-78** — command injection (`system(var)`, `exec`, `popen`, `subprocess`)
  - **CWE-798** — hardcoded credentials
  - **CWE-502** — unsafe deserialization (`pickle.loads`, `yaml.load`, etc.)
  - **CWE-22** — path traversal heuristics
- **`enabled_cwes` config** — per-CWE toggle in `.codeindex.toml`; Z3 CWEs on by default, pattern CWEs off unless listed.
- **Line quick-reject** — security markers skip irrelevant lines before regex/Z3 (no perf hit on safe code).

### Changed

- **README restructured** — inverted-pyramid layout (~120 lines); client setup, full config, and relevance scoring moved to [INSTALL.md](docs/INSTALL.md), [CONFIG.md](docs/CONFIG.md), and [BENCHMARKS.md](docs/BENCHMARKS.md).
- **Compact mode docs** — [BENCHMARKS.md](docs/BENCHMARKS.md) token savings table, [mcp-tools.md](assets/skill/references/mcp-tools.md) compact usage guide.
- Security module refactored: `src/security/sites/` (allocation, subtraction, division, buffer_copy), `src/security/patterns/`, shared `src/security/z3/overflow.rs`.
- `verify_security`, MCP gate, and CLI messages generalized beyond CWE-190-only wording.
- Docs updated: [security.md](assets/skill/references/security.md), [BENCHMARKS.md](docs/BENCHMARKS.md), [CONFIG.md](docs/CONFIG.md), [INSTALL.md](docs/INSTALL.md).

## [0.2.0] - 2026-07-13

### Added

#### Security (CWE-190)

- **`codebeacon verify`** — standalone CLI for hooks, CI, and pre-commit checks (`--content`, `--path`, `--json`; exit 0 = allow/warn, exit 1 = block).
- **MCP security gate** — `write_file` and `edit_file` run verification when `codebeacon serve --security` is enabled or `[security] enabled = true` in `.codeindex.toml`.
- **`verify_security` MCP tool** — check a code fragment without writing to disk (available when security is enabled).
- **Z3 formal verification** (optional `security-z3` Cargo feature) — SAT/UNSAT/Unknown outcomes for integer-overflow allocation sites; pattern-only fallback when the feature is off.
- **Policy modes** — `strict`, `balanced` (default), `permissive`; configurable Z3 timeout and `block_on_unknown`.
- **Hook examples** — `assets/hooks/` (Cursor + Claude + OpenCode `docs/opencode-security.example.jsonc`).
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

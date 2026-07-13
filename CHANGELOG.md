# Changelog

All notable changes to Codebeacon are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.4.0] - 2026-07-13

### Added

#### Edit intelligence layer

- **`[intelligence]` config** in `.codeindex.toml` — `enabled`, `focus_default_radius`, `change_impact_high_ref_threshold`, `conventions_enabled`, `git_context_enabled` (all on by default).
- **MCP tools** — hidden from `tools/list` when `[intelligence] enabled = false`:
  - `focus_context` — BFS subgraph around a file (anchor package, neighbors, symbols).
  - `task_context` — keyword search + package drill summary; optional `file` for proximity boost.
  - `change_impact` — symbol blast radius (definition, references, dependent files, `low|medium|high` risk).
  - `index_status` — index freshness vs working tree (stale files, git dirty count).
  - `package_conventions` — per-package convention fingerprint (error style, logging, async, tests).
  - `similar_symbols` — lightweight similarity by kind and signature token overlap.
  - `api_surface` — public exports per package (language-specific heuristics).
  - `why_file` — recent git commits, blame snippet, and dependency summary.
  - `fragile_files` — high-churn files weighted by reverse-dependency count.
- **Index artifacts** — `.codeindex/conventions.json` written on rebuild; package `purpose` populated from top symbols + convention tags.
- **Graph** — bidirectional BFS scoring (`score_files_bidirectional`) for edit-time context.
- **Compact encode** — `focus_context`, `task_context`, and `change_impact` accept `compact` with short keys (`anc`, `nbr`, `rf`, etc.).
- **CLI** — `codebeacon focus`, `status`, `impact`, `api`, `why`.

```toml
[intelligence]
enabled = true
focus_default_radius = 2
change_impact_high_ref_threshold = 10
conventions_enabled = true
git_context_enabled = true
```

### Changed

- **README** — essential MCP tools table and CLI quick reference include edit-intelligence commands.
- **Docs** — [mcp-tools.md](assets/skill/references/mcp-tools.md) edit workflow (`index_status` → `focus_context` → `change_impact` → edit); [CONFIG.md](docs/CONFIG.md) `[intelligence]` section; [ROADMAP.md](docs/ROADMAP.md) change-impact item completed.

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

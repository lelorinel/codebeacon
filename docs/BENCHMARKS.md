# Codebeacon Benchmarks

## Token efficiency (L0 index)

| Approach | Typical tokens (500-file repo) |
|----------|-------------------------------|
| grep + read loops | 10,000–50,000+ |
| Full repo dump | 100,000+ |
| Codebeacon `get_context` (L0) | ~500 |
| Codebeacon `get_context` (compact, default) | ~350 |
| `drill_package` (one package) | ~1,000–3,000 |
| `drill_package` (compact) | ~500–1,500 |

Codebeacon's hierarchical index is designed so the L0 summary always fits in context. Packages below relevance 0.05 are omitted.

## Relevance scoring

Codebeacon resolves `import` / `use` / `require` statements to build a dependency graph at index time. When you open files, it runs BFS from those files through the graph:

| Hop distance | Score |
|--------------|-------|
| 0 — your file | 1.0 |
| 1 hop away | 0.5 |
| 2 hops | 0.25 |
| 3+ hops | 0.1 |

`index.json` is sorted by score. Packages below 0.05 are omitted so the map stays small regardless of repo size.

### Compact dictionary mode (default on)

MCP responses use short JSON keys (`pk`, `hs`, `f`, `sy`, …) and path/symbol dictionary refs (`p1`, `s1`) to cut repeated path tokens across a session.

| Response | Verbose (`compact: false`) | Compact (default) |
|----------|--------------------------|-------------------|
| L0 `get_context` | ~500 tokens | ~350 tokens |
| `drill_package` (medium package) | ~2–5k | ~1–2k |
| Repeated session calls | full path each time | dict ref (`p1`) |

Disable globally in `.codeindex.toml`:

```toml
[compact]
enabled = false   # use legacy verbose JSON for local LLMs
```

Per-call override: pass `"compact": false` on any index/graph MCP tool.

Persistent artifacts in `.codeindex/`:

- `dict.json` — deterministic path/symbol IDs (rebuilt on index refresh)
- `usage.json` — local tool-usage stats for adaptive `hot_symbols` ranking

## Session comparison (445-file monorepo)

| Approach | Tool calls | Files read | Tokens (est.) |
|----------|------------|------------|---------------|
| Claude without Codebeacon | 5+ | 3–10 | ~5,000–8,000 |
| Claude with Codebeacon | 2 | 0 | ~800–1,200 |

## Security benchmark

Security verification runs on **edit fragments** (hook `new_string`, MCP `edit_file` delta, or `verify --content`). No full-repo scan.

| Mode | Typical latency per fragment |
|------|------------------------------|
| Pattern-only CWEs (78, 134, 502, 798, 22) | <1 ms |
| Z3 CWEs (190, 131, 191, 369, 680) | 5–50 ms per symbolic site |

Z3 CWEs are enabled by default; pattern CWEs require `enabled_cwes` in config.

Run locally:

```bash
codebeacon verify --content 'malloc(x * sizeof(int))' --json
codebeacon verify --content 'printf(user_input);' --json  # needs enabled_cwes = ["134"]
```

## Extraction (regex vs tree-sitter)

Optional `tree-sitter` feature replaces line-regex symbol/import extraction for Rust, Go, Python, TypeScript, and C#. Regex remains the fallback on parse error, timeout, or large files.

| Scenario | Regex (default) | tree-sitter (`--features tree-sitter`) |
|----------|-----------------|----------------------------------------|
| Single file cold parse (~1k LOC Rust) | ~1–3 ms | ~5–30 ms (debug), <30 ms p95 (release) |
| Daemon incremental edit (same file) | ~1 ms | ~2–15 ms (incremental parse) |
| Nested Python class method | missed | extracted |
| C# `using` → `depends_on` | regex + heuristic resolve | same resolve path |

Configure in `.codeindex.toml`:

```toml
[extractor]
mode = "auto"          # regex | tree-sitter | auto
parse_timeout_ms = 50
max_tree_sitter_bytes = 512000
```

Run perf budgets (ignored by default):

```bash
cargo test --features tree-sitter --test extract_perf -- --ignored
```

Corpus fixtures: `tests/fixtures/extract/` (expected symbol names in `*.json`).

## Reproducing

```bash
cd worked/simple-rust
codebeacon init
time codebeacon report
time codebeacon query "auth"
time codebeacon path src/auth.rs src/db.rs
```

# Codebeacon Benchmarks

## Token efficiency (L0 index)

| Approach | Typical tokens (500-file repo) |
|----------|-------------------------------|
| grep + read loops | 10,000–50,000+ |
| Full repo dump | 100,000+ |
| Codebeacon `get_context` (L0) | ~500 |
| `drill_package` (one package) | ~1,000–3,000 |

Codebeacon's hierarchical index is designed so the L0 summary always fits in context. Packages below relevance 0.05 are omitted.

## Security benchmark (placeholder)

Formal CWE-190 verification adds latency per `verify_security` call:

| Mode | Typical latency |
|------|-----------------|
| Pattern-only | <1 ms |
| Z3 (`security-z3` feature) | 5–50 ms (configurable timeout) |

Run locally:

```bash
codebeacon verify --content 'malloc(x * sizeof(int))' --json
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

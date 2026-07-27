# Codebeacon Roadmap

## v0.2 (released)

- Security: `codebeacon verify`, MCP gate, Z3 CWE-190 (`security-z3` feature), hook examples
- CLI: `query`, `path`, `explain`, `dependents`, `report`, `export mermaid`, `install` / `uninstall`, `hook`
- MCP: `query_context`, `shortest_path`, `hotspots`, `get_report`, `get_index_summary`, `verify_security`
- Optional tree-sitter extraction for Rust, Go, Python, TypeScript/JS, C# (`tree-sitter` feature)
- Multi-platform install (Cursor, Claude, Codex, OpenCode, Hermes, agents, VS Code)

## v0.6 — Released

- [x] Multi-agent path locks + `run-plan` — [LOCKS.md](LOCKS.md)

## v0.5 — Released

- [x] Loop Context Coordinator — `loop_begin` / `loop_tick` / `loop_record` / `loop_end`, CLI `loop watch`, [LOOP.md](LOOP.md)

## v0.8 — Intelligence upgrades

- [x] BM25 + camelCase/stem tokenization (`.codeindex/search.bin`)
- [x] Call graph (`calls.bin`, MCP `call_graph`, enriched `change_impact`)
- [x] Diff-aware review (`review` CLI / `review_bundle`)
- [x] Architecture layer checks (`[architecture]`, `arch_check`)
- [x] NL navigation (`navigate_to_feature`)
- [x] Test gap analysis (`test_gaps`)
- [x] Logistic risk scoring (`predict_risk`, scored `fragile_files`)
- [x] Dependency freshness (`dep_freshness`)
- [x] Optional `embeddings` feature → `semantic_search` (char n-gram; BM25 fallback without feature)

## v0.3 — Planned

- [ ] `codebeacon serve --http` — team MCP endpoint
- [x] PR triage / change-impact summaries
- [ ] Per-edge INFERRED/EXTRACTED metadata in graph
- [ ] Leiden communities in report

## Future — Explicitly out of scope for now

| Feature | Notes |
|---------|-------|
| tree-sitter (20+ langs) | 5 langs shipped; Java next |
| `graph.html` force-directed UI | Use `export mermaid` + Mermaid Live |
| PDF / video / image indexing | Code-only focus |
| Neo4j / FalkorDB | petgraph is sufficient |
| 30 README translations | — |
| Heavy ML embeddings (candle/ORT) | Use `embeddings` n-gram feature or BM25 for now |

## Language expansion

Current: Rust, Go, Python, TypeScript/JS, C# (regex default; optional tree-sitter).

Next: Java (class/method patterns).

## Contributing

Open an issue or PR on [GitHub](https://github.com/lelorinel/codebeacon). See [INSTALL.md](INSTALL.md) and [TEAM.md](TEAM.md) for setup.

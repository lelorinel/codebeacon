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

## Language expansion

Current: Rust, Go, Python, TypeScript/JS, C# (regex default; optional tree-sitter).

Next: Java (class/method patterns).

## Contributing

Open an issue or PR on [GitHub](https://github.com/lelorinel/codebeacon). See [INSTALL.md](INSTALL.md) and [TEAM.md](TEAM.md) for setup.

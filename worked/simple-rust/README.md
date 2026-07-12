# Simple Rust — Codebeacon Walkthrough

A minimal auth + db crate to demo Codebeacon in **30 seconds**.

## 1. Index

```bash
codebeacon init --root .
```

Creates `.codeindex/` with packages, symbols, and a dependency graph.

## 2. Explore (CLI)

```bash
codebeacon query "auth"
codebeacon path src/auth.rs src/db.rs
codebeacon explain login
codebeacon dependents src/db.rs
codebeacon report -o CODEBEACON_REPORT.md
codebeacon export mermaid -o dep-graph.mmd
```

## 3. Explore (MCP)

Start the server: `codebeacon serve`

| Tool | What it does |
|------|--------------|
| `get_context` | L0 index — start here |
| `query_context` | Search like `codebeacon query` |
| `shortest_path` | `auth.rs → db.rs` chain |
| `hotspots` | `db.rs` is the god node |
| `get_dependents` | What breaks if you change `db.rs`? |

## Dependency graph

```
src/lib.rs ──► src/auth.rs ──► src/db.rs
           └──► src/db.rs
```

Open `dep-graph.mmd` in [Mermaid Live Editor](https://mermaid.live) for a visual.

## Same fixture

This example mirrors `tests/fixtures/simple_rust/` used in integration tests.

# Codebeacon Report

> Generated for **simple-rust** at 2026-07-12T21:32:49.952064241+00:00

## Summary

| Metric | Value |
|--------|-------|
| Packages | 1 |
| Files | 3 |
| Symbols | 6 |
| Graph nodes | 3 |
| Graph edges | 3 |

## Hotspots

Files with the most reverse dependencies (god nodes):

| Rank | File | Dependents |
|------|------|------------|
| 1 | `src/db.rs` | 2 |
| 2 | `src/auth.rs` | 1 |
| 3 | `src/lib.rs` | 0 |

## Packages

| Package | Files | Score | Purpose |
|---------|-------|-------|--------|
| src | 3 | 0.10 |  |

## Hot Symbols

- `User`
- `auth`
- `db`
- `find_user`
- `login`
- `logout`

## Suggested Questions

- What breaks if I change `src/db.rs`? (2 dependents)
- What breaks if I change `src/auth.rs`? (1 dependents)
- How does authentication flow through the dependency graph?
- Which packages have the highest relevance scores?

## Edge Provenance

| Type | Count | Source |
|------|-------|--------|
| EXTRACTED | 3 | import-resolved edges |
| INFERRED | — | LSP enrichment may add edges in background daemon |

## Security

Security policy not enabled. Install with `--security` or set `[security] enabled = true` in `.codeindex.toml`.

## LCP Differentiators

- **Live index** — daemon updates `.codeindex/` on save (not batch `graph.json`)
- **LSP precision** — `find_definition` / `find_references` MCP tools
- **Impact analysis** — `get_dependents` / `codebeacon dependents`
- **Multi-repo** — MCP `repo` argument for workspaces
- **Z3 security gate** — CWE-190 formal verification (`codebeacon verify`)

## Commands

```bash
codebeacon query "auth" --root /home/lelor/projects/LCP/worked/simple-rust
codebeacon path src/auth.rs src/db.rs --root /home/lelor/projects/LCP/worked/simple-rust
codebeacon explain login --root /home/lelor/projects/LCP/worked/simple-rust
codebeacon export mermaid --root /home/lelor/projects/LCP/worked/simple-rust
```

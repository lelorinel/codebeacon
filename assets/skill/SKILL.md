---
name: codebeacon
description: Navigate code with Codebeacon MCP — live index, LSP precision, dependency graph, and Z3 security. Use instead of grep/Read for exploration.
---

# Codebeacon

Codebeacon is a **live** hierarchical code index with LSP-backed precision — not a batch knowledge graph like Graphify.

## First move (always)

Call **`get_context`** before grep, Read, or Glob. It returns a relevance-sorted L0 index (~500 tokens) that always fits in context.

If no index exists, call **`init_workspace`** first.

## Drill down

| Tool | When |
|------|------|
| `drill_package` | Need files + symbols in a package |
| `find_definition` | Jump to where a symbol is defined (LSP when available) |
| `find_references` | Find all usages (LSP when available) |

## Impact analysis (before risky edits)

Call **`get_dependents`** on a file to see what breaks if you change it. CLI: `codebeacon dependents <file>`.

## Loop context (iterative agents)

For multi-step edit loops: **`loop_begin`** → edit → **`loop_record`** → **`loop_tick`** → repeat → **`loop_end`**.  
CLI: `codebeacon loop begin "goal" --file src/foo.rs`. See [references/loop.md](references/loop.md) or [LOOP.md](../../../docs/LOOP.md).

## Graph queries (Graphify parity)

| Tool | CLI equivalent | Purpose |
|------|----------------|---------|
| `query_context` | `codebeacon query "…"` | Search packages/symbols/files |
| `shortest_path` | `codebeacon path A B` | Dependency chain between files |
| `hotspots` | (in report) | God nodes — most depended-on files |

Pseudo-resources (via tools): `get_report`, `get_index_summary`, `get_hotspots`.

## Multi-repo workspaces

Pass the **`repo`** argument to scope any tool to one repo. Use `repo/package` notation in `drill_package`.

## Security (LCP-only)

When security is enabled (`codebeacon serve --security`):

- Call **`verify_security`** before suspicious allocation/size edits
- CLI: `codebeacon verify --content "…" --path file.rs`
- Cursor/Claude hooks can block edits automatically

See [references/security.md](references/security.md).

## LCP differentiators (Graphify cannot do)

1. **Live daemon** — `.codeindex/` updates on save (100ms debounce)
2. **LSP precision** — `find_definition` / `find_references`
3. **Z3 CWE-190 gate** — formal verification on edits
4. **Multi-repo** — one MCP server, many repos
5. **Token-efficient L0** — always-fits index summary

## Tool reference

See [references/mcp-tools.md](references/mcp-tools.md) for full MCP tool list.

---
name: codebeacon
description: Navigate code with Codebeacon MCP — live index, BM25 search, call graph, LSP, and Z3 security. Use instead of grep/Read for exploration.
---

# Codebeacon

Codebeacon is a **live** hierarchical code index with LSP-backed precision — not a batch knowledge graph like Graphify.

## First move (always)

Call **`get_context`** before grep, Read, or Glob. It returns a relevance-sorted L0 index (~350–500 tokens) that always fits in context.

If no index exists, call **`init_workspace`** first.

## Explore & navigate

| Tool | When |
|------|------|
| `drill_package` | Need files + symbols in a package |
| `query_context` | BM25 search (camelCase/stem — `auth` finds `authentication`) |
| `navigate_to_feature` | NL task → ranked files, read order, related docs |
| `semantic_search` | Same idea; embeddings build or BM25 fallback |
| `find_definition` | Jump to where a symbol is defined (LSP when available) |
| `find_references` | Find all usages (LSP when available) |
| `shortest_path` / `hotspots` | Dependency chain / most depended-on files |

## Before risky edits

Recommended: **`index_status`** → **`focus_context`** → **`change_impact`** (or **`call_graph`**) → edit.

| Tool | When |
|------|------|
| `index_status` | Is the index stale vs the working tree? |
| `focus_context` | Subgraph around the file you are editing |
| `change_impact` | Blast radius for a symbol (dependent files + callers + risk) |
| `call_graph` | Function-level callers / callees |
| `get_dependents` | What files import this file? |
| `review_bundle` | PR / commit / base…HEAD diff → affected symbols + impact |
| `predict_risk` / `fragile_files` | Churn × deps × bug-fix × test-gap risk |
| `test_gaps` | Prod functions without a matching test |
| `arch_check` | Layer/package boundary violations (`[architecture]` in config) |
| `dep_freshness` | Cargo / npm / go.mod drift |
| `package_conventions` / `api_surface` / `why_file` | Style fingerprint, exports, git+deps context |

CLI: `codebeacon query`, `navigate`, `impact`, `call-graph`, `review`, `arch-check`, `test-gaps`, `predict-risk`, `dep-freshness`.

## Loop context (iterative agents)

For multi-step edit loops: **`loop_begin`** → edit → **`loop_record`** → **`loop_tick`** → repeat → **`loop_end`**.  
CLI: `codebeacon loop begin "goal" --file src/foo.rs`. See [references/loop.md](references/loop.md) or [LOOP.md](../../../docs/LOOP.md).

## File locks (parallel agents) — optional

When multiple agents edit the same workspace, claim paths before shared edits:

1. **`claim_path`** with `path` + `block_key` (your agent/task id) + optional `intent`
2. If held: **`await_path`** then retry claim
3. After finishing that path: **`release_path`** with a short summary
4. End of multi-file task / run-plan block: **`session_done`** (`block_key`, `ok`, summary)

If lock tools are missing or MCP errors "not found": **skip locks** — do not explore MCP catalogs.  
CLI: `codebeacon run-plan ./plans "prompt"`. See [LOCKS.md](../../../docs/LOCKS.md).

## Docs sidecar (when enabled)

If `--docs` / `[docs] path` is set: **`query_docs`**, **`resolve_doc`**, **`docs_status`**, **`update_docs`**.

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
2. **BM25 + call graph** — smarter search and function-level impact
3. **LSP precision** — `find_definition` / `find_references`
4. **Z3 CWE-190 gate** — formal verification on edits
5. **Multi-repo** — one MCP server, many repos
6. **Token-efficient L0** — always-fits index summary

## Tool reference

See [references/mcp-tools.md](references/mcp-tools.md) for the full MCP tool list.

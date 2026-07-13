# Codebeacon MCP Tools

## Compact mode (default on)

Index and graph tools return token-compressed JSON with short keys and a path dictionary (`p1` → `src/auth.rs`). Pass `"compact": false` per call for legacy verbose output, or set `[compact] enabled = false` in `.codeindex.toml`.

Compact responses include `dict` (and optional `dict_delta` when new paths appear). Tool inputs accept either full paths or dict refs (`file: "p1"`).

CLI: `codebeacon query "auth" --compact` (omit flag to follow config; default compact on).

## Index tools

- **get_context** — L0 relevance-sorted index. Start here. Args: `repo?`, `compact?`
- **drill_package** — Package detail: files, symbols, imports. Args: `name`, `repo?`, `compact?`
- **init_workspace** — Build/rebuild `.codeindex/` on demand.
- **get_index_summary** — `index.json` L0 (resource `codebeacon://index`). Args: `repo?`, `compact?`

## LSP tools

- **find_definition** — Symbol definition (LSP + index fallback). Args: `symbol`, `file?`, `line?`, `character?`, `repo?`, `compact?`
- **find_references** — All usages (LSP + index fallback). Same args as find_definition.

## Graph tools (Graphify equivalents)

| Codebeacon | Graphify | Args |
|------------|----------|------|
| query_context | query_graph | `question`, `repo?`, `compact?` |
| shortest_path | path | `from`, `to`, `repo?`, `compact?` |
| hotspots | god_nodes | `limit?`, `repo?`, `compact?` |
| get_dependents | get_neighbors (reverse) | `file`, `repo?`, `compact?` |

`from`, `to`, and `file` accept dict refs (`p1`) or full paths.

## Resource equivalents

| URI | Tool fallback |
|-----|---------------|
| codebeacon://report | get_report |
| codebeacon://index | get_index_summary |
| codebeacon://hotspots | get_hotspots |

## Optional file-system tools

Enabled with `codebeacon serve --fs-tools`:

- read_file, write_file, edit_file, list_directory

## Security

- **verify_security** — CWE-190 Z3 gate (when `--security` enabled)

## Edit intelligence (default on)

Recommended workflow before editing: `index_status` → `focus_context` → `change_impact` → edit.

| Tool | When to use |
|------|-------------|
| `index_status` | Check index freshness vs working tree |
| `focus_context` | Subgraph around the file you are editing |
| `task_context` | Task keywords + package drill (`query_context` + proximity) |
| `change_impact` | Blast radius before renaming/changing a symbol |
| `package_conventions` | How this package writes code (error style, logging, async) |
| `similar_symbols` | Lightweight symbol similarity |
| `api_surface` | Public exports for a package |
| `why_file` | Recent git commits + dependency context |
| `fragile_files` | High-churn files with many dependents |

CLI: `codebeacon focus`, `status`, `impact`, `api`, `why`. Disable via `[intelligence] enabled = false` in `.codeindex.toml`.

# Codebeacon MCP Tools

## Index tools

- **get_context** — L0 relevance-sorted index. Start here.
- **drill_package** — Package detail: files, symbols, imports.
- **init_workspace** — Build/rebuild `.codeindex/` on demand.

## LSP tools

- **find_definition** — Symbol definition (LSP + index fallback).
- **find_references** — All usages (LSP + index fallback).

## Graph tools (Graphify equivalents)

| Codebeacon | Graphify | Args |
|------------|----------|------|
| query_context | query_graph | `question`, `repo?` |
| shortest_path | path | `from`, `to`, `repo?` |
| hotspots | god_nodes | `limit?`, `repo?` |
| get_dependents | get_neighbors (reverse) | `file`, `repo?` |

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

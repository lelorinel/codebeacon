# Codebeacon

> Don't let your AI assistant grep the repo — give it a relevance-sorted map that always fits in context.

**Get started in 3 steps**

1. `cargo install codebeacon && codebeacon init`
2. `codebeacon install --platform cursor --project`
3. Run `codebeacon serve` in MCP — in every task, call **`get_context`** first (not grep)

If no index exists yet, the AI can call `init_workspace` to build one on demand.

Compact MCP output is on by default (~30% fewer tokens). For local LLMs, set `[compact] enabled = false` in `.codeindex.toml` — see [BENCHMARKS.md](docs/BENCHMARKS.md).

Demo: [`worked/simple-rust/`](worked/simple-rust/) · Install: [INSTALL.md](docs/INSTALL.md) · Tools: [mcp-tools.md](assets/skill/references/mcp-tools.md) · [Changelog](CHANGELOG.md)

---

## What you get

- **Small map** — L0 index ~350–500 tokens; fits large repos without overflow
- **Smart ordering** — packages near your open files rank first (BFS on the import graph)
- **Graph queries** — `query`, `path`, `dependents` via CLI or MCP

**grep loop:** search → read file → search again → …  
**Codebeacon:** `get_context` → `drill_package` when needed. Token savings: [BENCHMARKS.md](docs/BENCHMARKS.md).

---

## How it works

![Architecture](docs/images/architecture.png)

File changes are parsed (regex by default; optional tree-sitter), imports are resolved into a dependency graph, and a hierarchical `.codeindex/` is written. The MCP server exposes that map on demand — load `index.json` first, drill into packages only when you need detail. LSP is optional and only used for `find_definition` / `find_references`.

---

## Quick reference

### Supported languages

Rust, Go, Python, TypeScript/JavaScript, C# — regex extraction needs no LSP binaries for indexing.

### Essential MCP tools

| Tool | When to use |
|------|-------------|
| `get_context` | Start of every task |
| `drill_package` | Full file and symbol list for one package |
| `find_definition` / `find_references` | Track a symbol |
| `query_context` | Keyword search across packages/files |
| `get_dependents` | "What breaks if I change this file?" |

Full tool list: [mcp-tools.md](assets/skill/references/mcp-tools.md)

### CLI

```bash
codebeacon init                              # build .codeindex/
codebeacon serve                             # MCP server (add --fs-tools or --security as needed)
codebeacon query "auth"                      # search
codebeacon path src/auth.rs src/db.rs        # shortest dependency chain
codebeacon report                            # CODEBEACON_REPORT.md
```

Install for your editor: `codebeacon install --list` — see [INSTALL.md](docs/INSTALL.md).

---

## Optional features

**Multi-repo** — `codebeacon serve --root workspace/` indexes every git repo under that folder. Tool output prefixes paths with the repo name; pass `repo` to scope a call.

**Security gate** — `codebeacon serve --security` or `[security] enabled = true` runs Z3 checks on edit fragments. Hooks + CI: `codebeacon verify`. Details: [SECURITY_EDIT_PATHS.md](docs/SECURITY_EDIT_PATHS.md).

**Local LLMs** — use `--fs-tools` and a system prompt that mandates `get_context`. See [INSTALL.md](docs/INSTALL.md#lm-studio-and-other-local-ai-environments).

**Configuration** — minimal example:

```toml
[compact]
enabled = true

[security]
enabled = false
```

Full schema: [CONFIG.md](docs/CONFIG.md).

---

## Index layout

```
.codeindex/
  index.json        ← Level 0 (~350–500 tokens)
  packages/         ← Level 1 detail (on demand)
  graph.bin         ← dependency graph (daemon)
  dict.json         ← path refs for compact mode
```

---

## Documentation

| Doc | Contents |
|-----|----------|
| [INSTALL.md](docs/INSTALL.md) | Platform setup, MCP, hooks, LM Studio |
| [CONFIG.md](docs/CONFIG.md) | `.codeindex.toml` reference |
| [BENCHMARKS.md](docs/BENCHMARKS.md) | Token savings, relevance scoring, compact mode |
| [SECURITY_EDIT_PATHS.md](docs/SECURITY_EDIT_PATHS.md) | Security coverage matrix |
| [TEAM.md](docs/TEAM.md) · [ROADMAP.md](docs/ROADMAP.md) | Team workflow and roadmap |

---

## License

Codebeacon is open source under the [GNU AGPL v3.0](LICENSE).

Commercial licensing (proprietary use without AGPL obligations): **[onur.fidan@outlook.com.tr](mailto:onur.fidan@outlook.com.tr)**

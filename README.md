# Codebeacon

> A hierarchical code index for AI coding assistants. Replaces grep + read loops with a relevance-sorted map that always fits in context.

---

## The Problem

AI coding assistants waste tokens. On every task, they run the same cycle:

```
grep for symbol → read file → grep again → read another file → ...
```

On a 500-file repo this is slow. On a 10,000-file repo it overflows the context window before the AI even starts writing code.

Existing approaches have real gaps:

| Problem | Codebeacon |
|---|---|
| Context window overflow on large repos | ✅ hierarchical index, L0 always fits |
| LSP timeout on fresh checkout | ✅ regex extraction, no LSP needed |
| node_modules / vendor / build dirs indexed | ✅ auto-skip + .gitignore respected |
| Changes missed while daemon is offline | ✅ catch-up index on restart |
| "What breaks if I change this file?" | ✅ BFS on dependency graph |
| Multiple repos in one workspace | ✅ multi-repo workspace support |
| Local models without native file tools | ✅ optional file-system tools via `--fs-tools` |

---

## How It Works

![Architecture](docs/images/architecture.png)

1. **FSWatcher** detects file changes with 100ms debounce
2. **Extractor** parses symbols from source code line-by-line with regex (no LSP needed)
3. **Indexer** resolves import statements to build an accurate dependency graph, then writes a hierarchical `.codeindex/`
4. **LSP Enricher** (background) uses LSP `definition` calls on import statements to discover additional dependency edges
5. **MCP Server** exposes tools for AI to query on demand

The AI loads `index.json` (~500 tokens) at session start. When it needs more, it drills down — no more grep loops.

### Relevance Scoring

Codebeacon resolves `import` / `use` / `require` statements to build an accurate dependency graph at index time. When you open files, it runs BFS from those files through the dependency graph:

| Hop distance | Score |
|---|---|
| 0 — your file | 1.0 |
| 1 hop away | 0.5 |
| 2 hops | 0.25 |
| 3+ hops | 0.1 |

`index.json` is always sorted by score. Packages below 0.05 are omitted. The map stays small regardless of repo size.

---

## Supported Languages

| Language | Extensions |
|---|---|
| Rust | `.rs` |
| Go | `.go` |
| Python | `.py` |
| TypeScript / JavaScript | `.ts`, `.tsx`, `.js`, `.jsx` |
| C# | `.cs` |

Symbol extraction is done via regex — no LSP binaries needed for indexing.  
LSP binaries (`rust-analyzer`, `gopls`, `pylsp`, `typescript-language-server`, `csharp-ls`) are only required for the `find_definition` and `find_references` MCP tools. If a binary is missing, those tools fall back to index-based search.

---

## Installation

### From crates.io

```bash
cargo install codebeacon
```

### From npm

```bash
npx codebeacon
```

### From source

```bash
git clone https://github.com/lelorinel/codebeacon
cd codebeacon
cargo build --release
# binary at target/release/codebeacon
```

---

## Usage

### Build the index

```bash
cd your-project
codebeacon init
# Index written to /your-project/.codeindex/
```

You can skip this step — if no index exists when the MCP server starts, the `init_workspace` tool lets the AI build it on demand.

### Start the daemon + MCP server

```bash
codebeacon serve                         # default: code context tools only
codebeacon serve --fs-tools              # also enable file read/write/edit tools
codebeacon serve --root /path/to/project # override workspace root
```

---

## Client Integration

### Claude Code

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "codebeacon": {
      "command": "codebeacon",
      "args": ["serve"]
    }
  }
}
```

Or via CLI:

```bash
claude mcp add codebeacon -- codebeacon serve
```

Claude Code automatically sets `CLAUDE_PROJECT_DIR` when launching the server, so Codebeacon finds your project without any `--root` argument.

### Cursor

```json
{
  "mcpServers": {
    "codebeacon": {
      "command": "codebeacon",
      "args": ["serve"]
    }
  }
}
```

Cursor sets `CURSOR_WORKSPACE` automatically. No `--root` needed.

### VS Code, Zed, Cline

These clients launch MCP servers with `cwd` set to the workspace folder, so Codebeacon auto-detects the project root with no extra configuration.

```json
{
  "mcpServers": {
    "codebeacon": {
      "command": "codebeacon",
      "args": ["serve"]
    }
  }
}
```

### LM Studio and other local AI environments

Use `--fs-tools` to enable file read/write/edit tools for models that lack native file access.
`--root` is also required since LM Studio does not set workspace environment variables automatically:

```json
{
  "mcpServers": {
    "codebeacon": {
      "command": "codebeacon",
      "args": ["serve", "--fs-tools", "--root", "/path/to/your/project"]
    }
  }
}
```

#### Getting local models to use the tools

Unlike Claude Code, local models are not trained to call MCP tools automatically. Without guidance they will answer from their training data and ignore the tools entirely.

**Option 1 — System prompt (recommended).** In LM Studio go to **Settings → Model → System Prompt** and add:

```
You have access to codebeacon MCP tools for exploring this codebase.
ALWAYS use them instead of guessing from memory:

- get_context       → call this first to understand the project structure
- drill_package     → full file and symbol list for a package
- find_definition   → locate where a symbol is defined
- find_references   → find all usages of a symbol
- get_dependents    → what breaks if this file changes
- read_file         → read a source file

Never answer code questions without calling at least get_context first.
```

**Option 2 — Mention the tool in your query.** If you don't want to change the system prompt, be explicit in every message:

```
Use get_context to find the Rust microservice in this project and explain what it does.
```

### Manual root override

If auto-detection doesn't work in your environment:

```bash
codebeacon serve --root /path/to/your/project
```

---

## Multi-Repo Workspaces

Codebeacon can serve multiple git repos from a single server instance. Point `--root` at a directory that contains several repos:

```
workspace/
  api/       ← git repo
  frontend/  ← git repo
  infra/     ← git repo
```

```bash
codebeacon serve --root workspace/
# codebeacon workspace: 3 repo(s) selected
#   repo: /workspace/api
#   repo: /workspace/frontend
#   repo: /workspace/infra
```

Each repo keeps its own `.codeindex/` and is indexed independently. In multi-repo mode, tool responses prefix paths with the repo name (`api/src/main.rs`) and accept an optional `repo` argument to scope queries to a single repo.

**Single-repo output is unchanged** — the multi-repo envelope only appears when more than one repo is active.

---

## MCP Tools

### Code context tools (always available)

All tools accept an optional `repo` argument to scope the operation to a single repo in multi-repo workspaces.

| Tool | Description |
|---|---|
| `get_context(repo?)` | Relevance-sorted index for the workspace. Returns all repos in multi-repo mode; use `repo` to filter. |
| `drill_package(name, repo?)` | Full symbol list for a package. Use `repo/package` notation in multi-repo workspaces. |
| `find_references(symbol, repo?)` | All locations where a symbol is used, across all repos. |
| `find_definition(symbol, repo?)` | Definition location and signature. |
| `get_dependents(file, repo?)` | Files that depend on this file — "what breaks if I change this?" |
| `init_workspace(repo?)` | Build (or rebuild) the code index. Called automatically when no index exists yet. Use `repo` to index a single repo in a multi-repo workspace. |

### File-system tools (`--fs-tools` flag required)

These tools are **disabled by default**. Enable them with `codebeacon serve --fs-tools` for environments where the AI model has no native file access (e.g. LM Studio with a local model).

All file-system tools accept an optional `repo` argument in multi-repo workspaces. When multiple repos exist, write/edit operations require `repo` to be specified.

| Tool | Description |
|---|---|
| `read_file(path, repo?)` | Read the contents of a file. |
| `write_file(path, content, repo?)` | Create or overwrite a file. Creates parent directories as needed. |
| `edit_file(path, old_string, new_string, repo?)` | Replace the first occurrence of `old_string` in a file. Fails if not found. |
| `list_directory(path?, repo?)` | List files and subdirectories at a path (defaults to repo root). |

All file-system operations are sandboxed to the configured repo roots — path traversal attempts are rejected.

---

## On-demand indexing

If you start `codebeacon serve` without running `codebeacon init` first, the AI will see a prompt the first time it calls `get_context`:

```
No index found for repo 'myproject'.
Call `init_workspace` to build the index (may take a moment for large repos).
```

The AI will ask you for confirmation, then call `init_workspace` to build the index automatically. No CLI step required.

In multi-repo workspaces, pass `repo` to `init_workspace` to index a single repo, or omit it to index all repos at once.

---

## Workspace root auto-detection

Codebeacon resolves the project root in this order:

| Priority | Source |
|---|---|
| 1 | `--root` CLI flag |
| 2 | MCP `roots/list` request to the client (standard MCP protocol) |
| 3 | `CLAUDE_PROJECT_DIR` env var (Claude Code) |
| 4 | `CURSOR_WORKSPACE` env var (Cursor) |
| 5 | Process `cwd` (VS Code, Zed, Cline — they set this to the workspace folder) |

For a directory containing multiple git repos, Codebeacon serves all of them as a workspace. For a single git repo (or a path inside one), it serves that repo alone.

---

## Index Structure

```
.codeindex/
  index.json        ← Level 0: always loaded (~500 tokens)
  packages/
    auth.json       ← Level 1: per-package detail (on demand)
    db.json
  graph.bin         ← Binary dependency graph (daemon only)
```

`graph.bin` is written on every update. On restart, Codebeacon re-indexes only files changed since the last write — no full re-index needed.

---

## Configuration File

Place a `.codeindex.toml` at the repo root to customise indexing behaviour:

```toml
# Additional directories to skip during indexing
extra_ignore_dirs = ["my_build", "tmp"]

# Glob patterns for files to ignore
ignore_globs = ["**/*.generated.cs"]

# If set, only these languages are indexed (case-insensitive)
languages = ["rust", "go"]

# LSP worker pool size per language (default: 2)
lsp_concurrency = 4

# Seconds to spend enriching the heuristic index with LSP definition calls
# to discover additional dependency edges (default: 60, set 0 to disable)
lsp_enrich_timeout_secs = 30

[lsp]
# Override LSP binary per language (e.g. use OmniSharp instead of csharp-ls)
overrides = { csharp = "OmniSharp" }
```

All fields are optional. Without this file, Codebeacon uses sensible defaults.

---

## Token Comparison

Tested on a 445-file TypeScript + Rust monorepo:

| Approach | Tool calls | Files read | Tokens (est.) |
|---|---|---|---|
| Claude without Codebeacon | 5+ | 3–10 | ~5,000–8,000 |
| Claude with Codebeacon | 2 | 0 | ~800–1,200 |

---

## License

Codebeacon is open source under the [GNU AGPL v3.0](LICENSE).

If you want to use Codebeacon in a proprietary product without open-sourcing your modifications, a commercial license is available. Contact: **[onur.fidan@outlook.com.tr](mailto:onur.fidan@outlook.com.tr)**

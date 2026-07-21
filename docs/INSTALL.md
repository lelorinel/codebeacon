# Installing Codebeacon

## Quick install

```bash
cargo install codebeacon
cd your-project
codebeacon install --platform cursor --project   # prompts to init if needed; or pass --yes
codebeacon serve    # or let install write MCP config for you
```

In your AI agent: call **`get_context`** first (not grep).

## Install methods

### From crates.io

```bash
cargo install codebeacon
```

### From npm

```bash
npx codebeacon
# or: npm install -g codebeacon
```

On first interactive run of `npx codebeacon` (bare / `help` / `init`), the npm
wrapper offers to:

1. add a shell **alias** (`codebeacon` → `npx codebeacon`),
2. put the native binary directory on your **PATH**, or
3. keep using **`npx codebeacon`** with no shell changes.

Dismissal is stored in `~/.config/codebeacon/onboarding.json` and keyed by
**major.minor** (e.g. `0.5`). Bumping to `0.6.x` asks again; patch releases do
not. Skip with `CODEBEACON_SKIP_ONBOARD=1`. Never prompts for `serve` / MCP or
non-TTY runs.

### From source

```bash
git clone https://github.com/lelorinel/codebeacon
cd codebeacon
cargo build --release
# binary at target/release/codebeacon

# Optional: tree-sitter extraction (Rust, Go, Python, TypeScript, C#)
cargo build --release --features tree-sitter
```

## Platforms

| Platform | Command | What gets installed |
|----------|---------|---------------------|
| Cursor | `codebeacon install --platform cursor --project` | `.cursor/rules/codebeacon.mdc`, `.cursor/mcp.json` |
| Claude | `codebeacon install --platform claude` | `~/.claude/skills/codebeacon/`, discovery hook |
| Codex | `codebeacon install --platform codex --project` | `AGENTS.md`, `~/.codex/config.toml` (`[mcp_servers.codebeacon]`), `.codex/hooks.json` + project `.codex/config.toml` |
| OpenCode | `codebeacon install --platform opencode` | `~/.config/opencode/skills/codebeacon/` |
| Hermes | `codebeacon install --platform hermes` | `~/.hermes/skills/codebeacon/` |
| Agents | `codebeacon install --platform agents` | `~/.agents/skills/codebeacon/` |
| VS Code | `codebeacon install --platform vscode --project` | `.vscode/mcp.json` |

List all: `codebeacon install --list`

### Flags

- `--project` — install into current repo (rules, MCP config)
- `--security` — add `--security` to MCP `serve` args
- `--fs-tools` — enable file-system MCP tools
- `--yes` / `-y` — if `.codeindex/index.json` is missing, run `init` without prompting

After a successful install, if there is no index yet and you are on an interactive
terminal, Codebeacon asks `run init now? [Y/n]` (default yes). Non-interactive
sessions skip init unless `--yes` is set.

## MCP configuration

Standard MCP server entry (works for most clients):

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

With security: `"args": ["serve", "--security"]`

With file tools (local LLMs): `"args": ["serve", "--fs-tools", "--root", "/path/to/project"]`

## Client integration

### Claude Code

Add to your project's `.mcp.json` (see above), or:

```bash
claude mcp add codebeacon -- codebeacon serve
```

Claude Code sets `CLAUDE_PROJECT_DIR` when launching the server — no `--root` needed.

### Cursor

`codebeacon install --platform cursor --project` writes `.cursor/mcp.json`. Cursor sets `CURSOR_WORKSPACE` automatically.

### Codex

`codebeacon install --platform codex` writes OpenAI Codex MCP into **`~/.codex/config.toml`**:

```toml
[mcp_servers.codebeacon]
command = "/absolute/path/to/codebeacon"
args = ["serve"]
```

With `--project`, also writes `.codex/config.toml` in the repo (Codex only loads project MCP for **trusted** projects). Restart Codex after install. The `command` must be an absolute path (Codex often has a minimal `PATH`).

### VS Code, Zed, Cline

These clients launch MCP servers with `cwd` set to the workspace folder, so Codebeacon auto-detects the project root with no extra configuration.

### LM Studio and other local AI environments

Local models often lack native file access and are not trained to call MCP tools automatically. Use `--fs-tools` and an explicit `--root`:

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

**Option 2 — Mention the tool in your query:**

```
Use get_context to find the Rust microservice in this project and explain what it does.
```

For compact MCP output (default), local models may prefer verbose JSON — set `[compact] enabled = false` in `.codeindex.toml`. See [CONFIG.md](CONFIG.md) and [BENCHMARKS.md](BENCHMARKS.md).

### Manual root override

If auto-detection doesn't work:

```bash
codebeacon serve --root /path/to/your/project
```

Workspace root resolution order is documented in [CONFIG.md](CONFIG.md#workspace-root-resolution).

## Uninstall

```bash
codebeacon uninstall --platform cursor --project
codebeacon uninstall --purge   # remove skill directories
```

Idempotent markers (`<!-- codebeacon-start -->`) protect user edits outside Codebeacon sections.

## Hooks

Two hook types (do not confuse them):

1. **Discovery** (`assets/hooks/codebeacon-context.sh`) — nudges agent to use `get_context` when `.codeindex/` exists
2. **Security** (`assets/hooks/codebeacon-security.sh`) — blocks/warns on CWE patterns

See `assets/hooks/cursor-hooks.json.example`. After `codebeacon install --platform cursor --project`, copy `.cursor/hooks.json.example` to `.cursor/hooks.json`.

### Cursor security hook

Merge from [`assets/hooks/cursor-hooks.json.example`](../assets/hooks/cursor-hooks.json.example). Script: [`assets/hooks/codebeacon-security.sh`](../assets/hooks/codebeacon-security.sh).

### Claude Code security hook

Merge [`assets/hooks/claude-settings.security.example.json`](../assets/hooks/claude-settings.security.example.json) into your Claude settings. Copy [`assets/hooks/codebeacon-security.sh`](../assets/hooks/codebeacon-security.sh) to `.claude/hooks/` (or `~/.claude/hooks/`).

### OpenCode (force MCP path)

See [opencode-security.example.jsonc](opencode-security.example.jsonc) — deny native `edit` and allow Codebeacon MCP file tools with `--fs-tools --security`.

## Git post-commit hook

```bash
codebeacon hook install
```

Re-runs `codebeacon init` after each commit to keep the index fresh.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `get_context` says no index | `codebeacon init` or MCP `init_workspace` |
| `find_definition` empty | Install LSP binary (`rust-analyzer`, etc.) |
| MCP not connecting | Check `codebeacon` is on PATH; test with `codebeacon serve` |
| Large repo slow first index | Normal; daemon keeps it incremental after |

## Live index vs Graphify

Codebeacon uses a **live** `.codeindex/` updated by the daemon on save — not a batch `graph.json` rebuild. See [BENCHMARKS.md](BENCHMARKS.md) and [ROADMAP.md](ROADMAP.md).

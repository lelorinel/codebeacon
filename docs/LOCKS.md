# Path locks + run-plan

Coordinate multiple agents editing the same workspace so they do not overwrite each other.

## How it works

Codebeacon MCP exposes lock tools (default on). Claims are stored in a **file-backed JSON** store:

```text
.codeindex/locks/apply-locks.json
```

Processes share the store via `flock`, so IDE Task/subagents and `run-plan` spawned agents can coordinate even when each has its own MCP process.

### Tools

| Tool | Purpose |
|------|---------|
| `claim_path` | Exclusive edit lease (`path`, `block_key`, optional `intent`). Same `block_key` renews TTL. |
| `release_path` | Release + publish DONE summary for awaiters |
| `await_path` | Poll until free or DONE (or timeout) |
| `list_locks` / `list_done` | Inspect |
| `session_done` | Finish a session; drop remaining claims for `block_key` |
| `list_sessions` | Session status |

### Agent flow (optional)

```text
claim_path → edit → release_path
If held: await_path → retry claim
When whole task done: session_done(block_key, ok, summary)
```

**If lock tools are missing / MCP "not found": skip locks** — do not explore MCP catalogs.

`block_key`:

- IDE multi-agent: your agent/task id
- `run-plan`: plan file stem (`auth.md` → `auth`)

## Config (`.codeindex.toml`)

```toml
[locks]
enabled = true
ttl_secs = 600
# allow = ["src", "generated"]   # empty = any relative path
```

CLI: `codebeacon serve --no-locks` hides the tools.

## `codebeacon run-plan`

Run every `*.md` in a plans directory with parallel Cursor/Claude/Codex agents.

**Default UI is a multi-agent TUI** (sidebar + focused PTY). Use `--headless` for CI / inherit-stdout waves.

```bash
codebeacon run-plan ./plans "Implement these plans against the current codebase"
codebeacon run-plan ./plans "…" --parallel 2 --model composer-2.5
codebeacon run-plan ./plans "…" --provider claude
codebeacon run-plan ./plans "…" --provider codex --model o4-mini
codebeacon run-plan ./plans "…" --dry-run
codebeacon run-plan ./plans "…" --headless   # no TUI
```

### TUI keys

| Mode | Keys |
|------|------|
| **Nav** | `j`/`k` or ↑↓ select · Enter attach · `x` close pane · `p` re-prompt · `Q` quit (twice) |
| **Attach** | All keys → agent (Esc/Ctrl+C/Ctrl+J/… pass through) · **`Ctrl+]`** detach only |

Sidebar marks: spinner = working · **`?`** = waiting for your input (permission / question) · `✓` = done · `♪` = conductor.

Done agents stay in the sidebar with ✓ until you press `x`. Closing a pane frees a slot for the next queued plan.

### `codebeacon multi-agent`

Interactive session — on open, pick **Gallery** or **Conductor** (or pass `--mode`):

```bash
codebeacon multi-agent
codebeacon multi-agent --mode gallery
codebeacon multi-agent --mode conductor --provider claude
```

| Mode | Behavior |
|------|----------|
| **Gallery** | Equal panes; `n` creates agents; attach to any pane |
| **Conductor** | First agent is the **conductor** (`♪`); input only to conductor; ensemble panes are view-only; conductor spawns workers via MCP `spawn_agent` |

Conductor session files live under `.codeindex/multi-agent/<session_id>/` (`meta.json`, `queue.json`, `agents.json`).

| Provider | Binary / env | Notes |
|----------|--------------|-------|
| `cursor` (default) | `agent` / `CURSOR_AGENT` | `--force --approve-mcps` |
| `claude` | `claude` / `CLAUDE_BIN` | TUI: interactive; headless: `--print` + bypassPermissions; injects run-scoped `--mcp-config` (with `CODEBEACON_MA_*` env in conductor mode) |
| `codex` | `codex` / `CODEX_BIN` | TUI: interactive; headless: `codex exec --full-auto` |

Install MCP/skills first for the target platform: `codebeacon install --platform cursor|claude|codex --project`.

#### Conductor MCP tools

When an active conductor session is present (ACTIVE file / `CODEBEACON_MA_SESSION`):

| Tool | Who | Action |
|------|-----|--------|
| `spawn_agent` | conductor | Enqueue ensemble prompt → TUI opens pane |
| `list_agents` | all | List conductor + ensemble |
| `agent_status` | all | One agent by `block_key` |

Ensemble members with `CODEBEACON_MA_ROLE=ensemble` cannot call `spawn_agent`.

Flow for `run-plan`:

1. Discover flat `*.md` under the plans dir
2. Reset lock store; register a session per plan stem
3. Write briefs under `.codeindex/run-plan/<run_id>/`
4. Open TUI (or headless waves with `--parallel`)
5. Barrier on `session_done` or signal file `touch` + `CBDONE <block_key>`

Agent binary: `CURSOR_AGENT` env or `agent` on PATH (Cursor). Claude uses `claude` on PATH.

## Skill

Installed skill ([SKILL.md](../assets/skill/SKILL.md)) documents the optional lock flow. Cursor rule nudges parallel edits to claim first.

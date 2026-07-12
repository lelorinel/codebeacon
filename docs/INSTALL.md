# Installing Codebeacon

## Quick install

```bash
cargo install codebeacon
codebeacon install --platform cursor --project
codebeacon init
```

## Platforms

| Platform | Command | What gets installed |
|----------|---------|---------------------|
| Cursor | `codebeacon install --platform cursor --project` | `.cursor/rules/codebeacon.mdc`, `.cursor/mcp.json` |
| Claude | `codebeacon install --platform claude` | `~/.claude/skills/codebeacon/`, discovery hook |
| Codex | `codebeacon install --platform codex --project` | `AGENTS.md`, `.codex/hooks.json` |
| OpenCode | `codebeacon install --platform opencode` | `~/.config/opencode/skills/codebeacon/` |
| Hermes | `codebeacon install --platform hermes` | `~/.hermes/skills/codebeacon/` |
| Agents | `codebeacon install --platform agents` | `~/.agents/skills/codebeacon/` |
| VS Code | `codebeacon install --platform vscode --project` | `.vscode/mcp.json` |

List all: `codebeacon install --list`

### Flags

- `--project` — install into current repo (rules, MCP config)
- `--security` — add `--security` to MCP `serve` args
- `--fs-tools` — enable file-system MCP tools

## MCP configuration

```json
{
  "codebeacon": {
    "command": "codebeacon",
    "args": ["serve"]
  }
}
```

With security: `"args": ["serve", "--security"]`

## Uninstall

```bash
codebeacon uninstall --platform cursor --project
codebeacon uninstall --purge   # remove skill directories
```

Idempotent markers (`<!-- codebeacon-start -->`) protect user edits outside Codebeacon sections.

## Hooks

Two hook types (do not confuse them):

1. **Discovery** (`assets/hooks/codebeacon-context.sh`) — nudges agent to use `get_context` when `.codeindex/` exists
2. **Security** (`assets/hooks/codebeacon-security.sh`) — blocks/warns on CWE-190 patterns

See `assets/hooks/cursor-hooks.json.example`. After `codebeacon install --platform cursor --project`, copy `.cursor/hooks.json.example` to `.cursor/hooks.json`.

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

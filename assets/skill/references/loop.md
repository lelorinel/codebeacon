# Loop MCP tools

See [LOOP.md](../../../docs/LOOP.md) for full documentation.

## Tools

- **loop_begin** — `goal`, `file?`, `files?`, `tick?` (default true), `repo?`, `compact?`
- **loop_tick** — `session_id`, `file?`, `repo?`, `compact?`
- **loop_record** — `session_id`, `files[]`, `symbol?`, `repo?`
- **loop_end** — `session_id`, `repo?`

## Workflow

```
loop_begin → [agent edits] → loop_record → loop_tick → … → loop_end
```

Disable via `[loop] enabled = false` in `.codeindex.toml`.

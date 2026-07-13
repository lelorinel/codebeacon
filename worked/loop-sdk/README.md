# Codebeacon loop + Cursor SDK example

This example shows how an external script can drive **loop context** without Codebeacon running the LLM.

## Prerequisites

- `codebeacon serve` running (stdio MCP)
- Node.js 18+
- Cursor SDK (`@cursor/sdk`) if you wire a real agent

## Flow

1. `loop_begin` with a goal and active file
2. Each iteration: `loop_tick` → pass bundle to your agent prompt
3. After edits: `loop_record` with touched files
4. On `signals.should_stop`: `loop_end`

## Pseudocode

```typescript
// Pseudocode — adapt to your MCP client transport
const begin = await mcp.call("loop_begin", {
  goal: "fix login validation bug",
  file: "src/auth.rs",
  compact: true,
});
const sessionId = begin.session_id;

while (true) {
  const tick = await mcp.call("loop_tick", {
    session_id: sessionId,
    compact: true,
  });

  if (tick.sig?.ss) break; // should_stop

  // Inject tick JSON into agent system context, then run one agent turn
  await runAgentTurn(tick);

  await mcp.call("loop_record", {
    session_id: sessionId,
    files: ["src/auth.rs"],
  });

  if (tick.sig?.sp) {
    // should_pause — stale index; tick may have reindexed; continue or wait
  }
}

await mcp.call("loop_end", { session_id: sessionId });
```

## CLI alternative (no SDK)

```bash
SESSION=$(codebeacon loop begin "fix login" --file src/auth.rs --no-tick | jq -r .session_id)
codebeacon loop tick --session "$SESSION"
# ... agent work ...
codebeacon loop watch --session "$SESSION" --interval 5m
```

See [docs/LOOP.md](../../docs/LOOP.md) for full documentation.

# Loop Context Coordinator

Codebeacon does **not** run your agent loop. It supplies a fresh, relevance-sorted context packet on each iteration so iterative agents edit with the right map.

## Workflow

```
loop_begin → [agent edits] → loop_record → loop_tick → repeat → loop_end
```

Recommended per iteration:

1. **`loop_tick`** — index freshness, focus subgraph, reindex if policy says so, signals (`should_pause`, `should_stop`)
2. Agent edits code
3. **`loop_record`** — files touched; optional `symbol` for `change_impact`
4. Repeat until `should_stop` or task done
5. **`loop_end`** — summary (iterations, touched files)

## Integration paths

### MCP (default)

Call from any MCP client while `codebeacon serve` is running:

- `loop_begin` — `goal`, optional `file` / `files`, `tick` (default true)
- `loop_tick` — `session_id`, optional `file`
- `loop_record` — `session_id`, `files[]`, optional `symbol`
- `loop_end` — `session_id`

All support `compact` where applicable (same as edit-intelligence tools).

### CLI

```bash
codebeacon loop begin "fix login bug" --file src/auth.rs
codebeacon loop tick --session <id>
codebeacon loop record --session <id> --files src/auth.rs
codebeacon loop end --session <id>

# Cursor /loop sentinel integration
codebeacon loop watch --session <id> --interval 5m

# One-shot: begin + first tick + watch
codebeacon loop run "fix login" --file src/auth.rs --interval 5m
```

`watch` prints `AGENT_LOOP_TICK_codebeacon {"session_id":"...","prompt":"..."}` on each interval — compatible with Cursor's `/loop` skill.

### Cursor SDK

See [`worked/loop-sdk/README.md`](../worked/loop-sdk/README.md) for a TypeScript example that drives `loop_*` tools via MCP.

## Configuration

```toml
[loop]
enabled = true
reindex = "if_stale"   # never | if_stale | every_n | always
reindex_every_n = 3
stale_warn_threshold = 5
max_iterations = 50
```

- **`if_stale`** (default) — runs `catchup_index` when git reports modified files or mtime stale vs `graph.bin`
- **`every_n`** — periodic catch-up regardless of staleness
- **`always`** — catch-up every tick (expensive; use sparingly)

Full field reference: [CONFIG.md](CONFIG.md#loop).

## Artifacts

When `persist_sessions = true`:

```
.codeindex/loop/<session_id>/
  session.json
  iteration-001.json
  iteration-002.json
```

Useful for debugging why a loop iteration had stale context or skipped re-index.

## Signals (`loop_tick`)

| Field | Meaning |
|-------|---------|
| `stale_count` | Files changed vs last index |
| `reindex_recommended` | Policy suggests re-index |
| `reindexed` | `catchup_index` ran this tick |
| `should_pause` | Stale count ≥ threshold and not reindexed |
| `should_stop` | `max_iterations` reached |
| `hints` | Human/agent-readable next steps |

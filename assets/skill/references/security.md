# Codebeacon Security

## Enable

```bash
codebeacon install --platform cursor --project --security
codebeacon serve --security
```

Or in `.codeindex.toml`:

```toml
[security]
enabled = true
mode = "balanced"  # strict | balanced | permissive
```

## MCP

Call `verify_security` with `content` (code fragment) and optional `path` before write/edit operations on allocation-sensitive code.

## CLI (hooks + CI)

```bash
codebeacon verify --content "$NEW_STRING" --path src/foo.rs
# exit 0 = allow/warn, exit 1 = block
```

## Hooks

- **Security hook** (blocking): `.cursor/hooks/codebeacon-security.sh` — PreToolUse on Write
- **Discovery hook** (nudge only): `codebeacon-context.sh` — reminds to use get_context

These are separate hooks. Install both for best results.

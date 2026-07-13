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

# Z3 CWEs enabled by default: 190, 131, 191, 369, 680
# Pattern-only CWEs (opt-in): 78, 134, 502, 798, 22
enabled_cwes = ["190", "131", "191", "369", "680", "134"]
```

## Supported CWE checks

| CWE | Type | Default | Description |
|-----|------|---------|-------------|
| CWE-190 | Z3 | on | Integer overflow in allocation (`n * size`, shift) |
| CWE-131 | Z3 | on | Two-variable buffer size (`malloc(a * b)`) |
| CWE-191 | Z3 | on | Integer underflow (`malloc(n - k)`) |
| CWE-369 | Z3 | on | Divide by zero (`malloc(total / count)`) |
| CWE-680 | Z3 | on | Buffer copy overflow (`memcpy(..., n * size)`) |
| CWE-134 | pattern | off | Format string (`printf(var)`) |
| CWE-78 | pattern | off | Command injection (`system(var)`) |
| CWE-798 | pattern | off | Hardcoded credentials |
| CWE-502 | pattern | off | Unsafe deserialization |
| CWE-22 | pattern | off | Path traversal |

Verification runs on **edit fragments only** (same path as hooks and MCP gate) — not full-repo scans.

## MCP

Call `verify_security` with `content` (code fragment) and optional `path` before write/edit operations on security-sensitive code.

## CLI (hooks + CI)

```bash
codebeacon verify --content "$NEW_STRING" --path src/foo.rs
# exit 0 = allow/warn, exit 1 = block
```

## Hooks

- **Security hook** (blocking): `.cursor/hooks/codebeacon-security.sh` — PreToolUse on Write
- **Discovery hook** (nudge only): `codebeacon-context.sh` — reminds to use get_context

These are separate hooks. Install both for best results.

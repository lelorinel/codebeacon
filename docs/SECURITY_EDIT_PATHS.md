# Security Gate — Cross-Client Edit Path Coverage

> **Purpose:** Evaluate where Codebeacon's security gate actually runs across AI coding agents.
> **Status:** Architecture analysis (2026-07-13). Not end-to-end tested on every client.
> **Related:** `src/security/` (Z3 engine + policy), `src/mcp/tools.rs` (current gate)

---

## Core problem

Codebeacon gates **only its own MCP tools** today:

```
apply_security_gate()  →  handle_write_file / handle_edit_file  →  requires --fs-tools
```

Every agent also has **native file edit tools** that write directly to disk. An MCP server **cannot intercept** those unless the host provides a hook, permission layer, or you disable native tools.

**Product implication:** "Security MCP" ≠ "secured workspace" unless you integrate per host.

---

## Coverage matrix

| Client | Native edit tools | Default uses native or MCP? | Gate today (`--security` only) | Gate with `--fs-tools` | Hook / config path | Realistic coverage |
|--------|-------------------|----------------------------|-------------------------------|------------------------|--------------------|--------------------|
| **Cursor** | `Write`, `StrReplace`, Apply | **Native** (almost always) | ❌ | ⚠️ Only if agent picks MCP over native | `preToolUse` hook → `codebeacon verify` | **Hook required** |
| **Claude Code** | `Read`, `Edit`, `Write` | **Native** (recommended by Anthropic) | ❌ | ⚠️ Same | `PreToolUse` hook + `permissions.deny` | **Hook required** |
| **OpenCode** | `write`, `edit`, `apply_patch` | **Native** by default | ❌ | ⚠️ Same | `permission.edit: deny` + force MCP | **Config + MCP** (best non-hook story) |
| **Hermes Agent** | `read_file`, terminal, etc. | **Native** + MCP side by side | ❌ | ⚠️ If agent chooses `mcp_codebeacon_edit_file` | No first-class pre-edit hook documented | **Weak** — rules + MCP naming |
| **LM Studio** | None (no file tools) | **MCP** if prompted | ❌ (no write tools) | ✅ **Gate works** if agent uses MCP | N/A | **Good** (target use case for `--fs-tools`) |
| **VS Code + Cline** | Extension file tools | **Native** | ❌ | ⚠️ | Extension-dependent; no standard pre-edit hook | **Hook / CI** |
| **Zed** | Agent file tools | **Native** | ❌ | ⚠️ | ACP / extension hooks (immature) | **Weak** |
| **Codex (OpenAI)** | Sandboxed write | **Native** | ❌ | ⚠️ | Platform-controlled; third-party gate hard | **CI / post-merge** |
| **Windsurf / others** | Similar to Cursor | **Native** | ❌ | ⚠️ | IDE-specific hooks if available | **Per-IDE** |

Legend:
- ✅ = preemptive block works reliably today
- ⚠️ = works only if agent voluntarily uses Codebeacon MCP file tools (unreliable)
- ❌ = no coverage

---

## Per-client detail

### Cursor

**Edit path:** Agent → internal `Write` / search-replace → filesystem. Codebeacon MCP runs in parallel; not in the write path.

**Default MCP config** (`README`): `codebeacon serve` — no `--fs-tools`, no `--security`.

**How to get real coverage:**
1. **Project hook** `.cursor/hooks.json`:
   - Event: `preToolUse`
   - Matcher: `Write` (and any other edit tool names Cursor uses)
   - Script: pipe tool input → `codebeacon verify --json` → `permission: deny` on Block
2. Optional: Cursor rule telling agent to call `verify_security` (unreliable alone)

**Cannot do:** Block from MCP server when Cursor uses native Write.

---

### Claude Code

**Edit path:** Built-in `Edit` / `Write` — intentionally preferred over MCP file servers (~0 definition overhead vs ~5k tokens for filesystem MCP).

**Env:** Sets `CLAUDE_PROJECT_DIR` — workspace detection works.

**How to get real coverage:**
1. **`PreToolUse` hook** in `~/.claude/settings.json` or project hooks:
   - Matcher: `Edit|Write`
   - Call `codebeacon verify` on `new_string` / file content
   - Return `hookSpecificOutput.permissionDecision: deny` (exit **0**, not 2)
2. **`permissions.deny`** in settings for path patterns (blocks edit, no Z3 — coarse)
3. Do **not** rely on MCP `write_file` — model is trained to use built-ins

**Caveat:** PreToolUse deny behavior has had version-specific bugs; test on your Claude Code version. `permissions.deny` is more reliable but not semantic (no Z3).

---

### OpenCode

**Edit path:** Built-in `write`, `edit`, `apply_patch` + MCP tools alongside.

**Docs:** [Permissions](https://opencode.ai/docs/permissions/) — `edit` permission covers all file modifications.

**Best integration story (no custom IDE hook):**

```jsonc
// opencode.jsonc
{
  "permission": {
    "edit": "deny"           // disable native write/edit/patch
  },
  "mcp": {
    "codebeacon": {
      "type": "local",
      "command": ["codebeacon", "serve", "--fs-tools", "--security"],
      "enabled": true
    }
  },
  "permission": {
    "codebeacon_edit_file": "allow",
    "codebeacon_write_file": "allow"
  }
}
```

(Exact MCP tool names depend on how OpenCode prefixes them — verify with `opencode` tool list.)

**Result:** Agent **must** use Codebeacon MCP for file changes → gate runs.

**Risk:** Model may try native edit, get denied, confuse session — needs `AGENTS.md` instruction.

---

### Hermes Agent

**Edit path:** Built-in tools (`read_file`, `terminal`, …) **plus** MCP tools registered as `mcp_<server>_<tool>`.

**Config:** `~/.hermes/config.yaml` → `mcp_servers` with `tools.include` filtering.

**Coverage options:**
1. Add Codebeacon MCP with `--fs-tools --security`
2. **Do not** add separate `@modelcontextprotocol/server-filesystem` (bypasses gate)
3. Agent rules: "only use mcp_codebeacon_* for file writes" — **soft enforcement**
4. No documented `preToolUse`-style gate for built-in Hermes file tools

**Verdict:** Weaker than OpenCode unless Hermes adds pre-tool hooks or you disable built-in file tools (if possible).

---

### LM Studio / local models without file access

**Edit path:** Model has no native filesystem → must use MCP.

**Setup:** `codebeacon serve --fs-tools --security --root /path`

**Verdict:** ✅ **Current architecture works.** This is what `--fs-tools` was designed for.

---

### VS Code, Zed, Cline

**Edit path:** Extension provides file tools; MCP is separate.

**Coverage:** Same gap as Cursor unless extension exposes pre-edit hooks. Fallback: **git pre-commit hook** or **CI** running `codebeacon verify`.

---

## Three enforcement tiers (product architecture)

Build all three; no single tier covers every client.

```
Tier 1 — MCP gate (done)
  codebeacon write_file / edit_file + apply_security_gate
  Works: LM Studio, OpenCode (with edit denied), agents forced to MCP

Tier 2 — Host hooks (Phase 2.5)
  Cursor preToolUse, Claude PreToolUse → codebeacon verify CLI
  Works: Cursor, Claude Code native edit path

Tier 3 — Repo pipeline (Phase 3)
  git pre-commit, GitHub Action, daemon post-write scan
  Works: every client, every human edit; not edit-time block
```

---

## Required shared primitive: `codebeacon verify` CLI

Hooks and CI cannot call MCP easily. Add:

```bash
codebeacon verify --content 'malloc(n * sizeof(int));' [--path alloc.c] [--json]
# Exit 0 = Allow or Warn
# Exit 1 = Block (ProvenVulnerable or policy block)
# stdout: VerifyReport JSON when --json
```

Single implementation calls `verify_fragment()` + `decide()` — same logic as MCP gate.

**Owner:** Phase 2.5 agent (after or parallel to Z3 Phase 2).

---

## Recommended config snippets (documentation only)

### Cursor + security hook (sketch)

```json
{
  "version": 1,
  "hooks": {
    "preToolUse": [
      {
        "command": ".cursor/hooks/codebeacon-security.sh",
        "matcher": "Write",
        "failClosed": false
      }
    ]
  }
}
```

### Claude Code (sketch)

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [{ "type": "command", "command": ".claude/hooks/codebeacon-security.sh" }]
      }
    ]
  }
}
```

### OpenCode — force MCP path

See OpenCode section above (`permission.edit: deny`).

---

## What to test (QA checklist)

| # | Client | Action | Expected |
|---|--------|--------|----------|
| 1 | LM Studio | `write_file` via MCP with CWE-190 | Block or Warn per policy |
| 2 | Cursor | Native Apply bad `malloc` line | **Fail today**; pass after hook |
| 3 | Claude Code | `Edit` bad line | **Fail today**; pass after hook |
| 4 | OpenCode | `edit: deny` + MCP `edit_file` | Block via gate |
| 5 | Hermes | MCP `edit_file` only | Block via gate |
| 6 | All | `verify_security` manual call | Report findings |

---

## Out of scope for this doc

- Z3 encoding implementation → `src/security/z3/`
- Cloud audit / team dashboard
- Disabling human edits (only agent edits in scope)

---

*Last updated: 2026-07-13*

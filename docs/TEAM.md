# Team Workflow

## Sharing the index

Two patterns (like Graphify's `graphify-out/` vs per-dev rebuild):

### Option A: Commit `.codeindex/` (instant onboarding)

```gitignore
# Remove .codeindex/ from .gitignore if you want team-wide index
# .codeindex/
```

Pros: new clones have instant MCP context. Cons: merge conflicts on index files.

### Option B: Per-developer index (default)

`.codeindex/` is auto-added to `.gitignore` on first `codebeacon init`.

Each developer runs:

```bash
codebeacon init
codebeacon hook install   # optional: re-index on commit
```

## CI: security verification

```yaml
# .github/workflows/codebeacon-verify.yml
name: Codebeacon Security
on: [pull_request]
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install codebeacon
        run: cargo install codebeacon
      - name: Verify changed Rust files
        run: |
          for f in $(git diff --name-only origin/main -- '*.rs'); do
            codebeacon verify --content "$(cat "$f")" --path "$f" || exit 1
          done
```

## Reports for onboarding

Generate a committed report for new team members:

```bash
codebeacon report -o CODEBEACON_REPORT.md
git add CODEBEACON_REPORT.md
```

## Multi-repo workspaces

Point MCP at a parent directory containing multiple git repos. Use the `repo` argument in MCP tools to scope queries.

## Hooks

| Hook | Purpose | Install |
|------|---------|---------|
| Discovery | Nudge get_context | `codebeacon install --platform cursor` |
| Security | Block CWE-190 edits | Copy `codebeacon-security.sh` |
| Post-commit | Re-index | `codebeacon hook install` |

---
name: Bug Report
about: Report a bug to help us improve Codebeacon
title: "bug: "
labels: bug
assignees: ""
---

## Description

A clear and concise description of the bug.

## Reproduction Steps

1. Run `codebeacon ...`
2. Open file `...`
3. Call tool `...`
4. See error

## Expected vs Actual

**Expected:** What should happen.

**Actual:** What actually happens.

## Environment

- OS: [e.g. macOS 14, Ubuntu 22.04, Windows 11]
- Codebeacon version: `codebeacon --version` or `cargo install --version codebeacon`
- Client: [e.g. Claude Code, Cursor, VS Code, LM Studio, Cline]
- Rust version (if building from source): `rustc --version`

## Logs / Screenshots

If applicable, add logs or error output. Run the server with `RUST_LOG=debug` for more detail:

```bash
RUST_LOG=debug codebeacon serve
```

## Additional Context

Anything else relevant — repo size, languages used, multi-repo workspace?]

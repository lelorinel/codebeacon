#!/bin/sh
# Codebeacon discovery hook — nudge only (not blocking).
# Reminds the agent to prefer get_context when .codeindex exists.

ROOT="${CLAUDE_PROJECT_DIR:-${CURSOR_WORKSPACE:-$(pwd)}}"

find_index() {
  if [ -f "$ROOT/.codeindex/index.json" ]; then
    return 0
  fi
  # Walk up for git root
  dir="$ROOT"
  while [ "$dir" != "/" ]; do
    if [ -f "$dir/.codeindex/index.json" ]; then
      ROOT="$dir"
      return 0
    fi
    if [ -d "$dir/.git" ]; then
      return 1
    fi
    dir=$(dirname "$dir")
  done
  return 1
}

if find_index; then
  echo "Codebeacon: .codeindex exists at $ROOT — prefer MCP get_context over grep/Read/Glob." >&2
fi
exit 0

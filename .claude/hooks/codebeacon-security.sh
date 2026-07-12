#!/usr/bin/env bash
# Claude Code PreToolUse hook — gate native Edit/Write on CWE-190 policy.
# Merge .claude/settings.security.example.json into your settings to enable.
set -euo pipefail

input=$(cat)
path=$(echo "$input" | jq -r '.tool_input.file_path // .tool_input.path // "fragment"')
content=$(echo "$input" | jq -r '.tool_input.new_string // .tool_input.content // .tool_input.contents // empty')

if [ -z "$content" ] || [ "$content" = "null" ]; then
  exit 0
fi

if ! out=$(codebeacon verify --content "$content" --path "$path" --json 2>&1); then
  msg=$(echo "$out" | jq -r '.message // "Codebeacon security verification blocked this edit."')
  jq -n \
    --arg msg "$msg" \
    '{
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "deny",
        permissionDecisionReason: $msg
      }
    }'
  exit 0
fi

exit 0

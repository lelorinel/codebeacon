#!/usr/bin/env bash
# Cursor preToolUse hook — gate native Write on CWE-190 policy.
# Copy .cursor/hooks.json.example → .cursor/hooks.json to enable.
set -euo pipefail

input=$(cat)
path=$(echo "$input" | jq -r '.tool_input.path // .tool_input.file_path // "fragment"')
content=$(echo "$input" | jq -r '.tool_input.contents // .tool_input.content // .tool_input.new_string // empty')

if [ -z "$content" ] || [ "$content" = "null" ]; then
  echo '{"permission":"allow"}'
  exit 0
fi

if ! out=$(codebeacon verify --content "$content" --path "$path" --json 2>&1); then
  msg=$(echo "$out" | jq -r '.message // "Codebeacon security verification blocked this edit."')
  jq -n \
    --arg msg "$msg" \
    '{
      permission: "deny",
      user_message: "Edit blocked by Codebeacon security gate.",
      agent_message: $msg
    }'
  exit 0
fi

echo '{"permission":"allow"}'

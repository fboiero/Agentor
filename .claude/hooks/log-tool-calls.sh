#!/bin/bash
# Agentor — Log all tool calls and outputs
# Runs on PostToolUse and PostToolUseFailure events

INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id')
EVENT=$(echo "$INPUT" | jq -r '.hook_event_name')
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')
TOOL_USE_ID=$(echo "$INPUT" | jq -r '.tool_use_id // "unknown"')

LOG_DIR="$HOME/.claude/hook-stats"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/tool-calls-$(date +%Y%m%d).log"

{
  echo "=== $EVENT at $(date -u +%Y-%m-%dT%H:%M:%SZ) ==="
  echo "Session: $SESSION_ID"
  echo "Tool: $TOOL_NAME"
  echo "Use ID: $TOOL_USE_ID"
  echo "Input:"
  echo "$INPUT" | jq '.tool_input' 2>/dev/null || echo "(no input)"

  if [ "$EVENT" = "PostToolUse" ]; then
    echo "Response (truncated to 500 chars):"
    echo "$INPUT" | jq -r '.tool_response | tostring' 2>/dev/null | head -c 500
    echo ""
  elif [ "$EVENT" = "PostToolUseFailure" ]; then
    echo "Error:"
    echo "$INPUT" | jq -r '.error // "unknown error"' 2>/dev/null
  fi
  echo ""
} >> "$LOG_FILE" 2>/dev/null

exit 0

#!/bin/bash
# Agentor — Track bash command outputs separately
# Runs on PostToolUse, filters only Bash tool

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')

# Only log Bash commands
if [ "$TOOL_NAME" != "Bash" ]; then
  exit 0
fi

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id')
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // "unknown"')
EVENT=$(echo "$INPUT" | jq -r '.hook_event_name')

LOG_DIR="$HOME/.claude/hook-stats"
mkdir -p "$LOG_DIR"
BASH_LOG="$LOG_DIR/bash-commands-$(date +%Y%m%d).log"

{
  echo "[$EVENT] $(date -u +%H:%M:%SZ) | $COMMAND"

  if [ "$EVENT" = "PostToolUse" ]; then
    EXIT_CODE=$(echo "$INPUT" | jq -r '.tool_response.exit_code // "?"')
    STDOUT=$(echo "$INPUT" | jq -r '.tool_response.stdout // ""' 2>/dev/null | head -c 1000)
    STDERR=$(echo "$INPUT" | jq -r '.tool_response.stderr // ""' 2>/dev/null | head -c 500)
    echo "  exit=$EXIT_CODE"
    if [ -n "$STDOUT" ]; then
      echo "  stdout: $STDOUT"
    fi
    if [ -n "$STDERR" ]; then
      echo "  stderr: $STDERR"
    fi
  fi
  echo "---"
} >> "$BASH_LOG" 2>/dev/null

exit 0

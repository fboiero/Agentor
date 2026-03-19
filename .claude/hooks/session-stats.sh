#!/bin/bash
# Agentor — Compile session statistics on session start/end
# Captures model info on start, summarizes usage on end

INPUT=$(cat)
EVENT=$(echo "$INPUT" | jq -r '.hook_event_name')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id')

STATS_DIR="$HOME/.claude/hook-stats"
mkdir -p "$STATS_DIR"
STATS_FILE="$STATS_DIR/sessions.jsonl"

if [ "$EVENT" = "SessionStart" ]; then
  MODEL=$(echo "$INPUT" | jq -r '.model // "unknown"')
  SOURCE=$(echo "$INPUT" | jq -r '.source // "unknown"')

  jq -n \
    --arg sid "$SESSION_ID" \
    --arg event "start" \
    --arg model "$MODEL" \
    --arg source "$SOURCE" \
    --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    '{session_id: $sid, event: $event, model: $model, source: $source, timestamp: $ts}' \
    >> "$STATS_FILE" 2>/dev/null

elif [ "$EVENT" = "SessionEnd" ]; then
  REASON=$(echo "$INPUT" | jq -r '.reason // "other"')
  TRANSCRIPT=$(echo "$INPUT" | jq -r '.transcript_path // ""')

  TOOL_COUNT=0
  BASH_COUNT=0
  EDIT_COUNT=0
  READ_COUNT=0

  if [ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ]; then
    TOOL_COUNT=$(grep -c '"tool_use"' "$TRANSCRIPT" 2>/dev/null || echo 0)
    BASH_COUNT=$(grep -c '"Bash"' "$TRANSCRIPT" 2>/dev/null || echo 0)
    EDIT_COUNT=$(grep -c '"Edit"' "$TRANSCRIPT" 2>/dev/null || echo 0)
    READ_COUNT=$(grep -c '"Read"' "$TRANSCRIPT" 2>/dev/null || echo 0)
  fi

  jq -n \
    --arg sid "$SESSION_ID" \
    --arg event "end" \
    --arg reason "$REASON" \
    --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --argjson tools "$TOOL_COUNT" \
    --argjson bash "$BASH_COUNT" \
    --argjson edit "$EDIT_COUNT" \
    --argjson read "$READ_COUNT" \
    '{session_id: $sid, event: $event, reason: $reason, timestamp: $ts, tool_calls: $tools, bash_calls: $bash, edit_calls: $edit, read_calls: $read}' \
    >> "$STATS_FILE" 2>/dev/null
fi

exit 0

#!/bin/bash
# Claude Code -> Telegram notification hook
# Reads hook event JSON from stdin, sends formatted message to Telegram
#
# Setup:
#   1. Create bot: message @BotFather on Telegram, /newbot
#   2. Get chat_id: message @userinfobot on Telegram
#   3. Create ~/.claude/telegram-notify.conf with:
#        TELEGRAM_BOT_TOKEN="123456789:AAH..."
#        TELEGRAM_CHAT_ID="987654321"
#        NOTIFY_LEVEL="important"
#   4. This script is already registered in .claude/settings.json

set -uo pipefail

# ---- Configuration ----
CONFIG_FILE="$HOME/.claude/telegram-notify.conf"
if [ ! -f "$CONFIG_FILE" ]; then
  exit 0
fi
# shellcheck source=/dev/null
source "$CONFIG_FILE"

if [ -z "${TELEGRAM_BOT_TOKEN:-}" ] || [ -z "${TELEGRAM_CHAT_ID:-}" ]; then
  exit 0
fi

# ---- Rate limiting (5s between messages) ----
RATE_FILE="/tmp/claude-tg-rate"
if [ -f "$RATE_FILE" ]; then
  LAST_SEND=$(cat "$RATE_FILE" 2>/dev/null || echo 0)
  NOW=$(date +%s)
  if [ $((NOW - LAST_SEND)) -lt "${RATE_WINDOW:-3}" ]; then
    exit 0
  fi
fi

# ---- Parse input ----
INPUT=$(cat)
EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // "unknown"')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"' | head -c 8)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')
CWD=$(echo "$INPUT" | jq -r '.cwd // ""')
PROJECT=$(basename "$CWD" 2>/dev/null || echo "?")

# HTML escape helper
html_escape() {
  echo "$1" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g'
}

# ---- Build message ----
case "$EVENT" in

  SessionStart)
    MODEL=$(echo "$INPUT" | jq -r '.model // "unknown"' | sed 's/\[.*//') # strip [1m] etc
    SOURCE=$(echo "$INPUT" | jq -r '.source // "unknown"')
    MSG="🟢 <b>Session Started</b>
📦 <b>Project:</b> <code>${PROJECT}</code>
🤖 <b>Model:</b> <code>${MODEL}</code>
🔄 <b>Source:</b> ${SOURCE}
🔑 <b>Session:</b> <code>${SESSION_ID}…</code>"
    ;;

  SessionEnd)
    REASON=$(echo "$INPUT" | jq -r '.reason // "unknown"')
    MSG="🔴 <b>Session Ended</b>
📦 <b>Project:</b> <code>${PROJECT}</code>
📝 <b>Reason:</b> ${REASON}
🔑 <b>Session:</b> <code>${SESSION_ID}…</code>"
    ;;

  PostToolUse)
    # Skip reads unless NOTIFY_LEVEL=all
    if [ "$TOOL_NAME" = "Read" ] || [ "$TOOL_NAME" = "Glob" ] || [ "$TOOL_NAME" = "Grep" ]; then
      [ "${NOTIFY_LEVEL:-important}" != "all" ] && exit 0
    fi

    if [ "$TOOL_NAME" = "Bash" ]; then
      CMD=$(echo "$INPUT" | jq -r '.tool_input.command // ""' | head -c 300)
      CMD_ESC=$(html_escape "$CMD")
      MSG="⚡ <b>Bash</b> [${PROJECT}]
<code>${CMD_ESC}</code>"

    elif [ "$TOOL_NAME" = "Edit" ]; then
      FILE=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""' | xargs basename 2>/dev/null || echo "?")
      MSG="✏️ <b>Edit</b> [${PROJECT}]: <code>${FILE}</code>"

    elif [ "$TOOL_NAME" = "Write" ]; then
      FILE=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""' | xargs basename 2>/dev/null || echo "?")
      MSG="📝 <b>Write</b> [${PROJECT}]: <code>${FILE}</code>"

    elif [ "$TOOL_NAME" = "Read" ]; then
      FILE=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""' | xargs basename 2>/dev/null || echo "?")
      MSG="👁 <b>Read</b> [${PROJECT}]: <code>${FILE}</code>"

    elif [ "$TOOL_NAME" = "Agent" ]; then
      DESC=$(echo "$INPUT" | jq -r '.tool_input.description // ""' | head -c 100)
      MSG="🤖 <b>Agent</b> [${PROJECT}]: ${DESC}"

    else
      MSG="🔧 <b>${TOOL_NAME}</b> [${PROJECT}]"
    fi
    ;;

  PostToolUseFailure)
    ERROR=$(echo "$INPUT" | jq -r '.error // "unknown error"' | head -c 500)
    ERROR_ESC=$(html_escape "$ERROR")
    MSG="❌ <b>FAILED: ${TOOL_NAME}</b> [${PROJECT}]
<code>${ERROR_ESC}</code>"
    ;;

  Stop)
    REASON=$(echo "$INPUT" | jq -r '.reason // ""' | head -c 200)
    MSG="⏹ <b>Agent stopping</b> [${PROJECT}]
${REASON}"
    ;;

  Notification)
    MSG="🔔 <b>Claude needs attention!</b> [${PROJECT}]
Check the terminal."
    ;;

  UserPromptSubmit)
    [ "${NOTIFY_LEVEL:-important}" != "all" ] && exit 0
    PROMPT=$(echo "$INPUT" | jq -r '.user_prompt // ""' | head -c 200)
    PROMPT_ESC=$(html_escape "$PROMPT")
    MSG="💬 <b>User</b> [${PROJECT}]: ${PROMPT_ESC}"
    ;;

  *)
    exit 0
    ;;
esac

# ---- Send (fire-and-forget, don't block Claude) ----
curl -s -X POST "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage" \
  -d chat_id="$TELEGRAM_CHAT_ID" \
  -d parse_mode="HTML" \
  -d disable_notification="${SILENT_MODE:-false}" \
  --data-urlencode "text=${MSG}" \
  --max-time 5 \
  > /dev/null 2>&1 &

# Update rate limit
date +%s > "$RATE_FILE" 2>/dev/null

exit 0

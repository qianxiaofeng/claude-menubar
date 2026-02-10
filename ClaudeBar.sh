#!/bin/zsh
# <swiftbar.hideAbout>true</swiftbar.hideAbout>
# <swiftbar.hideRunInTerminal>true</swiftbar.hideRunInTerminal>
# <swiftbar.hideDisablePlugin>true</swiftbar.hideDisablePlugin>
#
# Each symlink (ClaudeBar-N.2s.sh) monitors the Nth Claude Code session.
# Empty output when no session exists → icon auto-hides.

# Resolve symlinks so HELPER works when SwiftBar runs a symlinked plugin
SELF="$0"
ORIG="$SELF"
if [[ -L "$SELF" ]]; then
    LINK="$(readlink "$SELF")"
    [[ "$LINK" != /* ]] && LINK="$(dirname "$SELF")/$LINK"
    SELF="$LINK"
fi
SCRIPT_DIR="$(cd "$(dirname "$SELF")" && pwd)"
HELPER="$SCRIPT_DIR/focus-iterm.sh"

# Extract slot number from filename: ClaudeBar-N.2s.sh → N
# If no slot number found (running directly), default to 1
SLOT_NUM=1
BASENAME="$(basename "$ORIG")"
if [[ "$BASENAME" =~ ^ClaudeBar-([0-9]+)\. ]]; then
    SLOT_NUM=${match[1]}
fi

# sfconfig base64: {"renderingMode":"Palette","colors":["<hex>"]}
SF_GREEN=eyJyZW5kZXJpbmdNb2RlIjoiUGFsZXR0ZSIsImNvbG9ycyI6WyIjMzJENzRCIl19    # #32D74B
SF_ORANGE=eyJyZW5kZXJpbmdNb2RlIjoiUGFsZXR0ZSIsImNvbG9ycyI6WyIjRkY5RjBBIl19   # #FF9F0A
SF_GRAY=eyJyZW5kZXJpbmdNb2RlIjoiUGFsZXR0ZSIsImNvbG9ycyI6WyIjOEU4RTkzIl19     # #8E8E93

PIDS=($(pgrep -x claude 2>/dev/null | sort -n))
PID_COUNT=${#PIDS[@]}

# If the Nth process doesn't exist, output nothing → icon hides
if (( PID_COUNT < SLOT_NUM )); then
    exit 0
fi

PID=${PIDS[$SLOT_NUM]}
TTY_DEV="/dev/$(ps -o tty= -p "$PID" 2>/dev/null | xargs)"
CWD=$(lsof -p "$PID" -Fn 2>/dev/null | grep '^n/' | head -1 | cut -c2-)
PROJECT_HASH=$(echo "$CWD" | sed 's|[/_]|-|g')
TRANSCRIPT=$(ls -t "$HOME/.claude/projects/$PROJECT_HASH"/*.jsonl 2>/dev/null | head -1)
if [[ -n "$TRANSCRIPT" ]]; then
    FILE_AGE=$(( $(date +%s) - $(stat -f %m "$TRANSCRIPT") ))
else
    FILE_AGE=0
fi

/usr/bin/python3 - "$HELPER" "$SF_GREEN" "$SF_ORANGE" "$SF_GRAY" \
    "$TTY_DEV" "$CWD" "$TRANSCRIPT" "$FILE_AGE" << 'PYEOF'
import json, sys, os

helper = sys.argv[1]
sf_green = sys.argv[2]
sf_orange = sys.argv[3]
sf_gray = sys.argv[4]
tty = sys.argv[5]
cwd = sys.argv[6]
transcript = sys.argv[7]
file_age = int(sys.argv[8])

STATUS_MAP = {
    "active":   ("bolt.fill",                   sf_green),
    "pending":  ("exclamationmark.triangle.fill", sf_orange),
    "idle":     ("moon.fill",                    sf_gray),
}
STATUS_LABEL = {"active": "Running", "pending": "Needs input", "idle": "Idle"}

def check_pending_tool(transcript):
    if not transcript:
        return False
    pending = False
    try:
        with open(transcript, 'rb') as f:
            f.seek(0, 2)
            size = f.tell()
            chunk = min(size, 65536)
            f.seek(size - chunk)
            lines = f.read().decode('utf-8', errors='replace').strip().split('\n')
        for line in lines:
            line = line.strip()
            if not line:
                continue
            try:
                e = json.loads(line)
            except Exception:
                continue
            t = e.get('type', '')
            msg = e.get('message', {})
            role = msg.get('role', '')
            content = msg.get('content', [])
            if t == 'assistant' and role == 'assistant':
                if isinstance(content, list):
                    types = [c.get('type') for c in content if isinstance(c, dict)]
                    if 'tool_use' in types:
                        pending = True
            elif t == 'user' and role == 'user' and isinstance(content, list):
                types = [c.get('type') for c in content if isinstance(c, dict)]
                if 'tool_result' in types:
                    pending = False
    except Exception:
        pass
    return pending

# Determine status
if not transcript:
    status = "active"
elif file_age < 15:
    status = "active"
elif check_pending_tool(transcript):
    status = "pending"
else:
    status = "idle"

click = f"bash={helper} param1={tty} terminal=false"
project = os.path.basename(cwd) if cwd else ""
img, cfg = STATUS_MAP[status]

# Menu bar: status icon, click to focus
print(f"| sfimage={img} sfconfig={cfg} sfsize=15 {click}")

# Dropdown
print("---")
label = project if project else "Claude"
print(f"{label} | sfimage={img} sfconfig={cfg} {click}")
print(f"--{STATUS_LABEL[status]} | sfimage={img} sfconfig={cfg} size=12")
PYEOF

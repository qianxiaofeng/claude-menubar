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
STATE_DIR="$SCRIPT_DIR/../.swiftbar"
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

# Read shared cache built by claude-status-cache.sh (runs every 2s)
source "$STATE_DIR/cache.env" 2>/dev/null || exit 0

# If the Nth session doesn't exist, output nothing → icon hides
(( SLOT_COUNT < SLOT_NUM )) && exit 0

eval "TTY_DEV=\$SLOT_${SLOT_NUM}_TTY"
eval "CWD=\$SLOT_${SLOT_NUM}_CWD"
eval "TRANSCRIPT=\$SLOT_${SLOT_NUM}_TRANSCRIPT"
eval "MTIME=\$SLOT_${SLOT_NUM}_MTIME"
eval "PREV_MTIME=\$SLOT_${SLOT_NUM}_PREV_MTIME"

/usr/bin/python3 - "$HELPER" "$SF_GREEN" "$SF_ORANGE" "$SF_GRAY" \
    "$TTY_DEV" "$CWD" "$TRANSCRIPT" "$MTIME" "$PREV_MTIME" << 'PYEOF'
import json, sys, os

helper = sys.argv[1]
sf_green = sys.argv[2]
sf_orange = sys.argv[3]
sf_gray = sys.argv[4]
tty = sys.argv[5]
cwd = sys.argv[6]
transcript = sys.argv[7]
mtime = sys.argv[8]
prev_mtime = sys.argv[9]

STATUS_MAP = {
    "active":   ("bolt.fill",                   sf_green),
    "pending":  ("exclamationmark.triangle.fill", sf_orange),
    "idle":     ("moon.fill",                    sf_gray),
}
STATUS_LABEL = {"active": "Running", "pending": "Needs input", "idle": "Idle"}

def check_pending_tool(transcript):
    """Parse transcript tail for unpaired tool_use."""
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

def determine_status(transcript, mtime, prev_mtime):
    """Determine status via mtime delta between cache cycles."""
    if not transcript:
        return "active"
    if mtime != prev_mtime:
        return "active"
    if check_pending_tool(transcript):
        return "pending"
    return "idle"

status = determine_status(transcript, mtime, prev_mtime)

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

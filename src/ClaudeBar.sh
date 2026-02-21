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

/usr/bin/python3 - "$HELPER" "$SF_GREEN" "$SF_ORANGE" "$SF_GRAY" \
    "$TTY_DEV" "$CWD" "$TRANSCRIPT" "$MTIME" << 'PYEOF'
import json, sys, os, time

helper = sys.argv[1]
sf_green = sys.argv[2]
sf_orange = sys.argv[3]
sf_gray = sys.argv[4]
tty = sys.argv[5]
cwd = sys.argv[6]
transcript = sys.argv[7]
mtime = sys.argv[8]

STATUS_MAP = {
    "active":   ("bolt.fill",                   sf_green),
    "pending":  ("exclamationmark.triangle.fill", sf_orange),
    "idle":     ("moon.fill",                    sf_gray),
}
STATUS_LABEL = {"active": "Running", "pending": "Needs input", "idle": "Idle"}

def parse_transcript_tail(transcript):
    """Parse transcript tail, return (last_role, has_pending_tool).

    last_role: 'user' | 'assistant' | None
    has_pending_tool: True if last assistant message has unpaired tool_use
    """
    if not transcript:
        return None, False
    last_role = None
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
                last_role = 'assistant'
                if isinstance(content, list):
                    types = [c.get('type') for c in content if isinstance(c, dict)]
                    pending = 'tool_use' in types
            elif t == 'user' and role == 'user':
                last_role = 'user'
                if isinstance(content, list):
                    types = [c.get('type') for c in content if isinstance(c, dict)]
                    if 'tool_result' in types:
                        pending = False
    except Exception:
        pass
    return last_role, pending

def determine_status(transcript, mtime_unused):
    """Determine status via transcript content and mtime age."""
    if not transcript:
        return "active"
    try:
        mtime = os.path.getmtime(transcript)
    except OSError:
        return "active"
    age = time.time() - mtime

    # Always parse transcript (cheap: last 64KB)
    last_role, pending = parse_transcript_tail(transcript)

    # Pending: tool_use waiting for user action
    # 3s grace period filters auto-approved tools (complete in <2s)
    # 120s timeout degrades to idle (session likely abandoned)
    if pending and age >= 3:
        return "pending" if age < 120 else "idle"

    # Recent activity -> active
    if age < 10:
        return "active"

    # User sent message, Claude processing (API call)
    if last_role == 'user':
        return "active" if age < 120 else "idle"

    # Assistant finished -> idle
    return "idle"

status = determine_status(transcript, mtime)

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

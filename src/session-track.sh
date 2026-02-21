#!/bin/sh
# SessionStart hook: maps TTY → transcript path for SwiftBar slot isolation.
# Called by Claude Code with JSON on stdin containing session_id + transcript_path.

# Walk up the process tree to find the claude process and its TTY
PID=$PPID
TTY=""
while [ "$PID" != "1" ] && [ -n "$PID" ]; do
    NAME=$(ps -o comm= -p "$PID" 2>/dev/null)
    if [ "$NAME" = "claude" ]; then
        TTY=$(ps -o tty= -p "$PID" 2>/dev/null | xargs)
        break
    fi
    PID=$(ps -o ppid= -p "$PID" 2>/dev/null | xargs)
done

# No TTY found or detached — nothing to do
[ -z "$TTY" ] || [ "$TTY" = "??" ] && exit 0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
STATE_DIR="$SCRIPT_DIR/../.swiftbar"
mkdir -p "$STATE_DIR"

# Read hook JSON from stdin, write state file for transcript resolution
python3 -c "
import json, sys, os
d = json.load(sys.stdin)
state_dir = sys.argv[1]
tty = sys.argv[2]
with open(os.path.join(state_dir, 'session-' + tty + '.json'), 'w') as f:
    json.dump({'session_id': d.get('session_id',''), 'transcript_path': d.get('transcript_path','')}, f)
" "$STATE_DIR" "$TTY"

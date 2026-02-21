#!/bin/zsh
# <swiftbar.hideAbout>true</swiftbar.hideAbout>
# <swiftbar.hideRunInTerminal>true</swiftbar.hideRunInTerminal>
# <swiftbar.hideDisablePlugin>true</swiftbar.hideDisablePlugin>
# <swiftbar.hideSwiftBar>true</swiftbar.hideSwiftBar>
#
# Cache builder: runs expensive queries once per cycle (AppleScript, pgrep, lsof).
# Writes results to .swiftbar/cache.env for slot plugins to source.
# Empty output → no menu bar icon for this plugin.

# Resolve symlink to find project root
SELF="$0"
if [[ -L "$SELF" ]]; then
    LINK="$(readlink "$SELF")"
    [[ "$LINK" != /* ]] && LINK="$(dirname "$SELF")/$LINK"
    SELF="$LINK"
fi
SCRIPT_DIR="$(cd "$(dirname "$SELF")" && pwd)"
STATE_DIR="$SCRIPT_DIR/../.swiftbar"
mkdir -p "$STATE_DIR"

# 1) Get all iTerm2 session TTYs in tab order (window → tab → session)
ITERM_TTYS=("${(@f)$(osascript -e '
tell application "iTerm2"
    set out to ""
    repeat with w in windows
        tell w
            repeat with t in tabs
                repeat with s in sessions of t
                    set out to out & (tty of s) & linefeed
                end repeat
            end repeat
        end tell
    end repeat
    return out
end tell
' 2>/dev/null)}")

# 2) Map claude PIDs by their TTY
typeset -A PID_BY_TTY
for p in $(pgrep -x claude 2>/dev/null); do
    t="/dev/$(ps -o tty= -p "$p" 2>/dev/null | xargs)"
    [[ "$t" != "/dev/" ]] && PID_BY_TTY[$t]=$p
done

# 3) Filter to TTYs running Claude, preserving iTerm tab order
CLAUDE_TTYS=()
for t in "${ITERM_TTYS[@]}"; do
    [[ -n "${PID_BY_TTY[$t]}" ]] && CLAUDE_TTYS+=("$t")
done

count=${#CLAUDE_TTYS[@]}

# Build ACTIVE_CLAUDE_TTYS comma-separated list (short form)
ACTIVE_CLAUDE_TTYS=""
for _ct in "${CLAUDE_TTYS[@]}"; do
    ACTIVE_CLAUDE_TTYS="${ACTIVE_CLAUDE_TTYS:+$ACTIVE_CLAUDE_TTYS,}${_ct#/dev/}"
done

# Write cache atomically (tmp + mv)
TMP="$STATE_DIR/cache.env.tmp"
{
    echo "CACHE_TS=$(date +%s)"
    echo "SLOT_COUNT=$count"
    echo "ACTIVE_CLAUDE_TTYS=$ACTIVE_CLAUDE_TTYS"

    # SwiftBar renders menu bar icons right-to-left: slot1=rightmost.
    # To match visual position (left icon = left tab), reverse the mapping.
    local slot=1
    while (( slot <= count )); do
        local idx=$(( count - slot + 1 ))
        TTY_DEV="${CLAUDE_TTYS[$idx]}"
        PID="${PID_BY_TTY[$TTY_DEV]}"
        CWD=$(lsof -p "$PID" -Fn 2>/dev/null | grep '^n/' | head -1 | cut -c2-)
        PROJECT_HASH=$(echo "$CWD" | sed 's|[/_]|-|g')
        TTY_SHORT="${TTY_DEV#/dev/}"

        TRANSCRIPT=$(/usr/bin/python3 "$SCRIPT_DIR/resolve_transcript.py" \
            "$TTY_SHORT" "$STATE_DIR" "$HOME/.claude/projects/$PROJECT_HASH" "$ACTIVE_CLAUDE_TTYS")
        if [[ -n "$TRANSCRIPT" && -f "$TRANSCRIPT" ]]; then
            MTIME=$(stat -f %m "$TRANSCRIPT")
        else
            TRANSCRIPT=""
            MTIME=""
        fi
        echo "SLOT_${slot}_TTY=$TTY_DEV"
        echo "SLOT_${slot}_PID=$PID"
        echo "SLOT_${slot}_CWD=$CWD"
        echo "SLOT_${slot}_PROJECT_HASH=$PROJECT_HASH"
        echo "SLOT_${slot}_TTY_SHORT=$TTY_SHORT"
        echo "SLOT_${slot}_TRANSCRIPT=$TRANSCRIPT"
        echo "SLOT_${slot}_MTIME=$MTIME"
        (( slot++ ))
    done
} > "$TMP"
mv "$TMP" "$STATE_DIR/cache.env"

# Output nothing → no menu bar icon

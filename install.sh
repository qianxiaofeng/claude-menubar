#!/bin/bash
# Install ClaudeBar symlinks into the SwiftBar plugins directory.
# Usage: ./install.sh [SLOTS]   (default: 5)

set -euo pipefail

SLOTS=${1:-5}
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

PLUGIN_DIR=$(defaults read com.ameba.SwiftBar PluginDirectory 2>/dev/null) || {
    echo "Error: Could not read SwiftBar plugin directory."
    echo "Is SwiftBar installed and configured?"
    exit 1
}

# Expand ~ if present
PLUGIN_DIR="${PLUGIN_DIR/#\~/$HOME}"

if [[ ! -d "$PLUGIN_DIR" ]]; then
    echo "Error: Plugin directory does not exist: $PLUGIN_DIR"
    exit 1
fi

# Warn if repo is inside plugin dir without dot-prefix (SwiftBar would execute all files)
case "$SCRIPT_DIR" in
    "$PLUGIN_DIR"/[!.]*)
        echo "Warning: This repo is inside the SwiftBar plugin directory without a dot-prefix."
        echo "SwiftBar will try to execute all files. Rename the directory to start with '.':"
        echo "  mv \"$SCRIPT_DIR\" \"$PLUGIN_DIR/.$(basename "$SCRIPT_DIR")\""
        exit 1
        ;;
esac

# Clean up old formats
rm -f "$PLUGIN_DIR"/ClaudeBar.*.sh
rm -f "$PLUGIN_DIR"/ClaudeBar-*.sh

# Create cache plugin symlink (runs shared queries once per cycle)
ln -sf "$SCRIPT_DIR/src/claude-status-cache.sh" "$PLUGIN_DIR/ClaudeBar-cache.2s.sh"

# Create slot symlinks
for i in $(seq 1 "$SLOTS"); do
    ln -sf "$SCRIPT_DIR/src/ClaudeBar.sh" "$PLUGIN_DIR/ClaudeBar-$i.2s.sh"
done

# Install Claude Code hook for session tracking
SESSION_TRACK_CMD="$SCRIPT_DIR/src/session-track.sh"

# Merge hook config into ~/.claude/settings.json
SETTINGS="$HOME/.claude/settings.json"
python3 -c "
import json, os, sys

path = sys.argv[1]
session_track_cmd = sys.argv[2]

cfg = {}
if os.path.exists(path):
    with open(path) as f:
        cfg = json.load(f)

hooks_cfg = cfg.setdefault('hooks', {})
changed = False

# --- Cleanup: remove old update-status.sh hooks from previous installs ---
for event in ('UserPromptSubmit', 'Stop', 'Notification', 'SessionEnd'):
    matchers = hooks_cfg.get(event, [])
    filtered = [m for m in matchers
                if not any('update-status.sh' in h.get('command', '')
                           for h in m.get('hooks', []))]
    if len(filtered) != len(matchers):
        changed = True
        if filtered:
            hooks_cfg[event] = filtered
        else:
            hooks_cfg.pop(event, None)

# --- Register SessionStart hook ---
matchers = hooks_cfg.setdefault('SessionStart', [])
already = any('session-track.sh' in h.get('command', '')
              for m in matchers for h in m.get('hooks', []))
if not already:
    matchers.append({'hooks': [{'type': 'command', 'command': session_track_cmd}]})
    changed = True

if not hooks_cfg:
    cfg.pop('hooks', None)

if changed:
    with open(path, 'w') as f:
        json.dump(cfg, f, indent=2)
        f.write('\n')
" "$SETTINGS" "$SESSION_TRACK_CMD"

echo "Installed cache + $SLOTS slot(s) in $PLUGIN_DIR"
ls -la "$PLUGIN_DIR"/ClaudeBar-*.sh
echo "Installed hook: SessionStart"

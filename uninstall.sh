#!/bin/bash
# Uninstall claude-bar: stop daemon, clean up hooks and state.

set -euo pipefail

# 1. Stop daemon
PLIST_LABEL="com.claude.claude-bar-daemon"
PLIST="$HOME/Library/LaunchAgents/$PLIST_LABEL.plist"

launchctl bootout "gui/$(id -u)/$PLIST_LABEL" 2>/dev/null || true
rm -f "$PLIST"
echo "Stopped and removed daemon: $PLIST_LABEL"

# 2. Remove hook config from settings.json
SETTINGS="$HOME/.claude/settings.json"
if [[ -f "$SETTINGS" ]]; then
    python3 -c "
import json, os

path = '$SETTINGS'
with open(path) as f:
    cfg = json.load(f)

hooks = cfg.get('hooks', {})

# Scripts/commands to remove across all event types
remove_patterns = ('session-track.sh', 'update-status.sh', 'claude-bar')

for event in list(hooks.keys()):
    matchers = hooks[event]
    filtered = [m for m in matchers
                if not any(any(s in h.get('command', '') for s in remove_patterns)
                           for h in m.get('hooks', []))]
    if not filtered:
        hooks.pop(event)
    else:
        hooks[event] = filtered

if not hooks:
    cfg.pop('hooks', None)

with open(path, 'w') as f:
    json.dump(cfg, f, indent=2)
    f.write('\n')
"
    echo "Removed hook config from settings"
fi

# 3. Clean up state files
rm -f "$HOME/.claude/claude-bar.sock"
rm -rf "$HOME/.claude/claude-bar"
echo "Cleaned up state files"
echo "Note: You may remove any leftover .claude-bar/ directories from project folders"

echo "Uninstallation complete."

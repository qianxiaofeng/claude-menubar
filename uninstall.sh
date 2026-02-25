#!/bin/bash
# Uninstall claude-bar: stop daemon, remove plugin, clean up hooks and state.

set -euo pipefail

# 1. Stop daemon
PLIST_LABEL="com.claude.swiftbar-daemon"
PLIST="$HOME/Library/LaunchAgents/$PLIST_LABEL.plist"

launchctl bootout "gui/$(id -u)/$PLIST_LABEL" 2>/dev/null || true
rm -f "$PLIST"
echo "Stopped and removed daemon: $PLIST_LABEL"

# 2. Remove SwiftBar plugins
PLUGIN_DIR=$(defaults read com.ameba.SwiftBar PluginDirectory 2>/dev/null) || {
    echo "Warning: Could not read SwiftBar plugin directory."
    PLUGIN_DIR=""
}

if [[ -n "$PLUGIN_DIR" ]]; then
    PLUGIN_DIR="${PLUGIN_DIR/#\~/$HOME}"
    # Remove new format
    rm -fv "$PLUGIN_DIR"/ClaudeBar.2s.sh
    # Remove old formats
    rm -fv "$PLUGIN_DIR"/ClaudeBar.*.sh
    rm -fv "$PLUGIN_DIR"/ClaudeBar-*.sh
    echo "Removed SwiftBar plugins from $PLUGIN_DIR"
fi

# 3. Remove hook config from settings.json
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

# 4. Clean up socket and state files
rm -f "$HOME/.claude/swiftbar.sock"
rm -rf "$HOME/.claude/swiftbar"
echo "Cleaned up state files"

echo "Uninstallation complete."

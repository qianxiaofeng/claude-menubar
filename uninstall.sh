#!/bin/bash
# Remove all ClaudeBar symlinks from the SwiftBar plugins directory.

set -euo pipefail

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

# Remove new format (ClaudeBar-N.2s.sh)
rm -fv "$PLUGIN_DIR"/ClaudeBar-*.sh

# Remove old format (ClaudeBar.2s.sh)
rm -fv "$PLUGIN_DIR"/ClaudeBar.*.sh

# Remove hook config from settings.json
SETTINGS="$HOME/.claude/settings.json"
if [[ -f "$SETTINGS" ]]; then
    python3 -c "
import json, os

path = '$SETTINGS'
with open(path) as f:
    cfg = json.load(f)

hooks = cfg.get('hooks', {})

# Scripts to remove across all event types
remove_scripts = ('session-track.sh', 'update-status.sh')

for event in list(hooks.keys()):
    matchers = hooks[event]
    filtered = [m for m in matchers
                if not any(any(s in h.get('command', '') for s in remove_scripts)
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

# Clean up swiftbar state files
rm -rf "$HOME/.claude/swiftbar"

echo "Uninstalled all ClaudeBar plugins from $PLUGIN_DIR"

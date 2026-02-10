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

echo "Uninstalled all ClaudeBar plugins from $PLUGIN_DIR"

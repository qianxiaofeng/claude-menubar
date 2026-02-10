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

# Clean up old formats
rm -f "$PLUGIN_DIR"/ClaudeBar.*.sh
rm -f "$PLUGIN_DIR"/ClaudeBar-*.sh

# Create new symlinks
for i in $(seq 1 "$SLOTS"); do
    ln -sf "$SCRIPT_DIR/ClaudeBar.sh" "$PLUGIN_DIR/ClaudeBar-$i.2s.sh"
done

echo "Installed $SLOTS slot(s) in $PLUGIN_DIR"
ls -la "$PLUGIN_DIR"/ClaudeBar-*.sh

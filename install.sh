#!/bin/bash
# Install claude-bar: build binary, create SwiftBar plugin, start daemon, register hook.
# Usage: ./install.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# 1. Build
echo "Building claude-bar..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
BINARY="$SCRIPT_DIR/target/release/claude-bar"

if [[ ! -x "$BINARY" ]]; then
    echo "Error: Build failed, binary not found at $BINARY"
    exit 1
fi

# 2. SwiftBar plugin (single file, replaces old multi-slot approach)
PLUGIN_DIR=$(defaults read com.ameba.SwiftBar PluginDirectory 2>/dev/null) || {
    echo "Error: Could not read SwiftBar plugin directory."
    echo "Is SwiftBar installed and configured?"
    exit 1
}
PLUGIN_DIR="${PLUGIN_DIR/#\~/$HOME}"

if [[ ! -d "$PLUGIN_DIR" ]]; then
    echo "Error: Plugin directory does not exist: $PLUGIN_DIR"
    exit 1
fi

# Warn if repo is inside plugin dir without dot-prefix
case "$SCRIPT_DIR" in
    "$PLUGIN_DIR"/[!.]*)
        echo "Warning: This repo is inside the SwiftBar plugin directory without a dot-prefix."
        echo "SwiftBar will try to execute all files. Rename the directory to start with '.':"
        echo "  mv \"$SCRIPT_DIR\" \"$PLUGIN_DIR/.$(basename "$SCRIPT_DIR")\""
        exit 1
        ;;
esac

# Clean up old formats (multi-slot symlinks, cache plugin)
rm -f "$PLUGIN_DIR"/ClaudeBar.*.sh
rm -f "$PLUGIN_DIR"/ClaudeBar-*.sh

# Create single SwiftBar plugin wrapper
cat > "$PLUGIN_DIR/ClaudeBar.2s.sh" << EOF
#!/bin/sh
# <swiftbar.hideAbout>true</swiftbar.hideAbout>
# <swiftbar.hideRunInTerminal>true</swiftbar.hideRunInTerminal>
# <swiftbar.hideDisablePlugin>true</swiftbar.hideDisablePlugin>
exec "$BINARY" display
EOF
chmod +x "$PLUGIN_DIR/ClaudeBar.2s.sh"

echo "Installed SwiftBar plugin: $PLUGIN_DIR/ClaudeBar.2s.sh"

# 3. Daemon (launchd plist)
PLIST_LABEL="com.claude.swiftbar-daemon"
PLIST="$HOME/Library/LaunchAgents/$PLIST_LABEL.plist"

# Stop existing daemon if running
launchctl bootout "gui/$(id -u)/$PLIST_LABEL" 2>/dev/null || true

cat > "$PLIST" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$PLIST_LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$BINARY</string>
        <string>serve</string>
    </array>
    <key>KeepAlive</key>
    <true/>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/claude-bar.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/claude-bar.err.log</string>
</dict>
</plist>
EOF

launchctl bootstrap "gui/$(id -u)" "$PLIST"
echo "Started daemon: $PLIST_LABEL"

# 4. Register SessionStart hook
HOOK_CMD="$BINARY hook"
SETTINGS="$HOME/.claude/settings.json"

python3 -c "
import json, os, sys

path = sys.argv[1]
hook_cmd = sys.argv[2]

cfg = {}
if os.path.exists(path):
    with open(path) as f:
        cfg = json.load(f)

hooks_cfg = cfg.setdefault('hooks', {})
changed = False

# Cleanup: remove old hooks from previous installs
old_scripts = ('update-status.sh', 'session-track.sh')
for event in ('UserPromptSubmit', 'Stop', 'Notification', 'SessionEnd', 'SessionStart'):
    matchers = hooks_cfg.get(event, [])
    filtered = [m for m in matchers
                if not any(any(s in h.get('command', '') for s in old_scripts)
                           for h in m.get('hooks', []))]
    if len(filtered) != len(matchers):
        changed = True
        if filtered:
            hooks_cfg[event] = filtered
        else:
            hooks_cfg.pop(event, None)

# Register new SessionStart hook
matchers = hooks_cfg.setdefault('SessionStart', [])
already = any(hook_cmd in h.get('command', '')
              for m in matchers for h in m.get('hooks', []))
if not already:
    matchers.append({'hooks': [{'type': 'command', 'command': hook_cmd}]})
    changed = True

if not hooks_cfg:
    cfg.pop('hooks', None)

if changed:
    with open(path, 'w') as f:
        json.dump(cfg, f, indent=2)
        f.write('\n')
" "$SETTINGS" "$HOOK_CMD"

echo "Registered hook: SessionStart → $HOOK_CMD"
echo "Installation complete."

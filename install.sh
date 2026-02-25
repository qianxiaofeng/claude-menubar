#!/bin/bash
# Install claude-bar: build Rust binary + Swift app, start daemon, register hook.
# Usage: ./install.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# 1. Build Rust binary
echo "Building claude-bar (Rust)..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
BINARY="$SCRIPT_DIR/target/release/claude-bar"

if [[ ! -x "$BINARY" ]]; then
    echo "Error: Build failed, binary not found at $BINARY"
    exit 1
fi

# 2. Build Swift menu bar app
echo "Building claude-bar-app (Swift)..."
swiftc -O -o "$SCRIPT_DIR/target/release/claude-bar-app" "$SCRIPT_DIR/swift/ClaudeBar.swift"
APP_BINARY="$SCRIPT_DIR/target/release/claude-bar-app"

if [[ ! -x "$APP_BINARY" ]]; then
    echo "Error: Swift build failed"
    exit 1
fi

# 3. Daemon (launchd plist) — runs the Swift menu bar app
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
        <string>$APP_BINARY</string>
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

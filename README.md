# Claude Code SwiftBar Status

A [SwiftBar](https://github.com/swiftbar/SwiftBar) plugin that shows the status of your running Claude Code sessions in the macOS menu bar. Each session gets its own independent menu bar icon that auto-hides when the session ends.

![Demo](doc/demo.gif)

## Status Indicators

| Icon | Color | Meaning |
|:----:|:-----:|---------|
| ↯ | Green | Session is actively running |
| △ | Orange | Session is waiting for tool approval |
| ☽ | Gray | Session is idle |

When no Claude session exists for a slot, its icon is automatically hidden.

## How It Works

The plugin uses a **cache + slot** architecture. A shared cache builder (`ClaudeBar-cache.2s.sh`) runs expensive queries once every 2 seconds, and lightweight slot plugins (`ClaudeBar-1.2s.sh`, `ClaudeBar-2.2s.sh`, ...) read the cached results.

**Cache builder** (`claude-status-cache.sh`):
1. Queries iTerm2 via AppleScript to get all session TTYs in tab order.
2. Finds running `claude` processes and maps each to its TTY.
3. Filters and orders sessions by iTerm2 tab position, with reversed index mapping so menu bar icons match visual tab order.
4. Writes all slot data to `.swiftbar/cache.env` (atomic write).

**Slot plugins** (`ClaudeBar.sh`):
1. Sources `cache.env` and reads `SLOT_N_*` variables for its slot number.
2. Resolves the Claude Code transcript (`.jsonl`) under `~/.claude/projects/`.
3. Determines status by checking transcript age and pending tool use.
4. Outputs SwiftBar-formatted lines with SF Symbols.
5. Clicking the icon runs `focus-iterm.sh` to activate the corresponding iTerm2 tab.

Menu bar icon order matches iTerm2 tab order. Dragging tabs rearranges icons on the next refresh (≤2s).

## Prerequisites

- macOS
- [SwiftBar](https://github.com/swiftbar/SwiftBar)
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI
- [iTerm2](https://iterm2.com/) (for click-to-focus)
- Python 3 (ships with macOS)

## Installation

1. Clone into your SwiftBar plugins directory:

   ```sh
   PLUGIN_DIR=$(defaults read com.ameba.SwiftBar PluginDirectory)
   git clone https://github.com/qianxiaofeng/claude-swiftbar-status.git \
     "$PLUGIN_DIR/claude-swiftbar-status"
   ```

2. Run the install script (creates 5 slots by default):

   ```sh
   "$PLUGIN_DIR/claude-swiftbar-status/install.sh"
   ```

   To customize the number of slots:

   ```sh
   "$PLUGIN_DIR/claude-swiftbar-status/install.sh" 3
   ```

3. SwiftBar will pick them up automatically. Icons appear only when Claude sessions are running.

## Uninstallation

```sh
PLUGIN_DIR=$(defaults read com.ameba.SwiftBar PluginDirectory)
"$PLUGIN_DIR/claude-swiftbar-status/uninstall.sh"
```

## Files

- **`src/claude-status-cache.sh`** - Cache builder: runs AppleScript/pgrep/lsof once per cycle
- **`src/ClaudeBar.sh`** - Slot plugin: reads cache and renders status icon
- **`src/focus-iterm.sh`** - Helper that focuses the iTerm2 tab for a given TTY
- **`src/resolve_transcript.py`** - Resolves which transcript belongs to which session
- **`src/session-track.sh`** - SessionStart hook: records TTY→transcript mapping
- **`install.sh`** - Creates cache + slot symlinks in the SwiftBar plugins directory
- **`uninstall.sh`** - Removes all ClaudeBar symlinks

## License

MIT

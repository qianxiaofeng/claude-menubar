# Claude Code SwiftBar Status

A [SwiftBar](https://github.com/swiftbar/SwiftBar) plugin that shows the status of your running Claude Code sessions in the macOS menu bar. Each session gets its own independent menu bar icon that auto-hides when the session ends.

## Status Indicators

| Icon | Color | Meaning |
|:----:|:-----:|---------|
| ↯ | Green | Session is actively running |
| △ | Orange | Session is waiting for tool approval |
| ☽ | Gray | Session is idle |

When no Claude session exists for a slot, its icon is automatically hidden.

## How It Works

Each slot is a symlink (`ClaudeBar-1.2s.sh`, `ClaudeBar-2.2s.sh`, ...) pointing to the same `ClaudeBar.sh` script. The script extracts its slot number from the filename and monitors only the Nth Claude process (sorted by PID). When no process exists for that slot, the script outputs nothing and SwiftBar hides the icon.

1. Finds running `claude` processes via `pgrep`, sorted by PID.
2. Each slot picks the Nth process and maps it to its TTY and working directory.
3. Locates the Claude Code transcript (`.jsonl`) under `~/.claude/projects/`.
4. Determines status by checking transcript age and pending tool use.
5. Outputs SwiftBar-formatted lines with SF Symbols.
6. Clicking the icon runs `focus-iterm.sh` to activate the corresponding iTerm2 tab.

## Prerequisites

- macOS
- [SwiftBar](https://github.com/swiftbar/SwiftBar)
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI
- [iTerm2](https://iterm2.com/) (for click-to-focus)
- Python 3 (ships with macOS)

## Installation

1. Clone into your SwiftBar plugins directory with a `.` prefix (so SwiftBar ignores the repo files):

   ```sh
   PLUGIN_DIR=$(defaults read com.ameba.SwiftBar PluginDirectory)
   git clone https://github.com/qianxiaofeng/claude-swiftbar-status.git \
     "$PLUGIN_DIR/.claude-swiftbar-status"
   ```

2. Run the install script (creates 5 slots by default):

   ```sh
   "$PLUGIN_DIR/.claude-swiftbar-status/install.sh"
   ```

   To customize the number of slots:

   ```sh
   "$PLUGIN_DIR/.claude-swiftbar-status/install.sh" 3
   ```

3. SwiftBar will pick them up automatically. Icons appear only when Claude sessions are running.

## Uninstallation

```sh
PLUGIN_DIR=$(defaults read com.ameba.SwiftBar PluginDirectory)
"$PLUGIN_DIR/.claude-swiftbar-status/uninstall.sh"
```

## Files

- **`ClaudeBar.sh`** - Main SwiftBar plugin script
- **`focus-iterm.sh`** - Helper that focuses the iTerm2 tab for a given TTY
- **`install.sh`** - Creates symlinks in the SwiftBar plugins directory
- **`uninstall.sh`** - Removes all ClaudeBar symlinks

## License

MIT

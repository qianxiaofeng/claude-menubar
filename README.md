# Claude Bar

A native macOS menu bar app that shows the status of your running Claude Code sessions. Uses SF Symbols to display session status at a glance — the icon auto-hides when no sessions are running.

![demo](doc/example.gif)

## Status Indicators

The menu bar shows a `cpu.fill` SF Symbol per session, colored by status. Pending/idle icons have a breathing animation to draw attention.

| Color | Meaning |
|:-----:|---------|
| Green | Session is actively running |
| Orange | Session is waiting for user action (tool approval / plan review) |
| Gray | Session is idle |

Click the icon to see a dropdown with project names and status. Click any project to focus its terminal window.

## How It Works

Two binaries work together:

- **`claude-bar poll`** (Rust) — single-shot polling. Discovers running `claude` processes via `pgrep`/`ps`/`lsof`, detects terminals (iTerm2 + Alacritty), reads Claude Code transcripts to determine status, and outputs `[SessionInfo]` JSON to stdout.

- **`claude-bar-app`** (Swift) — native `NSStatusItem` menu bar app, managed by launchd. Calls `claude-bar poll` every 2 seconds and renders SF Symbol icons + dropdown menu.

- **`claude-bar hook`** — registered as a Claude Code `SessionStart` hook. Reads session JSON from stdin and writes a state file for reliable transcript resolution.

- **`claude-bar focus`** — activated on dropdown click. Brings the corresponding terminal window/tab to the foreground via AppleScript (iTerm2) or window matching (Alacritty).

Sessions are auto-discovered — no manual configuration needed.

## Prerequisites

- macOS
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI
- [Rust toolchain](https://rustup.rs/) (for building)
- Xcode Command Line Tools (for `swiftc`)
- iTerm2 and/or Alacritty (also works under Zellij/tmux)

## Installation

1. Clone the repo:

   ```sh
   git clone https://github.com/qianxiaofeng/claude-menubar.git
   cd claude-menubar
   ```

2. Run the install script:

   ```sh
   ./install.sh
   ```

   This will:
   - Build the Rust binary (`cargo build --release`)
   - Build the Swift menu bar app (`swiftc`)
   - Start a launchd daemon (`com.claude.swiftbar-daemon`)
   - Register a `SessionStart` hook in `~/.claude/settings.json`

3. The icon appears automatically when Claude Code sessions are running.

## Uninstallation

```sh
./uninstall.sh
```

This stops the daemon, removes hooks from settings, and cleans up state files.

## Architecture

```
claude-bar-app  (Swift, launchd-managed NSStatusItem)
    │
    ├── NSTimer (every 2s) → shell out to claude-bar poll
    ├── SF Symbol composition → menu bar icon
    └── NSMenu → per-session dropdown → claude-bar focus
         │
claude-bar poll  (Rust, single-shot)
    │
    ├── pgrep/ps/lsof → discover claude processes + TTYs
    ├── iTerm2 AppleScript / Alacritty lsof → detect terminals
    └── ~/.claude/projects/*/*.jsonl → parse transcripts, detect status

claude-bar hook  (Rust, SessionStart hook, stdin JSON)
    └── write state file for transcript resolution
```

### Source Modules

| Module | Purpose |
|--------|---------|
| `src/main.rs` | CLI entry point (clap subcommands: poll, hook, focus) |
| `src/serve.rs` | `poll_sessions()` + `find_state_dir()` |
| `src/process.rs` | Process discovery (pgrep/ps/lsof parsing) |
| `src/transcript.rs` | JSONL transcript parsing + status detection |
| `src/terminal.rs` | iTerm2 AppleScript + Alacritty enumeration |
| `src/state.rs` | Data structures (SessionInfo, Status, Terminal) |
| `src/hook.rs` | SessionStart hook handler |
| `src/focus.rs` | Terminal window focus |
| `src/icon.rs` | Dot-grid PNG generation (test-only, legacy) |
| `build.rs` | Pre-generate PNG combinations (legacy, kept for icon.rs tests) |
| `swift/ClaudeBar.swift` | Native AppKit menu bar app |

## License

MIT

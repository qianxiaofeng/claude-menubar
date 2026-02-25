mod display;
mod focus;
mod hook;
mod icon;
mod process;
mod serve;
mod state;
mod terminal;
mod transcript;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "claude-bar", about = "Claude Code session status for SwiftBar")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the daemon: poll sessions every 2s, serve state via Unix socket
    Serve,
    /// SwiftBar plugin: connect to daemon, render dot grid + dropdown
    Display,
    /// SessionStart hook: read stdin JSON, write session state file
    Hook,
    /// Focus a terminal window
    Focus {
        /// Terminal type: iterm2 or alacritty
        #[arg(long)]
        terminal: String,
        /// TTY device path (e.g. /dev/ttys000)
        #[arg(long, default_value = "")]
        tty: String,
        /// Working directory (used for Alacritty window matching)
        #[arg(long, default_value = "")]
        cwd: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Serve => serve::run_serve(),
        Commands::Display => display::run_display(),
        Commands::Hook => hook::run_hook(),
        Commands::Focus { terminal, tty, cwd } => focus::run_focus(&terminal, &tty, &cwd),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

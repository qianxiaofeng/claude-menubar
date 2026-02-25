mod focus;
mod hook;
#[cfg(test)]
mod icon;
mod process;
mod serve;
mod state;
mod terminal;
mod transcript;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "claude-bar", about = "Claude Code session status for macOS menu bar")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Poll sessions once and output JSON to stdout
    Poll,
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
        Commands::Poll => run_poll(),
        Commands::Hook => hook::run_hook(),
        Commands::Focus { terminal, tty, cwd } => focus::run_focus(&terminal, &tty, &cwd),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run_poll() -> Result<(), Box<dyn std::error::Error>> {
    let sessions = serve::poll_sessions();
    let json = serde_json::to_string(&sessions)?;
    println!("{}", json);
    Ok(())
}

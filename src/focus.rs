use std::process::Command;

/// Focus the iTerm2 window/tab that owns the given TTY.
pub fn focus_iterm2(tty: &str) -> Result<(), Box<dyn std::error::Error>> {
    let script = format!(
        r#"tell application "iTerm2"
    activate
    repeat with w in windows
        tell w
            repeat with t in tabs
                repeat with s in sessions of t
                    if tty of s is "{tty}" then
                        select t
                        set index of w to 1
                        return
                    end if
                end repeat
            end repeat
        end tell
    end repeat
end tell"#
    );

    Command::new("osascript").arg("-e").arg(&script).output()?;
    Ok(())
}

/// Focus the Alacritty window whose title contains the given CWD.
/// Uses System Events accessibility to raise the window.
pub fn focus_alacritty(cwd: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir_name = std::path::Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let script = format!(
        r#"tell application "Alacritty" to activate
tell application "System Events"
    tell process "Alacritty"
        set frontmost to true
        repeat with w in windows
            if name of w contains "{dir_name}" then
                perform action "AXRaise" of w
                return
            end if
        end repeat
    end tell
end tell"#
    );

    Command::new("osascript").arg("-e").arg(&script).output()?;
    Ok(())
}

/// Focus the terminal window for the given session.
pub fn run_focus(
    terminal: &str,
    tty: &str,
    cwd: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match terminal {
        "iterm2" => focus_iterm2(tty),
        "alacritty" => focus_alacritty(cwd),
        "unknown" => Ok(()),
        other => Err(format!("Unknown terminal: {}", other).into()),
    }
}

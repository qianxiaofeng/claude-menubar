use crate::icon;
use crate::state::{DisplayResponse, SessionInfo, Status};
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".claude/swiftbar.sock")
}

/// Run the display subcommand: connect to daemon, render SwiftBar output.
pub fn run_display() -> Result<(), Box<dyn std::error::Error>> {
    let resp = match fetch_state() {
        Some(r) => r,
        None => return Ok(()), // No daemon or empty → hide icon
    };

    if resp.sessions.is_empty() {
        return Ok(()); // Empty output → icon hidden
    }

    let output = render_output(&resp.sessions);
    print!("{}", output);
    Ok(())
}

/// Connect to daemon socket and fetch current state.
fn fetch_state() -> Option<DisplayResponse> {
    let sock = socket_path();
    let mut stream = UnixStream::connect(&sock).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;

    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;

    serde_json::from_str(buf.trim()).ok()
}

/// Render SwiftBar output for the given sessions.
pub fn render_output(sessions: &[SessionInfo]) -> String {
    if sessions.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    // Get the binary path for click actions
    let binary = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "claude-bar".into());

    // Menu bar: dot grid icon
    let statuses: Vec<Status> = sessions.iter().map(|s| s.status).collect();
    let b64 = icon::get_dot_grid_base64(&statuses);
    out.push_str(&format!("| image={}\n", b64));

    // Dropdown separator
    out.push_str("---\n");

    // One entry per session
    for session in sessions {
        let project = std::path::Path::new(&session.cwd)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Claude".into());

        let (sfimage, sfconfig) = status_sf_icon(session.status);
        let tty = &session.tty;
        let terminal = &session.terminal;
        let cwd = &session.cwd;

        // Main row: project name, click to focus
        out.push_str(&format!(
            "{project} | sfimage={sfimage} sfconfig={sfconfig} \
             bash={binary} param1=focus param2=--terminal param3={terminal} \
             param4=--tty param5={tty} param6=--cwd param7={cwd} terminal=false\n"
        ));

        // Status sub-row
        let label = session.status.label();
        out.push_str(&format!(
            "--{label} | sfimage={sfimage} sfconfig={sfconfig} size=12\n"
        ));
    }

    out
}

/// Get SF Symbol name and sfconfig base64 for a status.
fn status_sf_icon(status: Status) -> (&'static str, &'static str) {
    match status {
        Status::Active => (
            "bolt.fill",
            "eyJyZW5kZXJpbmdNb2RlIjoiUGFsZXR0ZSIsImNvbG9ycyI6WyIjMzJENzRCIl19",
        ),
        Status::Pending => (
            "exclamationmark.triangle.fill",
            "eyJyZW5kZXJpbmdNb2RlIjoiUGFsZXR0ZSIsImNvbG9ycyI6WyIjRkY5RjBBIl19",
        ),
        Status::Idle => (
            "moon.fill",
            "eyJyZW5kZXJpbmdNb2RlIjoiUGFsZXR0ZSIsImNvbG9ycyI6WyIjOEU4RTkzIl19",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Terminal;

    fn make_session(tty: &str, pid: u32, cwd: &str, terminal: Terminal, status: Status) -> SessionInfo {
        SessionInfo {
            tty: tty.into(),
            pid,
            cwd: cwd.into(),
            terminal,
            transcript: None,
            status,
        }
    }

    #[test]
    fn test_render_output_has_image() {
        let sessions = vec![make_session(
            "/dev/ttys000", 100, "/Users/test/project", Terminal::ITerm2, Status::Active,
        )];
        let output = render_output(&sessions);
        assert!(output.contains("image="), "Output should contain image=");
        assert!(output.contains("---"), "Output should contain separator");
    }

    #[test]
    fn test_render_no_sessions_empty() {
        let output = render_output(&[]);
        assert!(output.is_empty());
    }

    #[test]
    fn test_render_dropdown_format() {
        let sessions = vec![
            make_session("/dev/ttys000", 100, "/Users/test/myapp", Terminal::ITerm2, Status::Active),
        ];
        let output = render_output(&sessions);
        assert!(output.contains("myapp"), "Should show project name");
        assert!(output.contains("--Running"), "Should show status label");
        assert!(output.contains("bolt.fill"), "Should use bolt.fill for active");
    }

    #[test]
    fn test_render_pending_status() {
        let sessions = vec![
            make_session("/dev/ttys000", 100, "/Users/test/proj", Terminal::ITerm2, Status::Pending),
        ];
        let output = render_output(&sessions);
        assert!(output.contains("--Needs input"), "Should show pending label");
        assert!(output.contains("exclamationmark.triangle.fill"));
    }

    #[test]
    fn test_render_idle_status() {
        let sessions = vec![
            make_session("/dev/ttys000", 100, "/Users/test/proj", Terminal::ITerm2, Status::Idle),
        ];
        let output = render_output(&sessions);
        assert!(output.contains("--Idle"), "Should show idle label");
        assert!(output.contains("moon.fill"));
    }

    #[test]
    fn test_render_mixed_terminals() {
        let sessions = vec![
            make_session("/dev/ttys000", 100, "/Users/test/a", Terminal::ITerm2, Status::Active),
            make_session("/dev/ttys001", 200, "/Users/test/b", Terminal::Alacritty, Status::Idle),
        ];
        let output = render_output(&sessions);

        // Both projects should appear
        assert!(output.contains("a |"), "Should show project a");
        assert!(output.contains("b |"), "Should show project b");

        // Check terminal params
        assert!(output.contains("param3=iterm2"), "Should have iterm2 param");
        assert!(output.contains("param3=alacritty"), "Should have alacritty param");
    }

    #[test]
    fn test_render_multiple_sessions_single_icon() {
        let sessions = vec![
            make_session("/dev/ttys000", 100, "/a", Terminal::ITerm2, Status::Active),
            make_session("/dev/ttys001", 200, "/b", Terminal::ITerm2, Status::Pending),
            make_session("/dev/ttys002", 300, "/c", Terminal::Alacritty, Status::Idle),
        ];
        let output = render_output(&sessions);

        // The menu bar line uses "image=" (PNG), dropdown entries use "sfimage="
        // Only one line should start with "| image="
        let menu_bar_lines: Vec<&str> = output
            .lines()
            .filter(|l| l.starts_with("| image="))
            .collect();
        assert_eq!(menu_bar_lines.len(), 1, "Should have exactly one menu bar icon");

        // Should have three dropdown entries
        let separator_pos = output.find("---").unwrap();
        let dropdown = &output[separator_pos..];
        // Count lines with sfimage (main entries, not sub-entries)
        let entries: Vec<&str> = dropdown
            .lines()
            .filter(|l| !l.starts_with("--") && l.contains("sfimage="))
            .collect();
        assert_eq!(entries.len(), 3, "Should have 3 dropdown entries");
    }

    #[test]
    fn test_status_sf_icon_values() {
        let (img, cfg) = status_sf_icon(Status::Active);
        assert_eq!(img, "bolt.fill");
        assert!(!cfg.is_empty());

        let (img, _) = status_sf_icon(Status::Pending);
        assert_eq!(img, "exclamationmark.triangle.fill");

        let (img, _) = status_sf_icon(Status::Idle);
        assert_eq!(img, "moon.fill");
    }
}

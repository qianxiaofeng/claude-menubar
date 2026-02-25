use crate::process;
use crate::state::Terminal;
use std::collections::HashMap;
use std::process::Command;

const ITERM2_APPLESCRIPT: &str = r#"
tell application "iTerm2"
    set out to ""
    repeat with w in windows
        tell w
            repeat with t in tabs
                repeat with s in sessions of t
                    set out to out & (tty of s) & linefeed
                end repeat
            end repeat
        end tell
    end repeat
    return out
end tell
"#;

/// Enumerate all iTerm2 session TTYs in tab order via AppleScript.
pub fn enumerate_iterm2_ttys() -> Vec<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(ITERM2_APPLESCRIPT)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    parse_iterm2_output(&output)
}

/// Parse AppleScript output (one TTY per line) into a list of TTY paths.
pub fn parse_iterm2_output(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("/dev/ttys"))
        .collect()
}

/// Enumerate all Alacritty session TTYs via lsof.
pub fn enumerate_alacritty_ttys() -> Vec<String> {
    let output = Command::new("lsof")
        .args(["-c", "alacritty"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    process::parse_lsof_ttys(&output)
}

/// Merge sessions from iTerm2 and Alacritty.
/// iTerm2 sessions come first (preserving tab order), then Alacritty (sorted by TTY).
/// Only TTYs that have a running Claude process (present in pid_by_tty) are included.
/// If the same TTY appears in both, iTerm2 takes priority.
pub fn merge_sessions(
    iterm2_ttys: &[String],
    alacritty_ttys: &[String],
    pid_by_tty: &HashMap<String, u32>,
) -> Vec<(String, Terminal)> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // iTerm2 first, preserving tab order
    for tty in iterm2_ttys {
        if pid_by_tty.contains_key(tty) && seen.insert(tty.clone()) {
            result.push((tty.clone(), Terminal::ITerm2));
        }
    }

    // Alacritty second, sorted by TTY
    let mut alacritty: Vec<_> = alacritty_ttys
        .iter()
        .filter(|tty| pid_by_tty.contains_key(*tty) && !seen.contains(*tty))
        .cloned()
        .collect();
    alacritty.sort();

    for tty in alacritty {
        seen.insert(tty.clone());
        result.push((tty, Terminal::Alacritty));
    }

    // Fallback: TTYs in pid_by_tty not claimed by any terminal (e.g. tmux/zellij PTYs)
    let mut unclaimed: Vec<_> = pid_by_tty
        .keys()
        .filter(|tty| !seen.contains(*tty))
        .cloned()
        .collect();
    unclaimed.sort();

    for tty in unclaimed {
        result.push((tty, Terminal::Unknown));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_iterm2_output() {
        let output = "/dev/ttys000\n/dev/ttys001\n/dev/ttys002\n";
        let ttys = parse_iterm2_output(output);
        assert_eq!(ttys, vec!["/dev/ttys000", "/dev/ttys001", "/dev/ttys002"]);
    }

    #[test]
    fn test_parse_iterm2_output_with_whitespace() {
        let output = "  /dev/ttys000  \n  /dev/ttys001\n\n";
        let ttys = parse_iterm2_output(output);
        assert_eq!(ttys, vec!["/dev/ttys000", "/dev/ttys001"]);
    }

    #[test]
    fn test_parse_iterm2_output_empty() {
        assert_eq!(parse_iterm2_output(""), Vec::<String>::new());
        assert_eq!(parse_iterm2_output("\n\n"), Vec::<String>::new());
    }

    #[test]
    fn test_merge_iterm2_only() {
        let iterm = vec!["/dev/ttys000".into(), "/dev/ttys001".into()];
        let alacritty: Vec<String> = vec![];
        let mut pid_by_tty = HashMap::new();
        pid_by_tty.insert("/dev/ttys000".into(), 100);
        pid_by_tty.insert("/dev/ttys001".into(), 200);

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        assert_eq!(
            result,
            vec![
                ("/dev/ttys000".into(), Terminal::ITerm2),
                ("/dev/ttys001".into(), Terminal::ITerm2),
            ]
        );
    }

    #[test]
    fn test_merge_alacritty_only() {
        let iterm: Vec<String> = vec![];
        let alacritty = vec!["/dev/ttys003".into(), "/dev/ttys001".into()];
        let mut pid_by_tty = HashMap::new();
        pid_by_tty.insert("/dev/ttys001".into(), 100);
        pid_by_tty.insert("/dev/ttys003".into(), 200);

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        // Alacritty sorted by TTY
        assert_eq!(
            result,
            vec![
                ("/dev/ttys001".into(), Terminal::Alacritty),
                ("/dev/ttys003".into(), Terminal::Alacritty),
            ]
        );
    }

    #[test]
    fn test_merge_mixed() {
        let iterm = vec!["/dev/ttys000".into(), "/dev/ttys002".into()];
        let alacritty = vec!["/dev/ttys003".into(), "/dev/ttys001".into()];
        let mut pid_by_tty = HashMap::new();
        pid_by_tty.insert("/dev/ttys000".into(), 100);
        pid_by_tty.insert("/dev/ttys001".into(), 200);
        pid_by_tty.insert("/dev/ttys002".into(), 300);
        pid_by_tty.insert("/dev/ttys003".into(), 400);

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        assert_eq!(
            result,
            vec![
                ("/dev/ttys000".into(), Terminal::ITerm2),
                ("/dev/ttys002".into(), Terminal::ITerm2),
                ("/dev/ttys001".into(), Terminal::Alacritty),
                ("/dev/ttys003".into(), Terminal::Alacritty),
            ]
        );
    }

    #[test]
    fn test_merge_overlapping_tty() {
        // Same TTY in both → iTerm2 wins
        let iterm = vec!["/dev/ttys000".into()];
        let alacritty = vec!["/dev/ttys000".into(), "/dev/ttys001".into()];
        let mut pid_by_tty = HashMap::new();
        pid_by_tty.insert("/dev/ttys000".into(), 100);
        pid_by_tty.insert("/dev/ttys001".into(), 200);

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        assert_eq!(
            result,
            vec![
                ("/dev/ttys000".into(), Terminal::ITerm2),
                ("/dev/ttys001".into(), Terminal::Alacritty),
            ]
        );
    }

    #[test]
    fn test_merge_no_claude() {
        let iterm = vec!["/dev/ttys000".into(), "/dev/ttys001".into()];
        let alacritty = vec!["/dev/ttys002".into()];
        let pid_by_tty = HashMap::new(); // no claude processes

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        assert!(result.is_empty());
    }

    #[test]
    fn test_merge_unclaimed_fallback() {
        // TTYs in pid_by_tty but not in any terminal list → Terminal::Unknown
        let iterm = vec!["/dev/ttys000".into()];
        let alacritty: Vec<String> = vec![];
        let mut pid_by_tty = HashMap::new();
        pid_by_tty.insert("/dev/ttys000".into(), 100);
        pid_by_tty.insert("/dev/ttys003".into(), 300); // zellij PTY
        pid_by_tty.insert("/dev/ttys005".into(), 500); // tmux PTY

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        assert_eq!(
            result,
            vec![
                ("/dev/ttys000".into(), Terminal::ITerm2),
                ("/dev/ttys003".into(), Terminal::Unknown),
                ("/dev/ttys005".into(), Terminal::Unknown),
            ]
        );
    }

    #[test]
    fn test_merge_all_unclaimed() {
        // No terminals detected at all → all sessions are Unknown
        let iterm: Vec<String> = vec![];
        let alacritty: Vec<String> = vec![];
        let mut pid_by_tty = HashMap::new();
        pid_by_tty.insert("/dev/ttys002".into(), 200);
        pid_by_tty.insert("/dev/ttys001".into(), 100);

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        assert_eq!(
            result,
            vec![
                ("/dev/ttys001".into(), Terminal::Unknown),
                ("/dev/ttys002".into(), Terminal::Unknown),
            ]
        );
    }

    #[test]
    fn test_merge_partial_claude() {
        // Only some TTYs have Claude running
        let iterm = vec!["/dev/ttys000".into(), "/dev/ttys001".into(), "/dev/ttys002".into()];
        let alacritty: Vec<String> = vec![];
        let mut pid_by_tty = HashMap::new();
        pid_by_tty.insert("/dev/ttys000".into(), 100);
        pid_by_tty.insert("/dev/ttys002".into(), 300);
        // ttys001 has no Claude

        let result = merge_sessions(&iterm, &alacritty, &pid_by_tty);
        assert_eq!(
            result,
            vec![
                ("/dev/ttys000".into(), Terminal::ITerm2),
                ("/dev/ttys002".into(), Terminal::ITerm2),
            ]
        );
    }
}

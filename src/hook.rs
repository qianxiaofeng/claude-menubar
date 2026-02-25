use crate::process;
use crate::state::SessionState;
use std::fs;
use std::io::Read;
use std::path::Path;

/// Parse the hook JSON input from stdin.
pub fn parse_hook_input(input: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(input).ok()?;
    let session_id = v.get("session_id")?.as_str()?.to_string();
    let transcript_path = v.get("transcript_path")?.as_str()?.to_string();
    Some((session_id, transcript_path))
}

/// Run the hook subcommand: read stdin JSON, find claude ancestor, write state file.
pub fn run_hook() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let (session_id, transcript_path) = parse_hook_input(&input)
        .ok_or("Failed to parse hook JSON from stdin")?;

    // Walk up process tree to find claude and its TTY
    let ppid = std::os::unix::process::parent_id();
    let (_, tty) = process::find_claude_ancestor(ppid)
        .ok_or("Could not find claude process in ancestor chain")?;

    let tty_short = tty.trim_start_matches("/dev/");

    // Determine CWD from the claude process to find the right state dir
    // The state dir is <project>/.swiftbar/
    // We derive it from the transcript path (which is under ~/.claude/projects/<hash>/)
    let cwd = find_project_cwd_from_transcript(&transcript_path);
    let state_dir = if cwd.is_empty() {
        // Fallback: use a default location
        let home = std::env::var("HOME").unwrap_or_default();
        Path::new(&home).join(".claude/swiftbar")
    } else {
        Path::new(&cwd).join(".swiftbar")
    };

    fs::create_dir_all(&state_dir)?;

    let state = SessionState {
        session_id,
        transcript_path,
    };

    let state_file = state_dir.join(format!("session-{}.json", tty_short));
    let json = serde_json::to_string(&state)?;
    fs::write(state_file, json)?;

    Ok(())
}

/// Try to find the project CWD by walking up the process tree and using lsof.
fn find_project_cwd_from_transcript(_transcript_path: &str) -> String {
    // Try to get CWD from our parent claude process
    let ppid = std::os::unix::process::parent_id();
    if let Some((pid, _)) = process::find_claude_ancestor(ppid) {
        if let Some(cwd) = process::get_pid_cwd(pid) {
            return cwd;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process;
    use std::collections::HashMap;

    /// Testable version: find claude in a mock process tree and return the TTY.
    fn find_tty_from_tree(
        start_pid: u32,
        lookup: &HashMap<u32, (String, u32, Option<String>)>,
    ) -> Option<String> {
        process::find_claude_in_tree(start_pid, lookup).map(|(_, tty)| tty)
    }

    #[test]
    fn test_parse_hook_stdin() {
        let input = r#"{"session_id":"abc-123","transcript_path":"/home/user/.claude/projects/test/session.jsonl"}"#;
        let (sid, tp) = parse_hook_input(input).unwrap();
        assert_eq!(sid, "abc-123");
        assert_eq!(tp, "/home/user/.claude/projects/test/session.jsonl");
    }

    #[test]
    fn test_parse_hook_stdin_extra_fields() {
        let input = r#"{"session_id":"x","transcript_path":"/t.jsonl","extra":"ignored"}"#;
        let (sid, tp) = parse_hook_input(input).unwrap();
        assert_eq!(sid, "x");
        assert_eq!(tp, "/t.jsonl");
    }

    #[test]
    fn test_parse_hook_stdin_missing_fields() {
        assert!(parse_hook_input(r#"{"session_id":"x"}"#).is_none());
        assert!(parse_hook_input(r#"{"transcript_path":"x"}"#).is_none());
        assert!(parse_hook_input("not json").is_none());
    }

    #[test]
    fn test_find_tty_from_tree() {
        let mut lookup = HashMap::new();
        lookup.insert(100, ("zsh".to_string(), 50, Some("/dev/ttys000".to_string())));
        lookup.insert(50, ("claude".to_string(), 1, Some("/dev/ttys000".to_string())));

        let tty = find_tty_from_tree(100, &lookup);
        assert_eq!(tty, Some("/dev/ttys000".to_string()));
    }

    #[test]
    fn test_find_tty_from_tree_no_claude() {
        let mut lookup = HashMap::new();
        lookup.insert(100, ("zsh".to_string(), 50, Some("/dev/ttys000".to_string())));
        lookup.insert(50, ("bash".to_string(), 1, Some("/dev/ttys000".to_string())));

        assert_eq!(find_tty_from_tree(100, &lookup), None);
    }

    #[test]
    fn test_state_file_write() {
        let tmp = tempfile::TempDir::new().unwrap();
        let state_dir = tmp.path();

        let state = SessionState {
            session_id: "test-123".into(),
            transcript_path: "/path/to/transcript.jsonl".into(),
        };

        let state_file = state_dir.join("session-ttys000.json");
        let json = serde_json::to_string(&state).unwrap();
        fs::write(&state_file, &json).unwrap();

        let read_back: SessionState =
            serde_json::from_str(&fs::read_to_string(&state_file).unwrap()).unwrap();
        assert_eq!(read_back.session_id, "test-123");
        assert_eq!(read_back.transcript_path, "/path/to/transcript.jsonl");
    }
}

use crate::process;
use crate::state::SessionInfo;
use crate::terminal;
use crate::transcript;
use std::collections::HashSet;
use std::path::Path;

/// Poll all terminal sessions and determine their statuses.
pub fn poll_sessions() -> Vec<SessionInfo> {
    let pid_by_tty = process::build_pid_by_tty();
    let iterm2_ttys = terminal::enumerate_iterm2_ttys();
    let alacritty_ttys = terminal::enumerate_alacritty_ttys();
    let merged = terminal::merge_sessions(&iterm2_ttys, &alacritty_ttys, &pid_by_tty);

    let active_ttys: HashSet<String> = merged
        .iter()
        .map(|(tty, _)| tty.trim_start_matches("/dev/").to_string())
        .collect();

    let home = std::env::var("HOME").unwrap_or_default();

    let mut sessions = Vec::new();
    for (tty, term) in &merged {
        let pid = match pid_by_tty.get(tty) {
            Some(&p) => p,
            None => continue,
        };

        let cwd = process::get_pid_cwd(pid).unwrap_or_default();
        let project_hash = transcript::project_hash(&cwd);
        let tty_short = tty.trim_start_matches("/dev/");

        let project_dir = Path::new(&home)
            .join(".claude/projects")
            .join(&project_hash);

        let state_dir = transcript::state_dir_for_cwd(&cwd);

        let transcript_path = transcript::resolve_transcript(
            tty_short,
            &state_dir,
            &project_dir,
            &active_ttys,
        );

        let transcript_opt = if transcript_path.is_empty() {
            None
        } else {
            Some(transcript_path)
        };

        let status = transcript::determine_status(transcript_opt.as_deref());

        sessions.push(SessionInfo {
            tty: tty.clone(),
            pid,
            cwd,
            terminal: *term,
            transcript: transcript_opt,
            status,
        });
    }

    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_dir_uses_centralized_path() {
        let home = std::env::var("HOME").unwrap_or_default();
        let expected = std::path::PathBuf::from(&home)
            .join(".claude/claude-bar/-Users-test-project");
        assert_eq!(
            transcript::state_dir_for_cwd("/Users/test/project"),
            expected
        );
    }
}

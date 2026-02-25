use crate::process;
use crate::state::SessionInfo;
use crate::terminal;
use crate::transcript;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

        let state_dir = find_state_dir(&cwd);

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

/// Find the .claude-bar state directory for a given project CWD.
fn find_state_dir(cwd: &str) -> PathBuf {
    if cwd.is_empty() {
        return PathBuf::from("/tmp/.claude-bar");
    }
    PathBuf::from(cwd).join(".claude-bar")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_state_dir() {
        assert_eq!(
            find_state_dir("/Users/test/project"),
            PathBuf::from("/Users/test/project/.claude-bar")
        );
        assert_eq!(find_state_dir(""), PathBuf::from("/tmp/.claude-bar"));
    }
}

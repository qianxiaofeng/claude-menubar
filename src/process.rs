use std::collections::HashMap;
use std::process::Command;

/// Parse `pgrep -x claude` output into a list of PIDs.
pub fn parse_pgrep_output(output: &str) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect()
}

/// Parse `ps -o tty= -p PID` output into a TTY device path.
/// Returns None for detached processes (tty = "??").
pub fn parse_ps_tty(output: &str) -> Option<String> {
    let tty = output.trim();
    if tty.is_empty() || tty == "??" {
        return None;
    }
    Some(format!("/dev/{}", tty))
}

/// Parse `lsof` output to extract /dev/ttys* paths.
/// Used for Alacritty TTY enumeration.
pub fn parse_lsof_ttys(output: &str) -> Vec<String> {
    let mut ttys = Vec::new();
    for line in output.lines() {
        for field in line.split_whitespace() {
            if field.starts_with("/dev/ttys") {
                if !ttys.contains(&field.to_string()) {
                    ttys.push(field.to_string());
                }
            }
        }
    }
    ttys
}

/// Parse `lsof -p PID -Fn` output to extract CWD (first `n/` entry after `fcwd`).
pub fn parse_lsof_cwd(output: &str) -> Option<String> {
    let mut found_cwd = false;
    for line in output.lines() {
        if line == "fcwd" {
            found_cwd = true;
            continue;
        }
        if found_cwd && line.starts_with('n') {
            return Some(line[1..].to_string());
        }
        if line.starts_with('f') && line != "fcwd" {
            found_cwd = false;
        }
    }
    None
}

/// Parse `ps -o comm= -p PID` output to get the process name.
pub fn parse_ps_comm(output: &str) -> Option<String> {
    let name = output.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Parse `ps -o ppid= -p PID` output to get the parent PID.
pub fn parse_ps_ppid(output: &str) -> Option<u32> {
    output.trim().parse::<u32>().ok()
}

/// Find all claude PIDs via pgrep.
pub fn find_claude_pids() -> Vec<u32> {
    let output = Command::new("pgrep")
        .args(["-x", "claude"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    parse_pgrep_output(&output)
}

/// Get the TTY for a given PID.
pub fn get_pid_tty(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    parse_ps_tty(&String::from_utf8_lossy(&output.stdout))
}

/// Get CWD for a given PID via lsof.
pub fn get_pid_cwd(pid: u32) -> Option<String> {
    let output = Command::new("lsof")
        .args(["-p", &pid.to_string(), "-Fn"])
        .output()
        .ok()?;
    parse_lsof_cwd(&String::from_utf8_lossy(&output.stdout))
}

/// Build a map of TTY -> PID for all running claude processes.
pub fn build_pid_by_tty() -> HashMap<String, u32> {
    let pids = find_claude_pids();
    let mut map = HashMap::new();
    for pid in pids {
        if let Some(tty) = get_pid_tty(pid) {
            map.insert(tty, pid);
        }
    }
    map
}

/// Walk up the process tree from `start_pid` to find a process named "claude".
/// Returns (pid, tty) if found.
pub fn find_claude_ancestor(start_pid: u32) -> Option<(u32, String)> {
    let mut pid = start_pid;
    loop {
        if pid <= 1 {
            return None;
        }
        // Get process name
        let comm_output = Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let name = parse_ps_comm(&String::from_utf8_lossy(&comm_output.stdout));
        if name.as_deref() == Some("claude") {
            let tty = get_pid_tty(pid)?;
            return Some((pid, tty));
        }
        // Move to parent
        let ppid_output = Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        pid = parse_ps_ppid(&String::from_utf8_lossy(&ppid_output.stdout))?;
    }
}

/// Testable version: walk up process tree using provided output.
/// `lookup` maps PID -> (comm, ppid, tty)
#[cfg(test)]
pub fn find_claude_in_tree(
    start_pid: u32,
    lookup: &HashMap<u32, (String, u32, Option<String>)>,
) -> Option<(u32, String)> {
    let mut pid = start_pid;
    loop {
        if pid <= 1 {
            return None;
        }
        let (comm, ppid, tty) = lookup.get(&pid)?;
        if comm == "claude" {
            return tty.clone().map(|t| (pid, t));
        }
        pid = *ppid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pgrep_output() {
        let output = "12345\n67890\n111\n";
        let pids = parse_pgrep_output(output);
        assert_eq!(pids, vec![12345, 67890, 111]);
    }

    #[test]
    fn test_parse_pgrep_empty() {
        assert_eq!(parse_pgrep_output(""), Vec::<u32>::new());
        assert_eq!(parse_pgrep_output("\n"), Vec::<u32>::new());
    }

    #[test]
    fn test_parse_pgrep_with_whitespace() {
        let output = "  12345  \n  67890\n";
        let pids = parse_pgrep_output(output);
        assert_eq!(pids, vec![12345, 67890]);
    }

    #[test]
    fn test_parse_ps_tty() {
        assert_eq!(
            parse_ps_tty("ttys000\n"),
            Some("/dev/ttys000".to_string())
        );
        assert_eq!(
            parse_ps_tty("  ttys042  \n"),
            Some("/dev/ttys042".to_string())
        );
    }

    #[test]
    fn test_parse_ps_tty_question_mark() {
        assert_eq!(parse_ps_tty("??\n"), None);
        assert_eq!(parse_ps_tty("  ??  "), None);
    }

    #[test]
    fn test_parse_ps_tty_empty() {
        assert_eq!(parse_ps_tty(""), None);
        assert_eq!(parse_ps_tty("  \n  "), None);
    }

    #[test]
    fn test_parse_lsof_ttys() {
        let output = "alacri  1234 user    0u  CHR  16,3  0t0  /dev/ttys003\n\
                       alacri  1234 user    1u  CHR  16,5  0t0  /dev/ttys005\n\
                       alacri  1234 user    2u  CHR  16,3  0t0  /dev/ttys003\n";
        let ttys = parse_lsof_ttys(output);
        assert_eq!(ttys, vec!["/dev/ttys003", "/dev/ttys005"]);
    }

    #[test]
    fn test_parse_lsof_ttys_empty() {
        assert_eq!(parse_lsof_ttys(""), Vec::<String>::new());
    }

    #[test]
    fn test_parse_lsof_cwd() {
        let output = "p12345\nfcwd\nn/Users/test/project\nftxt\nn/usr/bin/claude\n";
        assert_eq!(
            parse_lsof_cwd(output),
            Some("/Users/test/project".to_string())
        );
    }

    #[test]
    fn test_parse_lsof_cwd_no_cwd() {
        let output = "p12345\nftxt\nn/usr/bin/claude\n";
        assert_eq!(parse_lsof_cwd(output), None);
    }

    #[test]
    fn test_parse_lsof_cwd_empty() {
        assert_eq!(parse_lsof_cwd(""), None);
    }

    #[test]
    fn test_parse_ps_comm() {
        assert_eq!(parse_ps_comm("claude\n"), Some("claude".to_string()));
        assert_eq!(parse_ps_comm("  zsh  "), Some("zsh".to_string()));
        assert_eq!(parse_ps_comm(""), None);
        assert_eq!(parse_ps_comm("  \n  "), None);
    }

    #[test]
    fn test_parse_ps_ppid() {
        assert_eq!(parse_ps_ppid("  12345\n"), Some(12345));
        assert_eq!(parse_ps_ppid("not_a_number"), None);
        assert_eq!(parse_ps_ppid(""), None);
    }

    #[test]
    fn test_find_claude_in_tree() {
        let mut lookup = HashMap::new();
        // PID 100: shell (child of 50)
        lookup.insert(100, ("zsh".to_string(), 50, Some("/dev/ttys000".to_string())));
        // PID 50: claude (child of 1)
        lookup.insert(
            50,
            (
                "claude".to_string(),
                1,
                Some("/dev/ttys000".to_string()),
            ),
        );

        let result = find_claude_in_tree(100, &lookup);
        assert_eq!(result, Some((50, "/dev/ttys000".to_string())));
    }

    #[test]
    fn test_find_claude_in_tree_no_claude() {
        let mut lookup = HashMap::new();
        lookup.insert(100, ("zsh".to_string(), 50, Some("/dev/ttys000".to_string())));
        lookup.insert(50, ("bash".to_string(), 1, Some("/dev/ttys000".to_string())));

        assert_eq!(find_claude_in_tree(100, &lookup), None);
    }

    #[test]
    fn test_find_claude_in_tree_detached() {
        let mut lookup = HashMap::new();
        lookup.insert(100, ("zsh".to_string(), 50, None));
        lookup.insert(50, ("claude".to_string(), 1, None));

        // claude has no TTY
        assert_eq!(find_claude_in_tree(100, &lookup), None);
    }

    #[test]
    fn test_find_claude_in_tree_deep() {
        let mut lookup = HashMap::new();
        lookup.insert(200, ("python3".to_string(), 150, Some("/dev/ttys001".to_string())));
        lookup.insert(150, ("zsh".to_string(), 100, Some("/dev/ttys001".to_string())));
        lookup.insert(100, ("node".to_string(), 50, Some("/dev/ttys001".to_string())));
        lookup.insert(50, ("claude".to_string(), 1, Some("/dev/ttys001".to_string())));

        let result = find_claude_in_tree(200, &lookup);
        assert_eq!(result, Some((50, "/dev/ttys001".to_string())));
    }
}

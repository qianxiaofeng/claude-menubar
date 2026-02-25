use crate::process;
use crate::state::{DisplayResponse, SessionInfo};
use crate::terminal;
use crate::transcript;
use std::collections::HashSet;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".claude/swiftbar.sock")
}

/// Global flag for graceful shutdown.
static RUNNING: AtomicBool = AtomicBool::new(true);

/// Run the daemon: poll sessions every 2s, serve state via Unix socket.
pub fn run_serve() -> Result<(), Box<dyn std::error::Error>> {
    let sock_path = socket_path();

    // Clean up stale socket
    if sock_path.exists() {
        let _ = std::fs::remove_file(&sock_path);
    }

    // Ensure parent directory exists
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&sock_path)?;
    listener.set_nonblocking(true)?;

    let state: Arc<Mutex<DisplayResponse>> = Arc::new(Mutex::new(DisplayResponse {
        sessions: Vec::new(),
    }));

    // Spawn socket listener thread
    let state_clone = state.clone();
    let _listener_handle = std::thread::spawn(move || {
        while RUNNING.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let resp = state_clone.lock().unwrap().clone();
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    let _ = stream.write_all(json.as_bytes());
                    let _ = stream.write_all(b"\n");
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    });

    // Main poll loop
    while RUNNING.load(Ordering::Relaxed) {
        let sessions = poll_sessions();
        {
            let mut locked = state.lock().unwrap();
            locked.sessions = sessions;
        }
        // Sleep in small increments so we can respond to shutdown quickly
        for _ in 0..20 {
            if !RUNNING.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    let _ = std::fs::remove_file(&sock_path);
    Ok(())
}

/// Poll all terminal sessions and determine their statuses.
fn poll_sessions() -> Vec<SessionInfo> {
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

/// Find the .swiftbar state directory for a given project CWD.
fn find_state_dir(cwd: &str) -> PathBuf {
    if cwd.is_empty() {
        return PathBuf::from("/tmp/.swiftbar");
    }
    PathBuf::from(cwd).join(".swiftbar")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Status;
    use std::io::Read;
    use std::os::unix::net::UnixStream;

    #[test]
    fn test_socket_responds_json() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let state = Arc::new(Mutex::new(DisplayResponse {
            sessions: vec![SessionInfo {
                tty: "/dev/ttys000".into(),
                pid: 123,
                cwd: "/test".into(),
                terminal: crate::state::Terminal::ITerm2,
                transcript: None,
                status: Status::Active,
            }],
        }));

        let listener = UnixListener::bind(&sock_path).unwrap();

        let state_clone = state.clone();
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let resp = state_clone.lock().unwrap().clone();
                let json = serde_json::to_string(&resp).unwrap();
                let _ = stream.write_all(json.as_bytes());
                let _ = stream.write_all(b"\n");
                let _ = stream.flush();
            }
        });

        std::thread::sleep(Duration::from_millis(50));

        let mut stream = UnixStream::connect(&sock_path).unwrap();
        let mut buf = String::new();
        stream.read_to_string(&mut buf).unwrap();

        let resp: DisplayResponse = serde_json::from_str(buf.trim()).unwrap();
        assert_eq!(resp.sessions.len(), 1);
        assert_eq!(resp.sessions[0].pid, 123);
        assert_eq!(resp.sessions[0].status, Status::Active);

        handle.join().unwrap();
    }

    #[test]
    fn test_socket_cleanup() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sock_path = tmp.path().join("cleanup.sock");

        let listener = UnixListener::bind(&sock_path).unwrap();
        assert!(sock_path.exists());

        drop(listener);
        let _ = std::fs::remove_file(&sock_path);
        assert!(!sock_path.exists());
    }

    #[test]
    fn test_concurrent_clients() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sock_path = tmp.path().join("concurrent.sock");

        let state = Arc::new(Mutex::new(DisplayResponse {
            sessions: vec![SessionInfo {
                tty: "/dev/ttys000".into(),
                pid: 456,
                cwd: "/test".into(),
                terminal: crate::state::Terminal::ITerm2,
                transcript: None,
                status: Status::Idle,
            }],
        }));

        let listener = UnixListener::bind(&sock_path).unwrap();

        let state_clone = state.clone();
        let handle = std::thread::spawn(move || {
            for _ in 0..3 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let resp = state_clone.lock().unwrap().clone();
                    let json = serde_json::to_string(&resp).unwrap();
                    let _ = stream.write_all(json.as_bytes());
                    let _ = stream.write_all(b"\n");
                    let _ = stream.flush();
                }
            }
        });

        std::thread::sleep(Duration::from_millis(50));

        let mut handles = Vec::new();
        let sp = sock_path.clone();
        for _ in 0..3 {
            let path = sp.clone();
            handles.push(std::thread::spawn(move || {
                let mut stream = UnixStream::connect(&path).unwrap();
                let mut buf = String::new();
                stream.read_to_string(&mut buf).unwrap();
                let resp: DisplayResponse = serde_json::from_str(buf.trim()).unwrap();
                assert_eq!(resp.sessions[0].pid, 456);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
        handle.join().unwrap();
    }

    #[test]
    fn test_find_state_dir() {
        assert_eq!(
            find_state_dir("/Users/test/project"),
            PathBuf::from("/Users/test/project/.swiftbar")
        );
        assert_eq!(find_state_dir(""), PathBuf::from("/tmp/.swiftbar"));
    }
}

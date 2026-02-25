use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Active,
    Pending,
    Idle,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Active => write!(f, "active"),
            Status::Pending => write!(f, "pending"),
            Status::Idle => write!(f, "idle"),
        }
    }
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Active => "Running",
            Status::Pending => "Needs input",
            Status::Idle => "Idle",
        }
    }

    pub fn index(&self) -> u8 {
        match self {
            Status::Active => 0,
            Status::Pending => 1,
            Status::Idle => 2,
        }
    }

    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(Status::Active),
            1 => Some(Status::Pending),
            2 => Some(Status::Idle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Terminal {
    ITerm2,
    Alacritty,
}

impl fmt::Display for Terminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminal::ITerm2 => write!(f, "iterm2"),
            Terminal::Alacritty => write!(f, "alacritty"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub tty: String,
    pub pid: u32,
    pub cwd: String,
    pub terminal: Terminal,
    pub transcript: Option<String>,
    pub status: Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayResponse {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub transcript_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_serialize_deserialize() {
        let s = Status::Active;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"active\"");
        let back: Status = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Status::Active);
    }

    #[test]
    fn test_session_info_roundtrip() {
        let info = SessionInfo {
            tty: "/dev/ttys000".into(),
            pid: 12345,
            cwd: "/Users/test/project".into(),
            terminal: Terminal::ITerm2,
            transcript: Some("/path/to/transcript.jsonl".into()),
            status: Status::Active,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tty, info.tty);
        assert_eq!(back.pid, info.pid);
        assert_eq!(back.status, Status::Active);
        assert_eq!(back.terminal, Terminal::ITerm2);
    }

    #[test]
    fn test_display_response_roundtrip() {
        let resp = DisplayResponse {
            sessions: vec![
                SessionInfo {
                    tty: "/dev/ttys000".into(),
                    pid: 100,
                    cwd: "/a".into(),
                    terminal: Terminal::ITerm2,
                    transcript: None,
                    status: Status::Active,
                },
                SessionInfo {
                    tty: "/dev/ttys001".into(),
                    pid: 200,
                    cwd: "/b".into(),
                    terminal: Terminal::Alacritty,
                    transcript: Some("/t.jsonl".into()),
                    status: Status::Idle,
                },
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: DisplayResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sessions.len(), 2);
        assert_eq!(back.sessions[0].status, Status::Active);
        assert_eq!(back.sessions[1].terminal, Terminal::Alacritty);
    }

    #[test]
    fn test_session_state_roundtrip() {
        let state = SessionState {
            session_id: "abc-123".into(),
            transcript_path: "/path/to/transcript.jsonl".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "abc-123");
        assert_eq!(back.transcript_path, state.transcript_path);
    }

    #[test]
    fn test_status_index_roundtrip() {
        for s in [Status::Active, Status::Pending, Status::Idle] {
            assert_eq!(Status::from_index(s.index()), Some(s));
        }
        assert_eq!(Status::from_index(3), None);
    }

    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", Status::Active), "active");
        assert_eq!(format!("{}", Status::Pending), "pending");
        assert_eq!(format!("{}", Status::Idle), "idle");
    }

    #[test]
    fn test_terminal_display() {
        assert_eq!(format!("{}", Terminal::ITerm2), "iterm2");
        assert_eq!(format!("{}", Terminal::Alacritty), "alacritty");
    }
}

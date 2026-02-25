use crate::state::Status;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

/// Parse the tail of a transcript JSONL file.
/// Returns (last_role, has_pending_tool).
///
/// last_role: "user" | "assistant" | None
/// has_pending_tool: true if last assistant message has unpaired tool_use
pub fn parse_transcript_tail(path: &str) -> (Option<String>, bool) {
    if path.is_empty() {
        return (None, false);
    }

    let content = match read_tail(path, 65536) {
        Some(c) => c,
        None => return (None, false),
    };

    parse_transcript_content(&content)
}

/// Read the last `max_bytes` of a file as a string.
fn read_tail(path: &str, max_bytes: u64) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let chunk = size.min(max_bytes);
    if chunk == 0 {
        return Some(String::new());
    }
    file.seek(SeekFrom::Start(size - chunk)).ok()?;
    let mut buf = vec![0u8; chunk as usize];
    file.read_exact(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).to_string())
}

/// Parse transcript content (JSONL lines) and determine last_role + pending state.
pub fn parse_transcript_content(content: &str) -> (Option<String>, bool) {
    let mut last_role: Option<String> = None;
    let mut pending = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let msg = entry.get("message").unwrap_or(&serde_json::Value::Null);
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let content_arr = msg.get("content").and_then(|v| v.as_array());

        if entry_type == "assistant" && role == "assistant" {
            last_role = Some("assistant".to_string());
            if let Some(items) = content_arr {
                let types: Vec<&str> = items
                    .iter()
                    .filter_map(|c| c.get("type").and_then(|v| v.as_str()))
                    .collect();
                pending = types.contains(&"tool_use");
            }
        } else if entry_type == "user" && role == "user" {
            last_role = Some("user".to_string());
            if let Some(items) = content_arr {
                let types: Vec<&str> = items
                    .iter()
                    .filter_map(|c| c.get("type").and_then(|v| v.as_str()))
                    .collect();
                if types.contains(&"tool_result") {
                    pending = false;
                }
            }
        }
    }

    (last_role, pending)
}

/// Get file mtime age in seconds (how long ago it was modified).
pub fn get_mtime_age(path: &str) -> Option<f64> {
    let metadata = fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok()?;
    let age = SystemTime::now().duration_since(mtime).ok()?;
    Some(age.as_secs_f64())
}

/// Determine the status of a session based on its transcript file.
pub fn determine_status(transcript: Option<&str>) -> Status {
    let transcript = match transcript {
        Some(t) if !t.is_empty() => t,
        _ => return Status::Active,
    };

    let age = match get_mtime_age(transcript) {
        Some(a) => a,
        None => return Status::Active,
    };

    let (last_role, pending) = parse_transcript_tail(transcript);

    // Pending: tool_use waiting for user action
    // 3s grace period filters auto-approved tools (complete in <2s)
    // 120s timeout degrades to idle (session likely abandoned)
    if pending && age >= 3.0 {
        return if age < 120.0 {
            Status::Pending
        } else {
            Status::Idle
        };
    }

    // Recent activity -> active
    if age < 10.0 {
        return Status::Active;
    }

    // User sent message, Claude processing (API call)
    if last_role.as_deref() == Some("user") {
        return if age < 120.0 {
            Status::Active
        } else {
            Status::Idle
        };
    }

    // Assistant finished -> idle
    Status::Idle
}

/// Testable version of determine_status that takes age as parameter.
pub fn determine_status_with_age(
    transcript_content: Option<&str>,
    age: Option<f64>,
) -> Status {
    let age = match age {
        Some(a) => a,
        None => return Status::Active,
    };

    let (last_role, pending) = match transcript_content {
        Some(content) if !content.is_empty() => parse_transcript_content(content),
        _ => (None, false),
    };

    if pending && age >= 3.0 {
        return if age < 120.0 {
            Status::Pending
        } else {
            Status::Idle
        };
    }

    if age < 10.0 {
        return Status::Active;
    }

    if last_role.as_deref() == Some("user") {
        return if age < 120.0 {
            Status::Active
        } else {
            Status::Idle
        };
    }

    Status::Idle
}

/// Resolve the correct transcript file for a given TTY's session.
///
/// 1. Use this TTY's state file if its transcript still exists.
/// 2. Otherwise fall back to the most-recently-modified transcript
///    that is NOT claimed by another active session's state file.
pub fn resolve_transcript(
    tty_short: &str,
    state_dir: &Path,
    project_dir: &Path,
    active_ttys: &std::collections::HashSet<String>,
) -> String {
    // 1) Try this TTY's state file
    let state_file = state_dir.join(format!("session-{}.json", tty_short));
    if state_file.is_file() {
        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(state) = serde_json::from_str::<crate::state::SessionState>(&content) {
                if !state.transcript_path.is_empty()
                    && Path::new(&state.transcript_path).is_file()
                {
                    return state.transcript_path;
                }
            }
        }
    }

    // 2) Collect transcripts claimed by OTHER active sessions
    let mut claimed = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir(state_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("session-") || !name.ends_with(".json") {
                continue;
            }
            let tty = &name["session-".len()..name.len() - ".json".len()];
            if tty == tty_short || !active_ttys.contains(tty) {
                continue;
            }
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(state) = serde_json::from_str::<crate::state::SessionState>(&content) {
                    if !state.transcript_path.is_empty()
                        && Path::new(&state.transcript_path).is_file()
                    {
                        claimed.insert(state.transcript_path);
                    }
                }
            }
        }
    }

    // Pick the most recent unclaimed transcript
    let mut transcripts: Vec<_> = fs::read_dir(project_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "jsonl")
        })
        .filter_map(|e| {
            let path = e.path().to_string_lossy().to_string();
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((path, mtime))
        })
        .collect();

    // Sort by mtime descending (newest first)
    transcripts.sort_by(|a, b| b.1.cmp(&a.1));

    for (path, _) in transcripts {
        if !claimed.contains(&path) {
            return path;
        }
    }

    String::new()
}

/// Compute project hash from CWD (replaces / and _ with -)
pub fn project_hash(cwd: &str) -> String {
    cwd.replace(['/', '_'], "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_transcript(dir: &Path, name: &str, lines: &[serde_json::Value]) -> String {
        let path = dir.join(format!("{}.jsonl", name));
        let mut f = fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{}", serde_json::to_string(line).unwrap()).unwrap();
        }
        path.to_string_lossy().to_string()
    }

    fn set_mtime(path: &str, seconds_ago: f64) {
        let t = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
            - seconds_ago;
        let ft = filetime::FileTime::from_unix_time(t as i64, ((t.fract()) * 1_000_000_000.0) as u32);
        filetime::set_file_mtime(path, ft).unwrap();
    }

    // ─── parse_transcript_content tests ───

    #[test]
    fn test_empty_content() {
        assert_eq!(parse_transcript_content(""), (None, false));
    }

    #[test]
    fn test_text_only_assistant() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#;
        assert_eq!(
            parse_transcript_content(content),
            (Some("assistant".into()), false)
        );
    }

    #[test]
    fn test_thinking_and_text() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"Done"}]}}"#;
        assert_eq!(
            parse_transcript_content(content),
            (Some("assistant".into()), false)
        );
    }

    #[test]
    fn test_unpaired_tool_use() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            parse_transcript_content(content),
            (Some("assistant".into()), true)
        );
    }

    #[test]
    fn test_paired_tool_use() {
        let line1 = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        let line2 = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#;
        let content = format!("{}\n{}", line1, line2);
        assert_eq!(
            parse_transcript_content(&content),
            (Some("user".into()), false)
        );
    }

    #[test]
    fn test_multiple_rounds() {
        let lines = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{}}]}}"#,
        ];
        let content = lines.join("\n");
        assert_eq!(
            parse_transcript_content(&content),
            (Some("assistant".into()), true)
        );
    }

    #[test]
    fn test_all_paired_multiple_rounds() {
        let lines = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"All done!"}]}}"#,
        ];
        let content = lines.join("\n");
        assert_eq!(
            parse_transcript_content(&content),
            (Some("assistant".into()), false)
        );
    }

    #[test]
    fn test_progress_lines_ignored() {
        let lines = vec![
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hi"}]}}"#,
            r#"{"type":"progress","content":{"type":"status","text":"thinking..."}}"#,
        ];
        let content = lines.join("\n");
        assert_eq!(
            parse_transcript_content(&content),
            (Some("user".into()), false)
        );
    }

    #[test]
    fn test_invalid_json_lines_skipped() {
        let lines = vec![
            "NOT VALID JSON{{{",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done"}]}}"#,
        ];
        let content = lines.join("\n");
        assert_eq!(
            parse_transcript_content(&content),
            (Some("assistant".into()), false)
        );
    }

    #[test]
    fn test_user_message() {
        let content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#;
        assert_eq!(
            parse_transcript_content(content),
            (Some("user".into()), false)
        );
    }

    // ─── determine_status_with_age tests ───

    #[test]
    fn test_no_transcript_is_active() {
        assert_eq!(determine_status_with_age(None, None), Status::Active);
    }

    #[test]
    fn test_recent_mtime_is_active() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done!"}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(5.0)),
            Status::Active
        );
    }

    #[test]
    fn test_recent_mtime_overrides_idle_transcript() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done!"}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(5.0)),
            Status::Active
        );
    }

    #[test]
    fn test_boundary_at_10s() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done!"}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(10.0)),
            Status::Idle
        );
    }

    #[test]
    fn test_last_user_message_is_active() {
        let content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(30.0)),
            Status::Active
        );
    }

    #[test]
    fn test_last_assistant_text_is_idle() {
        let lines = vec![
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi there!"}]}}"#,
        ];
        let content = lines.join("\n");
        assert_eq!(
            determine_status_with_age(Some(&content), Some(30.0)),
            Status::Idle
        );
    }

    #[test]
    fn test_last_assistant_tool_use_is_pending() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(30.0)),
            Status::Pending
        );
    }

    #[test]
    fn test_pending_not_shown_under_3s() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(1.0)),
            Status::Active
        );
    }

    #[test]
    fn test_pending_detected_at_3s() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(3.0)),
            Status::Pending
        );
    }

    #[test]
    fn test_pending_at_5s() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(5.0)),
            Status::Pending
        );
    }

    #[test]
    fn test_pending_at_119s() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(119.0)),
            Status::Pending
        );
    }

    #[test]
    fn test_pending_timeout_120s() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(120.0)),
            Status::Idle
        );
    }

    #[test]
    fn test_pending_timeout_200s() {
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(200.0)),
            Status::Idle
        );
    }

    #[test]
    fn test_user_message_over_120s_is_idle() {
        let content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(130.0)),
            Status::Idle
        );
    }

    #[test]
    fn test_api_latency_60s_is_active() {
        let content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Complex task"}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(60.0)),
            Status::Active
        );
    }

    #[test]
    fn test_api_latency_110s_is_active() {
        let content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Very complex task"}]}}"#;
        assert_eq!(
            determine_status_with_age(Some(content), Some(110.0)),
            Status::Active
        );
    }

    #[test]
    fn test_tool_result_clears_pending() {
        let lines = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#,
        ];
        let content = lines.join("\n");
        // After tool_result, last_role=user, pending=false -> active (API call)
        assert_eq!(
            determine_status_with_age(Some(&content), Some(60.0)),
            Status::Active
        );
    }

    // ─── resolve_transcript tests ───

    #[test]
    fn test_resolve_state_file_valid() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let tp = make_transcript(&project_dir, "aaa", &[]);
        let state = crate::state::SessionState {
            session_id: "aaa".into(),
            transcript_path: tp.clone(),
        };
        fs::write(
            state_dir.join("session-ttys000.json"),
            serde_json::to_string(&state).unwrap(),
        )
        .unwrap();

        let active: std::collections::HashSet<String> = ["ttys000".into()].into();
        let result = resolve_transcript("ttys000", &state_dir, &project_dir, &active);
        assert_eq!(result, tp);
    }

    #[test]
    fn test_resolve_state_file_missing() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        make_transcript(&project_dir, "old", &[]);
        set_mtime(
            &project_dir.join("old.jsonl").to_string_lossy(),
            10.0,
        );
        let new_path = make_transcript(&project_dir, "new", &[]);

        let active: std::collections::HashSet<String> = ["ttys000".into()].into();
        let result = resolve_transcript("ttys000", &state_dir, &project_dir, &active);
        assert_eq!(result, new_path);
    }

    #[test]
    fn test_resolve_state_file_stale() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let state = crate::state::SessionState {
            session_id: "gone".into(),
            transcript_path: "/nonexistent/gone.jsonl".into(),
        };
        fs::write(
            state_dir.join("session-ttys000.json"),
            serde_json::to_string(&state).unwrap(),
        )
        .unwrap();

        let tp = make_transcript(&project_dir, "real", &[]);
        let active: std::collections::HashSet<String> = ["ttys000".into()].into();
        let result = resolve_transcript("ttys000", &state_dir, &project_dir, &active);
        assert_eq!(result, tp);
    }

    #[test]
    fn test_resolve_no_transcripts() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let active: std::collections::HashSet<String> = ["ttys000".into()].into();
        let result = resolve_transcript("ttys000", &state_dir, &project_dir, &active);
        assert_eq!(result, "");
    }

    #[test]
    fn test_resolve_two_sessions_both_valid() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let tp_a = make_transcript(&project_dir, "aaa", &[]);
        let tp_b = make_transcript(&project_dir, "bbb", &[]);
        set_mtime(&tp_b, 5.0);

        let state_a = crate::state::SessionState {
            session_id: "aaa".into(),
            transcript_path: tp_a.clone(),
        };
        let state_b = crate::state::SessionState {
            session_id: "bbb".into(),
            transcript_path: tp_b.clone(),
        };
        fs::write(
            state_dir.join("session-ttys000.json"),
            serde_json::to_string(&state_a).unwrap(),
        )
        .unwrap();
        fs::write(
            state_dir.join("session-ttys009.json"),
            serde_json::to_string(&state_b).unwrap(),
        )
        .unwrap();

        let active: std::collections::HashSet<String> =
            ["ttys000".into(), "ttys009".into()].into();
        assert_eq!(
            resolve_transcript("ttys000", &state_dir, &project_dir, &active),
            tp_a
        );
        assert_eq!(
            resolve_transcript("ttys009", &state_dir, &project_dir, &active),
            tp_b
        );
    }

    #[test]
    fn test_resolve_one_stale_does_not_steal() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let tp_b = make_transcript(&project_dir, "bbb", &[]);
        let tp_a = make_transcript(&project_dir, "aaa", &[]);
        set_mtime(&tp_a, 5.0);

        // A's state is stale
        let state_a = crate::state::SessionState {
            session_id: "gone".into(),
            transcript_path: "/nonexistent/gone.jsonl".into(),
        };
        let state_b = crate::state::SessionState {
            session_id: "bbb".into(),
            transcript_path: tp_b.clone(),
        };
        fs::write(
            state_dir.join("session-ttys000.json"),
            serde_json::to_string(&state_a).unwrap(),
        )
        .unwrap();
        fs::write(
            state_dir.join("session-ttys009.json"),
            serde_json::to_string(&state_b).unwrap(),
        )
        .unwrap();

        let active: std::collections::HashSet<String> =
            ["ttys000".into(), "ttys009".into()].into();
        assert_eq!(
            resolve_transcript("ttys009", &state_dir, &project_dir, &active),
            tp_b
        );
        // A's fallback must skip tp_b (claimed by B) and pick tp_a
        assert_eq!(
            resolve_transcript("ttys000", &state_dir, &project_dir, &active),
            tp_a
        );
    }

    #[test]
    fn test_resolve_dead_session_ignored() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let tp_live = make_transcript(&project_dir, "live", &[]);
        make_transcript(&project_dir, "dead", &[]);
        set_mtime(
            &project_dir.join("dead.jsonl").to_string_lossy(),
            5.0,
        );

        let state_dead = crate::state::SessionState {
            session_id: "dead".into(),
            transcript_path: project_dir.join("dead.jsonl").to_string_lossy().to_string(),
        };
        fs::write(
            state_dir.join("session-ttys005.json"),
            serde_json::to_string(&state_dead).unwrap(),
        )
        .unwrap();

        let active: std::collections::HashSet<String> = ["ttys000".into()].into();
        assert_eq!(
            resolve_transcript("ttys000", &state_dir, &project_dir, &active),
            tp_live
        );
    }

    #[test]
    fn test_resolve_corrupt_state_file() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("swiftbar");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        fs::write(
            state_dir.join("session-ttys000.json"),
            "NOT VALID JSON{{{",
        )
        .unwrap();

        let tp = make_transcript(&project_dir, "real", &[]);
        let active: std::collections::HashSet<String> = ["ttys000".into()].into();
        assert_eq!(
            resolve_transcript("ttys000", &state_dir, &project_dir, &active),
            tp
        );
    }

    #[test]
    fn test_resolve_state_dir_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&project_dir).unwrap();
        let tp = make_transcript(&project_dir, "only", &[]);

        let active: std::collections::HashSet<String> = ["ttys000".into()].into();
        let result = resolve_transcript(
            "ttys000",
            Path::new("/nonexistent/swiftbar"),
            &project_dir,
            &active,
        );
        assert_eq!(result, tp);
    }

    #[test]
    fn test_project_hash() {
        assert_eq!(
            project_hash("/Users/test/my_project"),
            "-Users-test-my-project"
        );
        assert_eq!(project_hash("/a/b/c"), "-a-b-c");
    }
}

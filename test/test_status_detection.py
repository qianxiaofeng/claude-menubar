"""Tests for the transcript-based status detection logic in ClaudeBar.sh's embedded Python.

Covers:
- mtime age fast path (active when recently modified)
- Last-role detection (user -> active, assistant -> idle/pending)
- Pending detection via unpaired tool_use in transcript
- Pending grace period (3s) and timeout (120s)
- Stale session safeguard (>120s)
- Edge cases: no transcript, missing file
"""

import json
import os
import tempfile
import time
import unittest


def parse_transcript_tail(transcript):
    """Exact copy of parse_transcript_tail from ClaudeBar.sh."""
    if not transcript:
        return None, False
    last_role = None
    pending = False
    try:
        with open(transcript, 'rb') as f:
            f.seek(0, 2)
            size = f.tell()
            chunk = min(size, 65536)
            f.seek(size - chunk)
            lines = f.read().decode('utf-8', errors='replace').strip().split('\n')
        for line in lines:
            line = line.strip()
            if not line:
                continue
            try:
                e = json.loads(line)
            except Exception:
                continue
            t = e.get('type', '')
            msg = e.get('message', {})
            role = msg.get('role', '')
            content = msg.get('content', [])
            if t == 'assistant' and role == 'assistant':
                last_role = 'assistant'
                if isinstance(content, list):
                    types = [c.get('type') for c in content if isinstance(c, dict)]
                    pending = 'tool_use' in types
            elif t == 'user' and role == 'user':
                last_role = 'user'
                if isinstance(content, list):
                    types = [c.get('type') for c in content if isinstance(c, dict)]
                    if 'tool_result' in types:
                        pending = False
    except Exception:
        pass
    return last_role, pending


def determine_status(transcript, mtime_unused=None):
    """Exact copy of determine_status from ClaudeBar.sh."""
    if not transcript:
        return "active"
    try:
        mtime = os.path.getmtime(transcript)
    except OSError:
        return "active"
    age = time.time() - mtime

    # Always parse transcript (cheap: last 64KB)
    last_role, pending = parse_transcript_tail(transcript)

    # Pending: tool_use waiting for user action
    # 3s grace period filters auto-approved tools (complete in <2s)
    # 120s timeout degrades to idle (session likely abandoned)
    if pending and age >= 3:
        return "pending" if age < 120 else "idle"

    # Recent activity -> active
    if age < 10:
        return "active"

    # User sent message, Claude processing (API call)
    if last_role == 'user':
        return "active" if age < 120 else "idle"

    # Assistant finished -> idle
    return "idle"


class StatusDetectionBase(unittest.TestCase):

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = self._tmp.name

    def tearDown(self):
        self._tmp.cleanup()

    def add_transcript(self, name, lines=None):
        """Create a .jsonl transcript file and return its path."""
        p = os.path.join(self.tmp, f"{name}.jsonl")
        with open(p, "w") as f:
            if lines:
                for line in lines:
                    f.write(json.dumps(line) + "\n")
        return p

    def set_mtime(self, path, seconds_ago):
        """Set file mtime to N seconds ago."""
        t = time.time() - seconds_ago
        os.utime(path, (t, t))


class TestMtimeAgeFastPath(StatusDetectionBase):
    """mtime < 10s -> active regardless of transcript content."""

    def test_no_transcript_is_active(self):
        self.assertEqual(determine_status(""), "active")

    def test_nonexistent_file_is_active(self):
        self.assertEqual(determine_status("/nonexistent/path.jsonl"), "active")

    def test_recent_mtime_is_active(self):
        tp = self.add_transcript("recent")
        self.set_mtime(tp, 3)
        self.assertEqual(determine_status(tp), "active")

    def test_recent_mtime_overrides_idle_transcript(self):
        """Even if transcript shows assistant done, recent mtime -> active."""
        tp = self.add_transcript("recent_idle", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "Done!"}]}}
        ])
        self.set_mtime(tp, 5)
        self.assertEqual(determine_status(tp), "active")

    def test_boundary_at_10s(self):
        """Exactly 10s ago -> NOT fast path (age < 10 is false)."""
        tp = self.add_transcript("boundary", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "Done!"}]}}
        ])
        self.set_mtime(tp, 10)
        self.assertEqual(determine_status(tp), "idle")


class TestLastRoleDetection(StatusDetectionBase):
    """When mtime > 10s, use last transcript role to determine status."""

    def test_last_user_message_is_active(self):
        """User sent a message, Claude hasn't responded -> active (API call)."""
        tp = self.add_transcript("user_waiting", lines=[
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "text", "text": "Hello"}]}}
        ])
        self.set_mtime(tp, 30)
        self.assertEqual(determine_status(tp), "active")

    def test_last_user_tool_result_is_active(self):
        """Tool result sent, Claude processing -> active (API call)."""
        tp = self.add_transcript("tool_result", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}}
        ])
        self.set_mtime(tp, 60)
        self.assertEqual(determine_status(tp), "active")

    def test_last_assistant_text_is_idle(self):
        """Claude responded with text, waiting for user -> idle."""
        tp = self.add_transcript("assistant_done", lines=[
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "text", "text": "Hello"}]}},
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "Hi there!"}]}}
        ])
        self.set_mtime(tp, 30)
        self.assertEqual(determine_status(tp), "idle")

    def test_last_assistant_tool_use_is_pending(self):
        """Claude called a tool, waiting for result -> pending (after 3s)."""
        tp = self.add_transcript("tool_pending", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.set_mtime(tp, 30)
        self.assertEqual(determine_status(tp), "pending")

    def test_api_latency_60s_is_active(self):
        """60s since user message (long API call) -> still active."""
        tp = self.add_transcript("slow_api", lines=[
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "text", "text": "Complex task"}]}}
        ])
        self.set_mtime(tp, 60)
        self.assertEqual(determine_status(tp), "active")

    def test_api_latency_110s_is_active(self):
        """110s since user message (very long API call) -> still active."""
        tp = self.add_transcript("very_slow_api", lines=[
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "text", "text": "Very complex task"}]}}
        ])
        self.set_mtime(tp, 110)
        self.assertEqual(determine_status(tp), "active")


class TestPendingGracePeriod(StatusDetectionBase):
    """Pending has a 3s grace period to filter auto-approved tools."""

    def test_pending_not_shown_under_3s(self):
        """Within 3s grace period, pending tool_use shows as active."""
        tp = self.add_transcript("pending_early", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.set_mtime(tp, 1)
        self.assertEqual(determine_status(tp), "active")

    def test_pending_detected_at_3s(self):
        """At exactly 3s, pending is detected (age >= 3)."""
        tp = self.add_transcript("pending_3s", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.set_mtime(tp, 3)
        self.assertEqual(determine_status(tp), "pending")

    def test_pending_at_5s(self):
        """At 5s, still pending."""
        tp = self.add_transcript("pending_5s", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]}}
        ])
        self.set_mtime(tp, 5)
        self.assertEqual(determine_status(tp), "pending")


class TestPendingTimeout(StatusDetectionBase):
    """Pending degrades to idle after 120s (session likely abandoned)."""

    def test_pending_at_119s(self):
        """119s -> still pending."""
        tp = self.add_transcript("pending_119", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.set_mtime(tp, 119)
        self.assertEqual(determine_status(tp), "pending")

    def test_pending_timeout_120s(self):
        """At 120s, pending degrades to idle."""
        tp = self.add_transcript("pending_timeout", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.set_mtime(tp, 120)
        self.assertEqual(determine_status(tp), "idle")

    def test_pending_timeout_200s(self):
        """200s -> idle (well past timeout)."""
        tp = self.add_transcript("pending_200", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.set_mtime(tp, 200)
        self.assertEqual(determine_status(tp), "idle")


class TestStaleSessionSafeguard(StatusDetectionBase):
    """Sessions >120s without transcript update -> idle (crash/abandon)."""

    def test_user_message_over_120s_is_idle(self):
        """Even with last=user, >120s means something went wrong -> idle."""
        tp = self.add_transcript("stale", lines=[
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "text", "text": "Hello"}]}}
        ])
        self.set_mtime(tp, 130)
        self.assertEqual(determine_status(tp), "idle")

    def test_assistant_over_120s_is_idle(self):
        """Assistant finished >120s ago -> idle."""
        tp = self.add_transcript("old_idle", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "Done"}]}}
        ])
        self.set_mtime(tp, 200)
        self.assertEqual(determine_status(tp), "idle")


class TestParseTranscriptTail(StatusDetectionBase):
    """Direct tests for parse_transcript_tail."""

    def test_empty_path(self):
        self.assertEqual(parse_transcript_tail(""), (None, False))

    def test_nonexistent_file(self):
        self.assertEqual(parse_transcript_tail("/nonexistent/path.jsonl"),
                         (None, False))

    def test_empty_file(self):
        tp = self.add_transcript("empty")
        self.assertEqual(parse_transcript_tail(tp), (None, False))

    def test_text_only_assistant(self):
        tp = self.add_transcript("text", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "Hello"}]}}
        ])
        self.assertEqual(parse_transcript_tail(tp), ("assistant", False))

    def test_thinking_and_text(self):
        tp = self.add_transcript("thinking", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "thinking", "thinking": "..."}, {"type": "text", "text": "Done"}]}}
        ])
        self.assertEqual(parse_transcript_tail(tp), ("assistant", False))

    def test_unpaired_tool_use(self):
        tp = self.add_transcript("unpaired", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.assertEqual(parse_transcript_tail(tp), ("assistant", True))

    def test_paired_tool_use(self):
        tp = self.add_transcript("paired", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}}
        ])
        self.assertEqual(parse_transcript_tail(tp), ("user", False))

    def test_multiple_rounds(self):
        tp = self.add_transcript("multi", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}},
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t2", "name": "Bash", "input": {}}]}}
        ])
        self.assertEqual(parse_transcript_tail(tp), ("assistant", True))

    def test_all_paired_multiple_rounds(self):
        tp = self.add_transcript("all_paired", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}},
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "All done!"}]}}
        ])
        self.assertEqual(parse_transcript_tail(tp), ("assistant", False))

    def test_progress_lines_ignored(self):
        """Progress lines don't affect last_role tracking."""
        tp = self.add_transcript("progress", lines=[
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "text", "text": "Hi"}]}},
            {"type": "progress", "content": {"type": "status", "text": "thinking..."}}
        ])
        self.assertEqual(parse_transcript_tail(tp), ("user", False))


if __name__ == "__main__":
    unittest.main()

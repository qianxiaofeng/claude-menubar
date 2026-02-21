"""Tests for the mtime-delta status detection logic in ClaudeBar.sh's embedded Python.

Covers:
- mtime delta detection (active when mtime changed, idle when unchanged)
- First cycle behavior (no prev_mtime → active)
- Pending detection via unpaired tool_use in transcript
- Edge cases: no transcript, empty mtime values
"""

import json
import os
import tempfile
import unittest


def check_pending_tool(transcript):
    """Exact copy of the check_pending_tool function from ClaudeBar.sh."""
    if not transcript:
        return False
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
                if isinstance(content, list):
                    types = [c.get('type') for c in content if isinstance(c, dict)]
                    if 'tool_use' in types:
                        pending = True
            elif t == 'user' and role == 'user' and isinstance(content, list):
                types = [c.get('type') for c in content if isinstance(c, dict)]
                if 'tool_result' in types:
                    pending = False
    except Exception:
        pass
    return pending


def determine_status(transcript, mtime, prev_mtime):
    """Exact copy of determine_status from ClaudeBar.sh."""
    if not transcript:
        return "active"
    if mtime != prev_mtime:
        return "active"
    if check_pending_tool(transcript):
        return "pending"
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


class TestMtimeDeltaDetection(StatusDetectionBase):

    def test_no_transcript_is_active(self):
        """No transcript → active (new session, not yet written)."""
        self.assertEqual(determine_status("", "", ""), "active")

    def test_first_cycle_no_prev_mtime_is_active(self):
        """First cache cycle: mtime set, prev_mtime empty → active."""
        tp = self.add_transcript("first")
        self.assertEqual(determine_status(tp, "1700000000", ""), "active")

    def test_mtime_changed_is_active(self):
        """mtime differs from prev_mtime → active (transcript being written)."""
        tp = self.add_transcript("changing")
        self.assertEqual(determine_status(tp, "1700000002", "1700000000"), "active")

    def test_mtime_unchanged_no_pending_is_idle(self):
        """mtime same as prev_mtime, no pending tool → idle."""
        tp = self.add_transcript("idle", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "Done!"}]}}
        ])
        self.assertEqual(determine_status(tp, "1700000000", "1700000000"), "idle")

    def test_mtime_unchanged_with_pending_tool_is_pending(self):
        """mtime same, unpaired tool_use → pending."""
        tp = self.add_transcript("pending", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]}}
        ])
        self.assertEqual(determine_status(tp, "1700000000", "1700000000"), "pending")

    def test_mtime_changed_overrides_pending_tool(self):
        """mtime changed takes priority over pending tool_use → active."""
        tp = self.add_transcript("active_pending", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]}}
        ])
        self.assertEqual(determine_status(tp, "1700000002", "1700000000"), "active")

    def test_swiftbar_restart_first_cycle(self):
        """After SwiftBar restart, prev_mtime is empty → active for 1 cycle."""
        tp = self.add_transcript("restart")
        self.assertEqual(determine_status(tp, "1700000050", ""), "active")


class TestCheckPendingTool(StatusDetectionBase):

    def test_empty_transcript_path(self):
        self.assertFalse(check_pending_tool(""))

    def test_nonexistent_file(self):
        self.assertFalse(check_pending_tool("/nonexistent/path.jsonl"))

    def test_no_tool_use(self):
        tp = self.add_transcript("text_only", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "text", "text": "Hello"}]}}
        ])
        self.assertFalse(check_pending_tool(tp))

    def test_unpaired_tool_use(self):
        tp = self.add_transcript("unpaired", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}
        ])
        self.assertTrue(check_pending_tool(tp))

    def test_paired_tool_use_and_result(self):
        tp = self.add_transcript("paired", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}}
        ])
        self.assertFalse(check_pending_tool(tp))

    def test_multiple_tools_last_unpaired(self):
        tp = self.add_transcript("multi", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}},
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t2", "name": "Bash", "input": {}}]}}
        ])
        self.assertTrue(check_pending_tool(tp))

    def test_multiple_tools_all_paired(self):
        tp = self.add_transcript("all_paired", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}},
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "tool_use", "id": "t2", "name": "Bash", "input": {}}]}},
            {"type": "user", "message": {"role": "user",
             "content": [{"type": "tool_result", "tool_use_id": "t2", "content": "done"}]}}
        ])
        self.assertFalse(check_pending_tool(tp))

    def test_empty_file(self):
        tp = self.add_transcript("empty")
        self.assertFalse(check_pending_tool(tp))

    def test_non_tool_content_types_ignored(self):
        """thinking and text blocks don't trigger pending."""
        tp = self.add_transcript("thinking", lines=[
            {"type": "assistant", "message": {"role": "assistant",
             "content": [{"type": "thinking", "thinking": "..."}, {"type": "text", "text": "Done"}]}}
        ])
        self.assertFalse(check_pending_tool(tp))


if __name__ == "__main__":
    unittest.main()

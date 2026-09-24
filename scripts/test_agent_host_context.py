#!/usr/bin/env python3

import importlib.util
import json
import os
import pathlib
import re
import shlex
import subprocess
import sys
import unittest
from unittest.mock import patch


sys.dont_write_bytecode = True
SCRIPT_DIR = pathlib.Path(__file__).parent


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, SCRIPT_DIR / filename)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


context = load("agent_host_context", "agent-host-context.py")

HERDR = {"HERDR_ENV": "1", "HERDR_PANE_ID": "wK:p1"}
WORKER = {"HERDR_AGENT_PANE": "1"}
HOST_KEYS = ("HERDR_ENV", "HERDR_PANE_ID", "HERDR_AGENT_PANE", "SWARM_AGENT_ID")


class HostContextTest(unittest.TestCase):
    """The hook is the only channel a codex root reads, so its gating is load-bearing."""

    def emit(self, provider="codex", **environment):
        env = {key: value for key, value in os.environ.items() if key not in HOST_KEYS}
        env.update(environment)
        result = subprocess.run(
            [sys.executable, str(SCRIPT_DIR / "agent-host-context.py"), "--provider", provider],
            input="", capture_output=True, text=True, env=env,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def codex_context(self, **environment):
        payload = json.loads(self.emit(**environment))
        return payload.get("hookSpecificOutput", {}).get("additionalContext", "")

    # -- gating

    def test_a_codex_session_outside_any_host_gets_exactly_nothing(self):
        self.assertEqual(json.loads(self.emit()), {})

    def test_each_hosted_provider_receives_only_its_runtime_adapter(self):
        expected = {
            "claude": "~/.claude/skills/orchestrate-claude/SKILL.md",
            "codex": "~/.agents/skills/orchestrate-codex/SKILL.md",
            "agy": "~/.gemini/config/skills/orchestrate-agy/SKILL.md",
        }
        for provider, adapter in expected.items():
            with self.subTest(provider=provider):
                emitted = self.emit(provider=provider, **HERDR)
                body = emitted if provider == "claude" else json.dumps(
                    json.loads(emitted)
                )
                self.assertIn(f"[agent-runtime: {provider}]", body)
                self.assertIn(adapter, body)
                for other in set(expected.values()) - {adapter}:
                    self.assertNotIn(other, body)

    def test_host_contract_precedes_runtime_binding(self):
        body = self.codex_context(**HERDR)
        self.assertLess(body.index("[agent-host: herdr]"),
                        body.index("[agent-runtime: codex]"))

    # -- the registry clause

    def test_a_session_without_a_drop_box_still_gets_the_probe(self):
        body = self.codex_context(**HERDR)
        self.assertIn("Settle your spawn path ONCE", body)
        self.assertIn("you are the unsandboxed driver", body)
        self.assertIn("You have no spawn path", body)

    # -- worker panes

    def test_a_herdr_worker_is_given_the_worker_contract_instead(self):
        body = self.emit(provider="claude", **dict(HERDR, **WORKER))
        self.assertIn("[agent-host: herdr — worker]", body)
        self.assertNotIn("[agent-host: herdr]", body)
        self.assertIn("Never spawn visible panes", body)
        self.assertIn("[agent-runtime: claude]", body)

    def test_either_marker_alone_makes_a_session_a_worker(self):
        for marker in ({"HERDR_AGENT_PANE": "1"}, {"SWARM_AGENT_ID": "cl-seat-1"}):
            with self.subTest(marker=marker):
                body = self.emit(provider="claude", **dict(HERDR, **marker))
                self.assertIn("[agent-host: herdr — worker]", body)

    def test_the_orchestrator_seat_keeps_the_orchestrator_contract(self):
        body = self.emit(provider="claude", **dict(HERDR, SWARM_AGENT_ID="orchestrator"))
        self.assertNotIn("[agent-host: herdr — worker]", body)

    def test_a_codex_worker_is_told_to_remain_a_leaf(self):
        body = self.codex_context(**dict(HERDR, **WORKER))
        self.assertIn("[agent-host: herdr — worker]", body)
        self.assertIn("Remain a leaf", body)
        self.assertNotIn("spawn_agent", body)

    def test_agy_workers_invoke_history_containment(self):
        calls = []

        def run(command, **kwargs):
            calls.append((command, kwargs))

        with patch.dict(os.environ, {"HERDR_AGENT_PANE": "1"}, clear=True):
            context.contain_worker_history("agy", '{"id":"worker"}', run)
        self.assertEqual(calls[0][0], [str(context.WORKER_HISTORY), "--provider", "agy"])
        self.assertEqual(calls[0][1]["input"], '{"id":"worker"}')

    def test_codex_history_is_contained_after_spawn(self):
        calls = []
        with patch.dict(os.environ, {"HERDR_AGENT_PANE": "1"}, clear=True):
            context.contain_worker_history("codex", "{}", lambda *args: calls.append(args))
        self.assertEqual(calls, [])

    def test_root_sessions_do_not_invoke_history_containment(self):
        calls = []
        with patch.dict(os.environ, {}, clear=True):
            context.contain_worker_history("codex", "{}", lambda *args, **kwargs: calls.append(args))
        self.assertEqual(calls, [])

    # -- other providers are unaffected

if __name__ == "__main__":
    unittest.main(verbosity=2)

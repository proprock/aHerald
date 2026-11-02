#!/usr/bin/env python3
"""Exercise console setup in a private pseudo-terminal and private user files."""
import json
import os
from pathlib import Path
import pty
import select
import shutil
import subprocess
import sys
import tempfile
import time


def dialog(executable, environment, answers):
    master, slave = pty.openpty()
    process = subprocess.Popen(
        [str(executable), "--console-setup"],
        stdin=slave, stdout=slave, stderr=slave, env=environment,
    )
    os.close(slave)
    output = b""
    try:
        for prompt, answer in answers:
            deadline = time.monotonic() + 10
            while prompt.encode() not in output:
                if time.monotonic() >= deadline:
                    raise AssertionError(f"Missing prompt {prompt!r}: {output!r}")
                ready, _, _ = select.select([master], [], [], 0.2)
                if ready:
                    chunk = os.read(master, 65536)
                    if not chunk:
                        raise AssertionError(f"Setup closed early: {output!r}")
                    output += chunk
            output = b""
            os.write(master, answer.encode() + b"\n")
        process.wait(timeout=10)
        assert process.returncode == 0, process.returncode
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)


def prompts(codex, claude):
    """The prompts a run shows: warning questions only for a selected agent."""
    names = ["Configure Codex hooks?", "Configure Claude Code hooks?",
             "Enable Codex usage monitoring?", "Enable Claude usage monitoring?"]
    if codex:
        names += ["Enable Codex context warning?", "Enable Codex idle warning?"]
    if claude:
        names += ["Enable Claude context warning?", "Enable Claude idle warning?"]
    return names + ["Apply these changes?"]


def run(exe, env, harnesses, answers, apply=True):
    """Answer hooks, usage and the selected agents' warnings; then apply or decline."""
    codex, claude = harnesses
    names = prompts(codex, claude)
    assert len(answers) == len(names) - 1, (answers, names)
    steps = list(zip(names, answers + ["y" if apply else "n"]))
    if apply:
        steps.append(("Launch aHerald now?", "n"))
    dialog(exe, env, steps)


def settings(exe, env):
    out = subprocess.run([str(exe), "--setup-settings"], env=env, check=True,
                         capture_output=True, text=True).stdout
    return dict(line.split("=") for line in out.split())


def main():
    binary = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix="aherald-console-") as temp:
        root = Path(temp)
        exe = root / "aherald"
        shutil.copy2(binary, exe)
        env = os.environ.copy()
        env.update(HOME=str(root), CODEX_HOME=str(root / "codex"),
                   CLAUDE_CONFIG_DIR=str(root / "claude"))
        codex_hooks = root / "codex/hooks.json"
        claude_file = root / "claude/settings.json"
        config = root / "config.toml"

        # Every optional feature starts off, and a declined preview changes nothing.
        assert all(value == "false" for value in settings(exe, env).values())
        run(exe, env, (True, False), ["y", "n", "", "", "", ""], apply=False)
        assert not codex_hooks.exists() and not config.exists()

        # Empty answers take the defaults: Codex hooks only, warnings off.
        run(exe, env, (True, False), ["y", "n", "", "", "", ""])
        assert codex_hooks.exists() and not claude_file.exists()
        state = settings(exe, env)
        assert state["codex_context"] == state["codex_idle"] == "false", state

        # Both agents, with a status line of the user's own that must stay in use.
        claude_file.parent.mkdir(parents=True)
        original = {"theme": "dark", "statusLine": {
            "type": "command", "command": "echo mine", "padding": 2}}
        claude_file.write_text(json.dumps(original))
        run(exe, env, (True, True), ["y", "y", "n", "n", "y", "y", "y", "n"])
        state = settings(exe, env)
        assert (state["codex_context"], state["codex_idle"], state["claude_context"],
                state["claude_idle"]) == ("true", "true", "true", "false"), state
        wrapped = json.loads(claude_file.read_text())["statusLine"]
        assert "--statusline --then 'echo mine'" in wrapped["command"], wrapped
        assert wrapped["padding"] == 2
        text = config.read_text()
        assert "250k" in text and "400k" in text, text

        # Empty answers keep every choice, byte for byte.
        files = [codex_hooks, claude_file, config]
        before = [file.read_bytes() for file in files]
        run(exe, env, (True, True), [""] * 8)
        assert [file.read_bytes() for file in files] == before

        # Turning the Claude context warning off restores the user's status line and keeps the
        # threshold they can turn back on.
        run(exe, env, (True, True), ["", "", "", "", "", "", "n", ""])
        restored = json.loads(claude_file.read_text())
        assert restored["statusLine"] == original["statusLine"], restored
        assert restored["theme"] == "dark" and "hooks" in restored
        assert settings(exe, env)["claude_context"] == "false"
        assert "400k" in config.read_text()

        # Deselecting Claude asks nothing about it and leaves its settings alone.
        kept = config.read_bytes()
        run(exe, env, (True, False), ["", "n", "", "", "", ""])
        assert config.read_bytes() == kept
        assert b"aherald" not in claude_file.read_bytes()

        # Deselecting everything removes the hooks.
        run(exe, env, (False, False), ["n", "n", "n", "n"])
        assert b'"hooks"' not in codex_hooks.read_bytes()
        print("Console setup: preview, defaults, warnings, statusLine wrap/restore, "
              "repeat and deselection passed")


if __name__ == "__main__":
    main()

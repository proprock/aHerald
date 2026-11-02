#!/usr/bin/env python3
"""Exercise console setup in a private pseudo-terminal and private user files."""
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


def main():
    binary = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix="aherald-console-") as temp:
        root = Path(temp)
        exe = root / "aherald"
        shutil.copy2(binary, exe)
        env = os.environ.copy()
        env.update(HOME=str(root), CODEX_HOME=str(root / "codex"),
                   CLAUDE_CONFIG_DIR=str(root / "claude"))
        questions = ["Configure Codex hooks?", "Configure Claude Code hooks?",
                     "Enable Codex usage monitoring?", "Enable Claude usage monitoring?",
                     "Apply these changes?"]
        dialog(exe, env, list(zip(questions, ["y", "n", "n", "n", "n"])))
        assert not (root / "codex/hooks.json").exists()
        dialog(exe, env, list(zip(questions, ["y", "n", "n", "n", "y"])) +
               [("Launch aHerald now?", "n")])
        hook_file = root / "codex/hooks.json"
        assert hook_file.exists()
        first = hook_file.read_bytes()
        # Empty answers retain the selected harness and usage values.
        dialog(exe, env, list(zip(questions, ["", "", "", "", "y"])) +
               [("Launch aHerald now?", "n")])
        assert hook_file.read_bytes() == first
        dialog(exe, env, list(zip(questions, ["n", "n", "n", "n", "y"])) +
               [("Launch aHerald now?", "n")])
        assert b'"hooks"' not in hook_file.read_bytes()
        print("Console setup: preview, apply, repeat and deselection passed")


if __name__ == "__main__":
    main()

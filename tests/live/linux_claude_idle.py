#!/usr/bin/env python3
"""Opt-in Claude print-mode idle probe; private X11, D-Bus and IPC required."""
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tempfile
import time
import uuid

PROMPT = "Reply with exactly OK. Do not use tools or inspect files."


def main():
    if os.environ.get("AH115_ISOLATED") != "1":
        raise RuntimeError("run under private Xvfb and D-Bus with AH115_ISOLATED=1")
    repo = Path(__file__).resolve().parents[2]
    exe = (repo / "target/debug/aherald").resolve(strict=True)
    bridge = (repo / "target/debug/examples/session_warning_bridge").resolve(strict=True)
    root = Path(tempfile.mkdtemp(prefix="aherald-claude-idle-linux-"))
    root.chmod(0o700)
    workspace = root / "workspace"
    workspace.mkdir()
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    config = root / "config.toml"
    config.write_text("[sounds]\nenabled=false\n[codex.usage]\nenabled=false\n"
                      "[claude.usage]\nenabled=false\n[claude.warnings]\nidle_minutes=1\n"
                      "[events.turn_complete]\nenabled=false\n[events.idle_warning]\nduration_sec=0\n")
    env = os.environ.copy()
    env.update(AH101_SCOPE="ah101-" + uuid.uuid4().hex, AH101_ROOT=str(root),
               AH101_AGENT="claude", XDG_RUNTIME_DIR=str(runtime))
    def status():
        return json.loads(subprocess.run([str(bridge), "--status"], env=env, capture_output=True,
                                         timeout=5, check=True).stdout)
    command = shlex.join([str(bridge), "--hook", "--resident", "--agent", "claude", "--config", str(config)])
    hooks = {}
    for name in ("UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop", "SessionEnd"):
        handlers = [{"type": "command", "command": command, "timeout": 3}]
        if name == "Stop":
            handlers.append({"type": "command", "command": "sleep 75", "timeout": 85})
        hooks[name] = [{"hooks": handlers}]
    settings = root / "settings.json"
    settings.write_text(json.dumps({"hooks": hooks, "enabledPlugins": {}}))
    resident = harness = None
    try:
        resident = subprocess.Popen([str(exe), "--resident-child", env["AH101_SCOPE"],
                                     "--config", str(config)], env=env, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                    start_new_session=True)
        deadline = time.monotonic() + 10
        while True:
            state = status()
            if state.get("result") == "status":
                break
            if resident.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("resident did not become ready")
            time.sleep(.1)
        if state.get("pid") != resident.pid or state.get("queued") != 0:
            raise RuntimeError("resident ownership or initial queue check failed")
        args = ["claude", "-p", PROMPT, "--setting-sources", "", "--settings", str(settings),
                "--strict-mcp-config", "--tools", "", "--no-session-persistence"]
        # The fixed response is discarded after checking; no raw payloads are retained.
        harness = subprocess.Popen(args, cwd=workspace, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                   start_new_session=True)
        deadline = time.monotonic() + 160
        warned = False
        while harness.poll() is None and time.monotonic() < deadline:
            if status().get("queued") == 1:
                warned = True
            time.sleep(.25)
        if harness.poll() is None:
            raise RuntimeError("Claude exceeded 160 seconds")
        out, _ = harness.communicate(timeout=5)
        if harness.returncode != 0 or out.strip() != b"OK":
            raise RuntimeError(f"Claude failed or response differed from OK (exit {harness.returncode}); output discarded")
        records = [json.loads(p.read_text()) for p in root.glob("claude-*.json")]
        names = {r.get("event") for r in records}
        if not {"UserPromptSubmit", "Stop", "SessionEnd"} <= names:
            raise RuntimeError("missing prompt, Stop or SessionEnd dispatch")
        if any(r.get("code") != 0 or r.get("milliseconds", 3000) >= 3000 for r in records):
            raise RuntimeError("bridge hook failed or exceeded three seconds")
        if not warned or status().get("queued") != 0:
            raise RuntimeError("idle warning was not observed or SessionEnd did not clear it")
        (root / "result.json").write_text(json.dumps({"idle_observed": True, "end_queue": 0}))
        print("PASS: Claude lifecycle hooks produced and cleared an idle warning through the private bridge")
    finally:
        if harness is not None and harness.poll() is None:
            os.killpg(harness.pid, signal.SIGKILL)
            harness.wait(timeout=5)
        if resident is not None and resident.poll() is None:
            try:
                if status().get("pid") == resident.pid:
                    subprocess.run([str(bridge), "--quit"], env=env, timeout=5,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                resident.wait(timeout=5)
            except (subprocess.SubprocessError, OSError, ValueError):
                os.killpg(resident.pid, signal.SIGKILL)
                resident.wait(timeout=5)
        print(f"Evidence: {root}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        sys.exit(1)

#!/usr/bin/env python3
"""Opt-in Codex context probe. Run under a private Xvfb and D-Bus session."""
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


def record():
    payload = sys.stdin.buffer.read(65537)
    if len(payload) > 65536:
        raise ValueError("hook input exceeds limit")
    event = json.loads(payload)
    name = event.get("hook_event_name")
    if name not in ("PreModelRequest", "PostModelRequest"):
        raise ValueError("unexpected hook")
    result = {"event": name, "session": event.get("session_id"),
              "turn": event.get("turn_id"), "subagent": bool(event.get("agent_id"))}
    if name == "PostModelRequest":
        usage = event.get("token_usage") or {}
        result.update(status=event.get("status"), input_tokens=usage.get("input_tokens"))
        subprocess.run([os.environ["AH115_BRIDGE"], "--hook", "--resident"],
                       input=payload, check=True, timeout=3, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL)
    target = Path(os.environ["AH115_ROOT"]) / (name + "-" + uuid.uuid4().hex + ".json")
    target.write_text(json.dumps(result), encoding="utf-8")


def main():
    if len(sys.argv) == 2 and sys.argv[1] == "--record":
        record()
        return
    if os.environ.get("AH115_ISOLATED") != "1":
        raise RuntimeError("run under a private Xvfb and D-Bus session with AH115_ISOLATED=1")
    repo = Path(__file__).resolve().parents[2]
    exe = (repo / "target/debug/aherald").resolve(strict=True)
    bridge = (repo / "target/debug/examples/session_warning_bridge").resolve(strict=True)
    root = Path(tempfile.mkdtemp(prefix="aherald-codex-linux-"))
    root.chmod(0o700)
    workspace = root / "workspace"
    workspace.mkdir()
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    config = root / "config.toml"
    config.write_text("[sounds]\nenabled=false\n[codex.usage]\nenabled=false\n"
                      "[claude.usage]\nenabled=false\n[codex.warnings]\ncontext_tokens=1\n"
                      "idle_minutes=0\n[events.context_warning]\nduration_sec=0\n")
    env = os.environ.copy()
    env.update(AH101_SCOPE="ah101-" + uuid.uuid4().hex, AH115_ROOT=str(root),
               AH115_BRIDGE=str(bridge), XDG_RUNTIME_DIR=str(runtime))
    # This directory is only a private socket namespace; the account is left untouched.
    def status():
        p = subprocess.run([str(bridge), "--status"], env=env, capture_output=True,
                           timeout=5, check=True)
        return json.loads(p.stdout)
    resident = None
    try:
        resident = subprocess.Popen([str(exe), "--resident-child", env["AH101_SCOPE"],
                                     "--config", str(config)], env=env,
                                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL, start_new_session=True)
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
        command = shlex.join([sys.executable, str(Path(__file__).resolve()), "--record"])
        args = ["codex", "--no-daemon", "exec", "--ignore-user-config", "--ignore-rules",
                "--ephemeral", "--skip-git-repo-check", "--dangerously-bypass-hook-trust",
                "--disable", "plugins", "--disable", "apps", "--disable", "remote_plugin",
                "--disable", "plugin_discovery_prefetch", "-C", str(workspace),
                "-s", "read-only", "-c", "analytics.enabled=false"]
        for name in ("PreModelRequest", "PostModelRequest"):
            args += ["-c", f"hooks.{name}=[{{hooks=[{{type=\"command\",command={json.dumps(command)},timeout=5}}]}}]"]
        args.append(PROMPT)
        # No raw output, input payload, credentials or provider diagnostics are stored.
        p = subprocess.Popen(args, cwd=workspace, env=env, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        try:
            out, _ = p.communicate(timeout=90)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.communicate(timeout=5)
            raise RuntimeError("Codex exceeded 90 seconds")
        if p.returncode != 0 or out.strip() != b"OK":
            raise RuntimeError(f"Codex failed or response differed from OK (exit {p.returncode}); output discarded")
        pre = list(root.glob("PreModelRequest-*.json"))
        post = [json.loads(x.read_text()) for x in root.glob("PostModelRequest-*.json")]
        valid = [x for x in post if x.get("status") == "completed" and not x.get("subagent")
                 and x.get("session") and x.get("turn") and isinstance(x.get("input_tokens"), int)
                 and x["input_tokens"] > 0]
        if not pre or not valid:
            raise RuntimeError("missing completed main-session model-request hook evidence")
        state = status()
        if state.get("queued") != 1:
            raise RuntimeError("expected one queued context warning")
        (root / "result.json").write_text(json.dumps({"queued": state["queued"], "valid_posts": len(valid)}))
        print("PASS: real Codex model request reached the private resident through the bridge")
    finally:
        if resident is not None:
            if resident.poll() is None:
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

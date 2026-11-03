"""Drives `pi --mode rpc` for one prompt and answers the extension's confirm dialog after 2 s,
so ui_prompt_start / ui_prompt_end can be observed (PROBE_UI=1 in the extension)."""
import json, select, subprocess, time

p = subprocess.Popen(["pi", "--mode", "rpc", "--provider", "fake", "--model", "fake-model"],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
p.stdin.write(json.dumps({"id": "r1", "type": "prompt", "message": "say ok"}) + "\n")
p.stdin.flush()
deadline = time.time() + 60
while time.time() < deadline:
    if not select.select([p.stdout], [], [], 1)[0]:
        continue
    line = p.stdout.readline()
    if not line:
        break
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if message.get("type") == "extension_ui_request" and message.get("method") == "confirm":
        time.sleep(2)
        p.stdin.write(json.dumps({"type": "extension_ui_response", "id": message["id"], "confirmed": True}) + "\n")
        p.stdin.flush()
    if message.get("type") == "agent_end":
        time.sleep(2)
        break
p.kill()

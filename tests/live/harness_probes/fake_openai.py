"""A local OpenAI-compatible chat endpoint for harness probes (AH-201).

usage: fake_openai.py PORT [MODE]
  ok     stream the word "ok" (default)
  error  answer every request with HTTP 500
  slow   wait 30 s before answering, so a probe can interrupt the turn
  once   like tool, but only the very first request calls the tool (a sub-agent then answers "ok")
  tool   first call the tool named by FAKE_TOOL (default bash) with {"command": "echo hi"};
         after the tool result answer "ok"
"""
import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MODE = sys.argv[2] if len(sys.argv) > 2 else "ok"
CALLS = 0


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        request = json.loads(self.rfile.read(n) or b"{}")
        if MODE == "error":
            body = json.dumps({"error": {"message": "probe failure", "type": "server_error"}}).encode()
            self.send_response(500); self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body))); self.end_headers(); self.wfile.write(body)
            return
        if MODE == "slow":
            time.sleep(30)
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()

        def chunk(delta, finish=None):
            body = {"id": "x", "object": "chat.completion.chunk", "created": 0, "model": "fake",
                    "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
            self.wfile.write(("data: " + json.dumps(body) + "\n\n").encode()); self.wfile.flush()

        global CALLS
        CALLS += 1
        had_tool_result = any(m.get("role") == "tool" for m in request.get("messages", []))
        if (MODE == "tool" and not had_tool_result) or (MODE == "once" and CALLS == 1):
            chunk({"role": "assistant", "tool_calls": [{"index": 0, "id": "call_1", "type": "function",
                   "function": {"name": os.environ.get("FAKE_TOOL", "bash"), "arguments": os.environ.get("FAKE_ARGS") or json.dumps({"command": "echo hi", "description": "probe"})}}]})
            chunk({}, "tool_calls")
        else:
            chunk({"role": "assistant", "content": "ok"}); chunk({}, "stop")
        self.wfile.write(b"data: [DONE]\n\n"); self.wfile.flush()


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()

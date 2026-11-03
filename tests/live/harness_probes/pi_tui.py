"""Starts Pi's interactive TUI in a pseudo-terminal, submits one prompt and quits, so the
extension probe can report ctx.mode / ctx.hasUI of an interactive session."""
import os, pty, select, sys, time

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execvp("pi", ["pi", "--provider", "fake", "--model", "fake-model", "--tui-mode", "regular"])
deadline = time.time() + 45
sent = False
start = time.time()
while time.time() < deadline:
    if select.select([fd], [], [], 0.5)[0]:
        try:
            if not os.read(fd, 65536):
                break
        except OSError:
            break
    if not sent and time.time() - start > 6:
        os.write(fd, b"say ok\r"); sent = True; sent_at = time.time()
    if sent and time.time() - sent_at > 15:
        os.write(fd, b"\x03"); time.sleep(0.5); os.write(fd, b"\x03"); time.sleep(1)
        break
try:
    os.kill(pid, 15)
except OSError:
    pass

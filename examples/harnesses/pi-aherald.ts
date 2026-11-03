// aHerald bridge for Pi (verified on Pi 1.5.0, see docs/harness-compatibility.md).
// Install: copy to ~/.pi/agent/extensions/aherald.ts. Remove: delete that file.
// It reports to `aherald --notify pi` and sends only the session id and the folder; never
// prompts, messages, tool arguments or credentials. A missing or failing aherald is ignored.
import { spawn } from "node:child_process"

const EXE = process.env.AHERALD_EXE || "aherald"

type Send = (args: string[], session: string, cwd: string) => void

// Calls run one after another: a permission request and its answer arrive milliseconds
// apart and must reach aherald in that order. Each call is bounded to 3 s.
let queue: Promise<void> = Promise.resolve()
function spawnSend(args: string[], session: string, cwd: string): void {
  queue = queue.then(() => run(args, session, cwd))
}

function run(args: string[], session: string, cwd: string): Promise<void> {
  return new Promise((resolve) => {
    try {
      const child = spawn(EXE, ["--notify", "pi", ...args], { stdio: ["pipe", "ignore", "ignore"], windowsHide: true, detached: true })
      const timer = setTimeout(() => { child.kill(); resolve() }, 3000)
      timer.unref()
      const done = () => { clearTimeout(timer); resolve() }
      child.on("error", done)
      child.on("exit", done)
      child.stdin.on("error", () => {})
      child.stdin.end(JSON.stringify({ session_id: session, cwd }))
      child.unref()
    } catch { resolve() }
  })
}

export function bridge(pi: any, send: Send): void {
  const failed = new Set<string>()
  // Only an interactive session (TUI or RPC) has a user to notify. Sub-agents and `pi -p`
  // scripts run with hasUI === false and stay silent.
  const on = (name: string, handle: (event: any, session: string, cwd: string) => void) =>
    pi.on(name, async (event: any, ctx: any) => {
      try {
        if (!ctx?.hasUI) return
        handle(event, String(ctx.sessionManager?.getSessionId?.() ?? ""), String(ctx.cwd ?? ""))
      } catch {}
    })
  on("agent_start", (_e, s, c) => send(["--kind", "turn_complete", "--activity"], s, c))
  on("ui_prompt_start", (_e, s, c) => send(["--kind", "input_required"], s, c))
  on("ui_prompt_end", (_e, s, c) => send(["--kind", "turn_complete", "--activity"], s, c))
  // agent_end repeats across Pi's own retries; agent_settled is the final idle boundary and
  // carries no error flag, so the outcome of the last settle attempt is kept here.
  on("agent_before_settle", (e, s) => {
    if (e?.outcome === "error") failed.add(s)
    else failed.delete(s)
  })
  on("agent_settled", (e, s, c) => {
    const wasError = failed.delete(s)
    const kind = e?.aborted ? "interrupted" : wasError ? "error" : "turn_complete"
    send(["--kind", kind], s, c)
  })
  on("session_shutdown", (_e, s, c) => {
    failed.delete(s)
    send(["--kind", "turn_complete", "--ended"], s, c)
  })
}

export default function (pi: any): void {
  bridge(pi, spawnSend)
}

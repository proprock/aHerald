// aHerald bridge for OpenCode 2.x (verified on 2.2.0, see docs/harness-compatibility.md).
// Install: copy to ~/.config/opencode/plugins/aherald.ts. Remove: delete that file.
// It reports to `aherald --notify opencode` and sends only the session id and the folder;
// never prompts, messages, tool arguments or credentials. A missing or failing aherald is ignored.
import { spawn } from "node:child_process"
import { Plugin } from "@opencode/plugin"

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
      const child = spawn(EXE, ["--notify", "opencode", ...args], { stdio: ["pipe", "ignore", "ignore"], windowsHide: true, detached: true })
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

/** Turns one OpenCode event into at most one aherald call. */
export function handler(send: Send, fallbackCwd: () => string) {
  const folders = new Map<string, string>()
  const children = new Set<string>()
  return (event: any): void => {
    try {
      const data = event?.data ?? {}
      const session = typeof data.sessionID === "string" ? data.sessionID : ""
      if (!session) return
      if (event.type === "session.created") {
        if (data.parentID) children.add(session)
        else if (typeof event.location?.directory === "string") folders.set(session, event.location.directory)
        return
      }
      // Sub-agents run as child sessions; only the parent session is the user's.
      if (children.has(session)) return
      const cwd = folders.get(session) ?? fallbackCwd()
      switch (event.type) {
        case "session.execution.started":
        case "permission.replied":
          return send(["--kind", "turn_complete", "--activity"], session, cwd)
        case "session.execution.succeeded":
          return send(["--kind", "turn_complete"], session, cwd)
        // session.retry.scheduled precedes the final failure and is deliberately ignored.
        case "session.execution.failed":
          return send(["--kind", "error"], session, cwd)
        case "session.execution.interrupted":
          return send(["--kind", "interrupted"], session, cwd)
        case "permission.asked":
          return send(["--kind", "permission_required"], session, cwd)
      }
    } catch {}
  }
}

export default Plugin.define({
  id: "aherald",
  async setup(ctx: any) {
    const handle = handler(spawnSend, () => process.cwd())
    ;(async () => {
      try {
        for await (const event of ctx.event.subscribe()) handle(event)
      } catch {}
    })()
  },
})

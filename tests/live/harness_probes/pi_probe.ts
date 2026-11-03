import { appendFileSync } from "node:fs"
const NAMES = ["session_start","session_shutdown","before_agent_start","agent_start","agent_end","agent_before_settle","agent_settled","turn_start","turn_end","ui_prompt_start","ui_prompt_end","session_compact","session_compact_failed","session_info_changed","tool_execution_start","tool_execution_end"]
export default function (pi: any) {
  const out = process.env.PROBE_OUT!
  for (const name of NAMES) {
    pi.on(name, async (event: any, ctx: any) => {
      appendFileSync(out, JSON.stringify({ name, keys: Object.keys(event ?? {}), aborted: event?.aborted, outcome: event?.outcome, reason: event?.reason, cwd: ctx?.cwd, sessionId: ctx?.sessionManager?.getSessionId?.() }) + "\n")
    })
  }
}

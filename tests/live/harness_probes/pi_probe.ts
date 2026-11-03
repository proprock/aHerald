import { appendFileSync } from "node:fs"
// Logs the lifecycle events a notifier could use. PROBE_ABORT=1 aborts the turn as Esc does;
// PROBE_UI=1 asks one confirmation so the ui_prompt_* events can be seen.
const NAMES = ["session_start","session_shutdown","before_agent_start","agent_start","agent_end","agent_before_settle","agent_settled","turn_start","turn_end","ui_prompt_start","ui_prompt_end","session_compact","session_compact_failed","session_info_changed","tool_execution_start","tool_execution_end"]
export default function (pi: any) {
  const out = process.env.PROBE_OUT!
  for (const name of NAMES) {
    pi.on(name, async (event: any, ctx: any) => {
      appendFileSync(out, JSON.stringify({ name, keys: Object.keys(event ?? {}), aborted: event?.aborted, outcome: event?.outcome, reason: event?.reason, kind: event?.kind, error: event?.message?.errorMessage, stopReason: event?.message?.stopReason, hasUI: ctx?.hasUI, mode: ctx?.mode, cwd: ctx?.cwd, sessionId: ctx?.sessionManager?.getSessionId?.() }) + "\n")
      if (name === "turn_start" && process.env.PROBE_ABORT) ctx.abort()
      if (name === "before_agent_start" && process.env.PROBE_UI) {
        const answer = await ctx.ui.confirm("probe", "continue?")
        appendFileSync(out, JSON.stringify({ confirmAnswered: answer }) + "\n")
      }
    })
  }
}

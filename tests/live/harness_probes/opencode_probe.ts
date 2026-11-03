import { appendFileSync } from "node:fs"
import { Plugin } from "@opencode/plugin"
// Logs the lifecycle events a notifier could use (names and field names, plus short scalar
// fields: never message text, prompts or tool arguments).
const out = process.env.PROBE_OUT!
const log = (o: unknown) => appendFileSync(out, JSON.stringify(o) + "\n")
const scalars = (d: any) => Object.fromEntries(Object.entries(d ?? {}).filter(([, v]) => typeof v === "string" && (v as string).length < 40 || typeof v === "boolean"))
export default Plugin.define({
  id: "aherald-probe",
  async setup(ctx: any) {
    log({ setup: true, sessionApi: Object.keys(ctx.session ?? {}) })
    ;(async () => {
      try {
        for await (const event of ctx.event.subscribe()) {
          const e = event as any
          if (/^(integration|provider|model|agent|command|skill|websearch|reference|plugin)\.updated$/.test(e.type)) continue
          if (/^session\.(text|step|usage|instructions|message)/.test(e.type)) continue
          if (e.type === "session.execution.started" && process.env.PROBE_INTERRUPT) {
            const call = ctx.session?.interrupt ?? ctx.session?.abort
            setTimeout(() => Promise.resolve(call?.call(ctx.session, { sessionID: e.data.sessionID })).catch((x: any) => log({ interruptError: String(x).slice(0, 120) })), 3000)
          }
          const err = e.data?.error
          log({ type: e.type, dataKeys: Object.keys(e.data ?? {}), scalars: scalars(e.data), ...(err ? { error: { keys: Object.keys(err), type: err.type ?? err.name ?? err._tag, message: String(err.message ?? "").slice(0, 80) } } : {}) })
        }
      } catch (err: any) { log({ subscribeError: String(err) }) }
    })()
  },
})

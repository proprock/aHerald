import { appendFileSync } from "node:fs"
import { Plugin } from "@opencode/plugin"
const out = process.env.PROBE_OUT!
const log = (o: unknown) => appendFileSync(out, JSON.stringify(o) + "\n")
export default Plugin.define({
  id: "aherald-probe",
  async setup(ctx: any) {
    log({ setup: true, ctxKeys: Object.keys(ctx) })
    const ac = new AbortController()
    ;(async () => {
      try {
        for await (const event of ctx.event.subscribe()) {
          const p = (event as any).properties ?? (event as any).data ?? {}
          log({ type: (event as any).type, keys: Object.keys(event as any), pkeys: Object.keys(p) })
        }
      } catch (e: any) { log({ subscribeError: String(e) }) }
    })()
    return () => ac.abort()
  },
})

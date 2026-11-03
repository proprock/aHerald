// Contract tests for the harness bridges: events in, aherald calls out. Run with
// `node --test tests/bridges` (Node >= 22.18); tests/bridges.rs runs this when node exists.
import test from "node:test"
import assert from "node:assert/strict"
import { chmodSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { setTimeout as sleep } from "node:timers/promises"

// OpenCode's bridge imports a package that only exists inside OpenCode.
import { register } from "node:module"
register("data:text/javascript," + encodeURIComponent(`
export async function resolve(s, c, n) { return s === "@opencode/plugin" ? { url: "data:text/javascript,export const Plugin={define:(x)=>x}", shortCircuit: true } : n(s, c) }`))
const { bridge } = await import("../../examples/harnesses/pi-aherald.ts")
const { handler } = await import("../../examples/harnesses/opencode-aherald.ts")

const recorder = () => {
  const calls = []
  return { calls, send: (args, session, cwd) => calls.push([args.join(" "), session, cwd]) }
}

function pi() {
  const r = recorder()
  const handlers = {}
  bridge({ on: (name, fn) => (handlers[name] = fn) }, r.send)
  const ctx = (hasUI = true, id = "p1") => ({ hasUI, cwd: "/work/app", sessionManager: { getSessionId: () => id } })
  return { ...r, fire: (name, event, c = ctx()) => handlers[name]?.(event, c), ctx }
}

test("pi: a settled turn completes, an aborted one is interrupted, a failed one is an error", async () => {
  const p = pi()
  await p.fire("agent_before_settle", { outcome: "completed" })
  await p.fire("agent_settled", { aborted: false })
  await p.fire("agent_before_settle", { outcome: "error" })
  await p.fire("agent_settled", { aborted: false })
  await p.fire("agent_settled", { aborted: true })
  await p.fire("agent_settled", { aborted: false })
  assert.deepEqual(p.calls.map((c) => c[0]), [
    "--kind turn_complete", "--kind error", "--kind interrupted", "--kind turn_complete",
  ])
  assert.deepEqual(p.calls[0].slice(1), ["p1", "/work/app"])
})

test("pi: retried rounds are not final and prompts wait and resolve", async () => {
  const p = pi()
  await p.fire("agent_end", { outcome: "error" })
  await p.fire("turn_end", {})
  assert.deepEqual(p.calls, [])
  await p.fire("ui_prompt_start", { kind: "confirm" })
  await p.fire("ui_prompt_end", {})
  await p.fire("session_shutdown", {})
  assert.deepEqual(p.calls.map((c) => c[0]), [
    "--kind input_required", "--kind turn_complete --activity", "--kind turn_complete --ended",
  ])
})

test("pi: sub-agents and print mode (no UI) stay silent; pi sessions stay separate", async () => {
  const p = pi()
  await p.fire("agent_settled", { aborted: false }, p.ctx(false))
  await p.fire("ui_prompt_start", {}, p.ctx(false))
  assert.deepEqual(p.calls, [])
  await p.fire("agent_before_settle", { outcome: "error" }, p.ctx(true, "a"))
  await p.fire("agent_settled", {}, p.ctx(true, "b"))
  assert.equal(p.calls[0][0], "--kind turn_complete")
})

test("pi: a throwing context never reaches the harness", async () => {
  const p = pi()
  await p.fire("agent_settled", {}, { hasUI: true, get cwd() { throw new Error("boom") } })
  assert.deepEqual(p.calls, [])
})

const opencode = () => {
  const r = recorder()
  const handle = handler(r.send, () => "/fallback")
  const e = (type, data = {}, location) => handle({ type, data, location })
  return { ...r, e }
}

test("opencode: executions map to kinds and use the folder of session.created", () => {
  const o = opencode()
  o.e("session.created", { sessionID: "s1" }, { directory: "/work/app" })
  o.e("session.execution.started", { sessionID: "s1" })
  o.e("session.execution.succeeded", { sessionID: "s1" })
  o.e("session.execution.failed", { sessionID: "s1", error: { message: "secret" } })
  o.e("session.execution.interrupted", { sessionID: "s1", reason: "user" })
  o.e("session.execution.succeeded", { sessionID: "unknown" })
  assert.deepEqual(o.calls, [
    ["--kind turn_complete --activity", "s1", "/work/app"],
    ["--kind turn_complete", "s1", "/work/app"],
    ["--kind error", "s1", "/work/app"],
    ["--kind interrupted", "s1", "/work/app"],
    ["--kind turn_complete", "unknown", "/fallback"],
  ])
})

test("opencode: retries and unrelated events are ignored; permissions wait and resolve", () => {
  const o = opencode()
  o.e("session.retry.scheduled", { sessionID: "s1", attempt: 1 })
  o.e("session.text.delta", { sessionID: "s1" })
  o.e("session.form.requested", { sessionID: "s1" })
  o.e("session.execution.succeeded", {})
  assert.deepEqual(o.calls, [])
  o.e("permission.asked", { sessionID: "s1", resources: ["rm -rf /"] })
  o.e("permission.replied", { sessionID: "s1", reply: "reject" })
  assert.deepEqual(o.calls.map((c) => c[0]), ["--kind permission_required", "--kind turn_complete --activity"])
})

test("opencode: a sub-agent's child session never notifies", () => {
  const o = opencode()
  o.e("session.created", { sessionID: "child", parentID: "s1" })
  o.e("session.execution.started", { sessionID: "child" })
  o.e("session.execution.succeeded", { sessionID: "child" })
  o.e("permission.asked", { sessionID: "child" })
  assert.deepEqual(o.calls, [])
})

test("opencode: malformed events never throw, even when sending fails", () => {
  const handle = handler(() => { throw new Error("x") }, () => "/")
  for (const bad of [null, undefined, 3, {}, { type: "session.execution.failed", data: null }, { type: "session.execution.failed", data: { sessionID: "s" } }])
    assert.doesNotThrow(() => handle(bad))
})

// The real spawn path: a recorder script stands in for aherald.
// The recorder is a `sh` script, so this real-spawn check runs on Unix only.
test("the spawned command receives only the session id and folder on stdin and may be missing", { skip: process.platform === "win32" }, async () => {
  const dir = mkdtempSync(join(tmpdir(), "bridge-"))
  const out = join(dir, "out.txt")
  const exe = join(dir, "aherald")
  writeFileSync(exe, `#!/bin/sh\necho "$@" >> "${out}"\ncat >> "${out}"; echo >> "${out}"\n`)
  chmodSync(exe, 0o755)
  process.env.AHERALD_EXE = exe
  const fresh = await import("../../examples/harnesses/pi-aherald.ts?real")
  const handlers = {}
  fresh.default({ on: (n, f) => (handlers[n] = f) })
  const ctx = { hasUI: true, cwd: "/w", sessionManager: { getSessionId: () => "s9" } }
  await handlers.agent_start({ prompt: "SECRET" }, ctx)
  await handlers.agent_settled({ aborted: false, prompt: "SECRET" }, ctx)
  for (let i = 0; i < 100 && !(existsSync(out) && readFileSync(out, "utf8").split("\n").length > 5); i++) await sleep(50)
  // Calls arrive in the order the events happened, each with the same two fields only.
  assert.equal(readFileSync(out, "utf8"), '--notify pi --kind turn_complete --activity\n{"session_id":"s9","cwd":"/w"}\n--notify pi --kind turn_complete\n{"session_id":"s9","cwd":"/w"}\n')
  // The variable is read at import time, so the missing executable needs a fresh module.
  process.env.AHERALD_EXE = join(dir, "missing")
  const missing = await import("../../examples/harnesses/pi-aherald.ts?missing")
  const h2 = {}
  missing.default({ on: (n, f) => (h2[n] = f) })
  await h2.agent_settled({}, { hasUI: true, cwd: "/w", sessionManager: { getSessionId: () => "s9" } })
  await sleep(100)
})

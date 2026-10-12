import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import plugin from "../assets/harnesses/opencode-report.mjs";

test("native reporting recovers from API failure, heartbeats idle, and clears on disposal", async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "cones-report-test-"));
  const path = join(directory, "session.json");
  let tick, dispose, failed = false, now = 10000;
  t.mock.method(globalThis, "setInterval", (fn) => { tick = fn; return 123; });
  const clear = t.mock.method(globalThis, "clearInterval", () => {});
  t.mock.method(Date, "now", () => now);
  const api = {
    state: {
      ready: true,
      session: {
        get: () => {
          if (failed) throw Error("native API failed");
          return { id: "ses_test", directory, title: "Task", cost: 0 };
        },
        messages: () => [],
        status: () => ({ type: "idle" }),
        permission: () => [],
        question: () => [],
      },
    },
    route: { current: { name: "session", params: { sessionID: "ses_test" } } },
    lifecycle: { onDispose: (fn) => { dispose = fn; } },
  };
  try {
    await plugin.tui(api, { path });
    const initial = readFileSync(path, "utf8");
    assert.equal(JSON.parse(initial).session.id, "ses_test");
    assert.equal(JSON.parse(initial).status.type, "idle");
    failed = true;
    tick();
    assert.equal(existsSync(path), false, "an API error withdraws the old report");
    failed = false;
    tick();
    assert.equal(readFileSync(path, "utf8"), initial, "identical state is republished after recovery");
    utimesSync(path, new Date(0), new Date(0));
    now += 1000;
    tick();
    assert.ok(statSync(path).mtimeMs > 0, "unchanged idle state refreshes liveness");
    api.state.ready = false;
    tick();
    assert.equal(existsSync(path), false);
    api.state.ready = true;
    tick();
    assert.equal(existsSync(path), true);
    dispose();
    assert.equal(clear.mock.calls.length, 1);
    assert.equal(existsSync(path), false);
  } finally {
    dispose?.();
    rmSync(directory, { recursive: true, force: true });
  }
});

test("Pi reports native dialogs, nested waits, switches, failure and reload without handling input", async (t) => {
  const { default: report } = await import("../assets/harnesses/pi-report.mjs");
  const directory = mkdtempSync(join(tmpdir(), "cones-pi-report-test-"));
  const path = join(directory, "session.json");
  const original = process.env.CONES_PI_REPORT;
  process.env.CONES_PI_REPORT = path;
  const handlers = new Map();
  let tick, failed = false, idle = true, id = "aaaaaaaa-1111-4111-8111-111111111111";
  t.mock.method(globalThis, "setInterval", (fn, ms) => { if (ms === 1000) tick = fn; return { unref() {} }; });
  t.mock.method(globalThis, "clearInterval", () => {});
  const pi = { on: (event, handler) => handlers.set(event, handler) };
  const ctx = {
    cwd: directory,
    sessionManager: {
      getSessionId: () => { if (failed) throw Error("native API failed"); return id; },
      getSessionFile: () => undefined,
    },
    isIdle: () => idle,
  };
  const emit = (name) => handlers.get(name)({ type: name }, ctx);
  const read = () => JSON.parse(readFileSync(path, "utf8"));
  try {
    report(pi);
    emit("ui_prompt_start");
    tick();
    assert.equal(read().waiting, true, "a startup dialog may precede this observer's session_start");
    emit("session_start");
    assert.equal(read().waiting, true);
    emit("ui_prompt_end");
    assert.equal(read().pid, process.pid);
    assert.equal(read().session.id, id);
    assert.equal(read().idle, true);
    assert.equal(read().waiting, false);
    emit("ui_prompt_start");
    assert.equal(read().waiting, true);
    emit("ui_prompt_start"); emit("ui_prompt_end");
    assert.equal(read().waiting, true, "ending one nested dialog leaves the other pending");
    emit("ui_prompt_end");
    assert.equal(read().waiting, false);
    idle = false; emit("agent_start");
    assert.equal(read().idle, false);
    failed = true; tick();
    assert.equal(existsSync(path), false, "API failure withdraws the report");
    failed = false; tick();
    assert.equal(read().idle, false);
    utimesSync(path, new Date(0), new Date(0)); tick();
    assert.ok(statSync(path).mtimeMs > 0, "unchanged state refreshes liveness");
    emit("ui_prompt_start"); emit("session_shutdown");
    assert.equal(existsSync(path), false);
    id = "bbbbbbbb-2222-4222-8222-222222222222";
    report(pi); emit("session_start");
    assert.equal(read().session.id, id);
    assert.equal(read().waiting, false, "session replacement clears the old wait");
    assert.ok(!handlers.has("input") && !handlers.has("tool_call"));
    emit("session_shutdown");
    assert.equal(existsSync(path), false);
  } finally {
    handlers.get("session_shutdown")?.();
    if (original === undefined) delete process.env.CONES_PI_REPORT;
    else process.env.CONES_PI_REPORT = original;
    rmSync(directory, { recursive: true, force: true });
  }
});

// A note is `<name>.json` in the directory beside the report; the reporter answers `<name>.done`.
const note = (directory, name, value) => {
  mkdirSync(join(directory, "notes"), { recursive: true });
  writeFileSync(join(directory, "notes", `${name}.json`), typeof value === "string" ? value : JSON.stringify(value));
};
const answer = (directory, name) => JSON.parse(readFileSync(join(directory, "notes", `${name}.done`), "utf8"));

test("Pi submits a note through sendUserMessage and answers every note", async (t) => {
  const { default: report } = await import("../assets/harnesses/pi-report.mjs");
  const directory = mkdtempSync(join(tmpdir(), "cones-pi-notes-test-"));
  const original = process.env.CONES_PI_REPORT;
  process.env.CONES_PI_REPORT = join(directory, "session.json");
  const handlers = new Map(), sent = [];
  let deliver, refuse = false;
  const cleared = [];
  t.mock.method(globalThis, "setInterval", (fn, ms) => { if (ms === 250) deliver = fn; return { ms, unref() {} }; });
  t.mock.method(globalThis, "clearInterval", (timer) => cleared.push(timer?.ms));
  const pi = {
    on: (event, handler) => handlers.set(event, handler),
    sendUserMessage: (text, options) => { if (refuse) throw Error("extension is stale"); sent.push([text, options]); },
  };
  const ctx = { cwd: directory, sessionManager: { getSessionId: () => "here", getSessionFile: () => undefined }, isIdle: () => false };
  try {
    report(pi);
    handlers.get("session_start")({}, ctx);
    note(directory, "a", { text: "rebase" });
    note(directory, "c", "{");
    writeFileSync(join(directory, "notes", "d.tmp"), "unfinished");
    deliver();
    assert.deepEqual(sent, [["rebase", { deliverAs: "steer" }]]);
    assert.deepEqual(answer(directory, "a"), {});
    assert.ok(answer(directory, "c").error, "an unreadable note is answered, not retried");
    assert.deepEqual(readdirSync(join(directory, "notes")).sort(),
      ["a.done", "c.done", "d.tmp"], "a note is consumed once and a partial write is left alone");
    deliver();
    assert.equal(sent.length, 1, "a delivered note is not delivered again");
    refuse = true;
    note(directory, "e", { text: "lost" });
    deliver();
    assert.match(answer(directory, "e").error, /extension is stale/, "a native refusal reaches the sender");
    handlers.get("session_shutdown")();
    assert.ok(cleared.includes(250), "a replaced session stops taking notes");
  } finally {
    if (original === undefined) delete process.env.CONES_PI_REPORT;
    else process.env.CONES_PI_REPORT = original;
    rmSync(directory, { recursive: true, force: true });
  }
});

test("OpenCode submits a note to the shown session through prompt_async and reports a refusal", async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "cones-opencode-notes-test-"));
  let tick, rejection;
  const sent = [];
  t.mock.method(globalThis, "setInterval", (fn) => { tick = fn; return 123; });
  t.mock.method(globalThis, "clearInterval", () => {});
  const api = {
    state: { ready: false },
    route: { current: { name: "session", params: { sessionID: "ses_here" } } },
    lifecycle: { onDispose: () => {} },
    client: { session: { promptAsync: async (input) => { sent.push(input); return rejection ? { error: rejection } : {}; } } },
  };
  const settled = () => new Promise((resolve) => setImmediate(resolve));
  try {
    await plugin.tui(api, { path: join(directory, "session.json") });
    note(directory, "a", { text: "rebase" });
    note(directory, "b", { text: "then push" });
    tick(); tick();
    await settled();
    assert.deepEqual(sent.map((input) => [input.sessionID, input.parts]).sort(), [
      ["ses_here", [{ type: "text", text: "rebase" }]],
      ["ses_here", [{ type: "text", text: "then push" }]],
    ], "overlapping passes deliver each note once");
    assert.deepEqual(answer(directory, "a"), {});
    assert.deepEqual(answer(directory, "b"), {});
    rejection = { name: "NotFoundError" };
    note(directory, "c", { text: "refused" });
    tick();
    await settled();
    assert.match(answer(directory, "c").error, /NotFoundError/);
    api.route.current = { name: "home" };
    note(directory, "d", { text: "nobody is there" });
    tick();
    await settled();
    assert.match(answer(directory, "d").error, /shows no session/);
    assert.equal(sent.length, 3);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

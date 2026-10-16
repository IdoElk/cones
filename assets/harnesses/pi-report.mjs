import { readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

// A private observer loaded on cones-owned native Pi clients. It registers no
// tools or input handlers and never changes a dialog's answer. Notes cones leaves
// beside the report are submitted through Pi's own sendUserMessage.
export default function report(pi) {
  const path = process.env.CONES_PI_REPORT;
  if (!path) return;
  const notes = join(dirname(path), "notes");
  let context, waiting = 0, timer;
  const invalidate = () => { try { rmSync(path, { force: true }); } catch {} };
  const publish = () => {
    try {
      if (!context) { invalidate(); return; }
      const value = {
        version: 1, pid: process.pid,
        session: { id: context.sessionManager.getSessionId(), directory: context.cwd,
          file: context.sessionManager.getSessionFile() ?? null },
        waiting: waiting > 0, idle: context.isIdle(),
      };
      writeFileSync(`${path}.tmp`, JSON.stringify(value), { mode: 0o600 });
      renameSync(`${path}.tmp`, path);
    } catch { invalidate(); }
  };
  // Each `<name>.json` is {text}, or {name} for Pi's own session name, and is answered by
  // `<name>.done`, so the sender learns whether Pi took the note. A note that cannot be
  // removed was withdrawn by its sender and is not delivered.
  const deliver = () => {
    let names;
    try { names = readdirSync(notes); } catch { return; }
    for (const name of names) {
      if (!name.endsWith(".json")) continue;
      const file = join(notes, name), done = `${file.slice(0, -5)}.done`;
      let text, answer = {};
      try { text = readFileSync(file, "utf8"); rmSync(file); } catch { continue; }
      try {
        const note = JSON.parse(text);
        // `/name` calls the same; Pi records it as a session_info entry.
        if (note.name !== undefined) pi.setSessionName(note.name);
        // Pi runs it at once when idle and queues it behind the current assistant turn otherwise.
        else pi.sendUserMessage(note.text, { deliverAs: "steer" });
      } catch (error) { answer = { error: String(error?.message ?? error) }; }
      try {
        writeFileSync(`${done}.tmp`, JSON.stringify(answer), { mode: 0o600 });
        renameSync(`${done}.tmp`, done);
      } catch {}
    }
  };
  const inbox = setInterval(deliver, 250); inbox.unref?.();
  timer = setInterval(publish, 1000); timer.unref?.();
  pi.on("session_start", (_event, ctx) => {
    context = ctx;
    clearInterval(timer); timer = setInterval(publish, 1000); timer.unref?.(); publish();
  });
  pi.on("ui_prompt_start", (_event, ctx) => { context = ctx; waiting++; publish(); });
  pi.on("ui_prompt_end", (_event, ctx) => { context = ctx; waiting = Math.max(0, waiting - 1); publish(); });
  pi.on("agent_start", (_event, ctx) => { context = ctx; publish(); });
  pi.on("agent_settled", (_event, ctx) => { context = ctx; publish(); });
  pi.on("session_shutdown", () => {
    clearInterval(timer); clearInterval(inbox); context = undefined; waiting = 0; invalidate();
  });
}

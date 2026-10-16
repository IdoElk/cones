import { readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

// Loaded only by a cones-owned viewer. Read OpenCode's native TUI state without
// registering tools, changing permissions, or handling the user's input. Notes cones
// leaves beside the report are submitted through OpenCode's own prompt endpoint.
export default {
  id: "cones-session-report",
  async tui(api, options) {
    let previous;
    let published = 0;
    const invalidate = () => {
      previous = undefined;
      published = 0;
      try { rmSync(options.path, { force: true }); } catch {}
    };
    const publish = () => {
      try {
        if (!api.state.ready) {
          invalidate();
          return;
        }
        const route = api.route.current;
        const id = route.name === "session" ? route.params?.sessionID : undefined;
        const session = id ? api.state.session.get(id) : undefined;
        const messages = id ? api.state.session.messages(id) : [];
        const last = messages.findLast((message) =>
          message.role === "assistant" && message.tokens?.output > 0);
        const context = last?.tokens;
        const reply = last ? api.state.part(last.id).find((part) =>
          part.type === "text" && !part.synthetic && !part.ignored && part.text?.trim()) : undefined;
        const native = session ? {
          id: session.id,
          directory: session.directory,
          title: session.title,
          time: session.time,
          model: session.model,
          cost: session.cost,
          tokens: session.tokens,
        } : null;
        const report = JSON.stringify({
          version: 1,
          pid: process.pid,
          session: native,
          status: id ? api.state.session.status(id) : undefined,
          waiting: !id ? undefined
            : api.state.session.permission(id).length ? "permission"
            : api.state.session.question(id).length ? "question"
            : undefined,
          context: context ? {
            input: context.input,
            output: context.output,
            reasoning: context.reasoning,
            cache: context.cache,
          } : undefined,
          last: reply?.text.trim().split("\n", 1)[0].slice(0, 512),
        });
        const now = Date.now();
        // An unchanged idle session still proves the reporter is alive.
        if (report === previous && now - published < 1000) return;
        const temporary = `${options.path}.tmp`;
        writeFileSync(temporary, report, { mode: 0o600 });
        renameSync(temporary, options.path);
        previous = report;
        published = now;
      } catch {
        // Reporting must not interrupt OpenCode's editor or execution.
        invalidate();
      }
    };
    // Each `<name>.json` is {text}, or {name} for the session title, and is answered by
    // `<name>.done`, so the sender learns whether OpenCode took the note. A note that cannot be
    // removed was withdrawn by its sender, or taken by an earlier pass still awaiting
    // OpenCode, and is not delivered.
    const notes = join(dirname(options.path), "notes");
    const deliver = async () => {
      let names;
      try { names = readdirSync(notes); } catch { return; }
      for (const name of names) {
        if (!name.endsWith(".json")) continue;
        const file = join(notes, name), done = `${file.slice(0, -5)}.done`;
        let text, answer = {};
        try { text = readFileSync(file, "utf8"); rmSync(file); } catch { continue; }
        try {
          const note = JSON.parse(text);
          const route = api.route.current;
          const id = route.name === "session" ? route.params?.sessionID : undefined;
          if (!id) {
            answer = { error: "this OpenCode shows no session" };
          } else if (note.name !== undefined) {
            // What OpenCode's own rename dialog calls.
            const result = await api.client.session.update({ sessionID: id, title: note.name });
            if (result?.error) answer = { error: JSON.stringify(result.error) };
          } else {
            // The native prompt endpoint: OpenCode queues it behind a running turn.
            const result = await api.client.session.promptAsync({
              sessionID: id, parts: [{ type: "text", text: note.text }],
            });
            if (result?.error) answer = { error: JSON.stringify(result.error) };
          }
        } catch (error) { answer = { error: String(error?.message ?? error) }; }
        try {
          writeFileSync(`${done}.tmp`, JSON.stringify(answer), { mode: 0o600 });
          renameSync(`${done}.tmp`, done);
        } catch {}
      }
    };
    const timer = setInterval(() => { deliver(); publish(); }, 250);
    api.lifecycle.onDispose(() => {
      clearInterval(timer);
      invalidate();
    });
    publish();
  },
};

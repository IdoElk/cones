#!/usr/bin/env node
// A pi, OpenCode or terminal-only client for the note-delivery tests. No model is contacted.
//
// Installed under a fixture HOME as `.local/bin/pi`, `.local/bin/opencode` or
// `.local/bin/gemini`, where `harness::launch_path` looks first; `ps` shows `node <path>/pi`,
// a form process discovery recognises. As pi or OpenCode it loads the reporter cones passed it,
// the shipped file and not a copy, and gives it the part of the native extension API the
// reporter uses. Every user message the reporter submits through that API is appended to
// `$HOME/reporter-fixture/<pid>.jsonl`, after a first line naming the session and its
// arguments, as is every name it sets.
//
// `FAKE_REPORTER=deaf` starts without loading the reporter, as a client does when its
// extensions are disabled.
import { appendFileSync, mkdirSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";

const args = process.argv.slice(2);
const name = basename(process.argv[1]);
if (args.includes("--version")) { console.log("0.0.0-fixture"); process.exit(0); }
if (args.includes("--help")) {
  const flags = "--prompt --session --model --prompt-interactive fork";
  console.log(flags); console.error(flags); process.exit(0);
}

const directory = join(process.env.HOME, "reporter-fixture");
mkdirSync(directory, { recursive: true });
const record = (value) =>
  appendFileSync(join(directory, `${process.pid}.jsonl`), `${JSON.stringify(value)}\n`);
const deaf = process.env.FAKE_REPORTER === "deaf";
const cwd = process.cwd();

if (name === "pi") {
  const session = crypto.randomUUID();
  record({ harness: name, session, args });
  if (!deaf) {
    const handlers = new Map();
    const { default: load } = await import(args[args.indexOf("--extension") + 1]);
    load({
      on: (event, handler) => handlers.set(event, handler),
      sendUserMessage: (text, options) => record({ text, options }),
      setSessionName: (name) => record({ name }),
    });
    handlers.get("session_start")({ type: "session_start" }, {
      cwd,
      sessionManager: { getSessionId: () => session, getSessionFile: () => undefined },
      isIdle: () => true,
    });
  }
} else if (name === "opencode") {
  const session = `ses_fixture${process.pid}`;
  record({ harness: name, session, args });
  if (!deaf) {
    const config = JSON.parse(readFileSync(process.env.OPENCODE_TUI_CONFIG, "utf8"));
    const [path, options] = config.plugin.at(-1);
    const { default: plugin } = await import(path);
    const now = Date.now();
    await plugin.tui({
      state: {
        ready: true,
        session: {
          get: () => ({ id: session, directory: cwd, title: "fixture", cost: 0,
            time: { created: now, updated: now } }),
          messages: () => [],
          status: () => ({ type: "idle" }),
          permission: () => [],
          question: () => [],
        },
        part: () => [],
      },
      route: { current: { name: "session", params: { sessionID: session } } },
      lifecycle: { onDispose: () => {} },
      client: { session: {
        promptAsync: async (input) => { record(input); return {}; },
        update: async (input) => { record({ update: input }); return {}; },
      } },
    }, options);
  }
} else {
  record({ harness: name });
}
// The reporter's timers do not hold the process open; a terminal client stays until stopped.
setInterval(() => {}, 1000);

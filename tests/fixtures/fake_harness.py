#!/usr/bin/python3
"""Disposable launch targets for the detached-launch tests. No model is contacted.

Installed under a fixture HOME as `.local/bin/claude`, `.local/bin/pi` and `.local/bin/codex`,
which is where `harness::launch_path` looks first. The shebang names the interpreter directly so `ps` shows
`python3 <path>/pi`, the form pi's process discovery recognises.

`FAKE_LAUNCH` selects the failure being exercised:
  ok       start normally (default)
  no-id    start, print an unusable line instead of a background id
  fail     refuse to start, with a diagnostic on stderr
  twice    codex only: report two threads for the one prompt
"""
import json
import os
import pathlib
import signal
import subprocess
import sys
import time
import uuid

name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
mode = os.environ.get("FAKE_LAUNCH", "ok")

if name == "claude":
    if "--help" in args:
        print("--bg  start a background session\nattach  join one")
        sys.exit(0)
    if mode == "fail":
        print("fixture refused this launch", file=sys.stderr)
        sys.exit(3)
    session = str(uuid.uuid4())
    # A background launch hands the session to something that outlives the launcher, and a
    # registry entry whose pid is dead is not a session. This stand-in daemon is that process;
    # the test kills every pid it finds in this registry.
    daemon = subprocess.Popen(["/bin/sleep", "600"], start_new_session=True,
                              stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                              stderr=subprocess.DEVNULL)
    home = pathlib.Path(os.environ["CLAUDE_CONFIG_DIR"])
    registry = home / "sessions"
    registry.mkdir(parents=True, exist_ok=True)
    (registry / f"{session}.json").write_text(json.dumps({
        "pid": daemon.pid, "sessionId": session, "cwd": os.getcwd(),
        "startedAt": int(time.time() * 1000), "kind": "bg", "status": "idle",
        "jobId": session[:8], "entrypoint": "cli",
    }))
    if mode == "no-id":
        print("started, somewhere")
        sys.exit(0)
    print(f"backgrounded · {session[:8]} (idle)")
    sys.exit(0)

if name == "pi":
    if "--version" in args:
        print("0.0.0-fixture")
        sys.exit(0)
    if mode == "fail":
        print("fixture refused this launch", file=sys.stderr)
        sys.exit(3)
    # A terminal client: hold the pty, report the prompt it was given, and wait to be
    # stopped. This is what a detached launch has to keep alive past its launcher.
    sys.stdout.write(f"pi fixture ready: {' '.join(args)}\r\n")
    sys.stdout.flush()
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    while True:
        time.sleep(0.05)

if name == "codex":
    home = pathlib.Path(os.environ["CODEX_HOME"])
    locks = home / "thread-writer-locks"
    if args == ["app-server", "daemon", "start"]:
        # The daemon is what holds a thread, and cones reads that from the lock files its
        # pid keeps open. This stand-in opens every lock that appears and holds it.
        pid_file = home / "app-server-daemon" / "app-server.pid"
        if not pid_file.exists():
            locks.mkdir(parents=True, exist_ok=True)
            pid_file.parent.mkdir(parents=True, exist_ok=True)
            daemon = subprocess.Popen(
                [sys.executable, "-c",
                 "import glob, sys, time\nheld = {}\nwhile True:\n"
                 "    for p in glob.glob(sys.argv[1] + '/*.lock'):\n"
                 "        held.setdefault(p, open(p))\n    time.sleep(0.02)", str(locks)],
                start_new_session=True, stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            pid_file.write_text(json.dumps({"pid": daemon.pid}))
        print(json.dumps({"socketPath": str(home / "control.sock"), "cliVersion": "0.162.1"}))
        sys.exit(0)
    # A remote client: `--remote ADDR -C CWD [flags] -- PROMPT`. The daemon makes the thread
    # on the first turn and tells the launcher nothing, so the rollout is all cones can read.
    cwd = args[args.index("-C") + 1]
    prompt = args[args.index("--") + 1] if "--" in args else None
    made = []
    for _ in range(2 if mode == "twice" else 1 if prompt else 0):
        thread = str(uuid.uuid4())
        made.append(thread)
        now = time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime()) + ".%03dZ" % (time.time() % 1 * 1000)
        lines = [
            {"timestamp": now, "ordinal": 0, "type": "session_meta", "payload": {
                "session_id": thread, "id": thread, "timestamp": now, "cwd": cwd,
                "originator": "codex-tui", "cli_version": "0.162.1", "source": "vscode"}},
            {"timestamp": now, "ordinal": 1, "type": "event_msg",
             "payload": {"type": "task_started", "turn_id": "t"}},
            {"timestamp": now, "ordinal": 2, "type": "event_msg", "payload": {
                "type": "item_completed", "thread_id": thread, "turn_id": "t",
                "item": {"type": "UserMessage", "id": "u",
                         "content": [{"type": "text", "text": prompt, "text_elements": []}]}}},
        ]
        day = home / "sessions" / time.strftime("%Y/%m/%d", time.gmtime())
        day.mkdir(parents=True, exist_ok=True)
        # Written whole and renamed: cones reads a rollout's first line once.
        rollout = day / f"rollout-{now[:19].replace(':', '-')}-{thread}.jsonl"
        partial = rollout.with_suffix(".tmp")
        partial.write_text("".join(json.dumps(line) + "\n" for line in lines))
        partial.rename(rollout)
        with open(home / "fixture-threads", "a") as reported:
            reported.write(thread + "\n")
    # A thread is a row once the daemon holds its lock, so both become rows together.
    for thread in made:
        (locks / f"{thread}.lock").touch()
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    while True:
        time.sleep(0.05)

print(f"unexpected fixture name {name}", file=sys.stderr)
sys.exit(2)

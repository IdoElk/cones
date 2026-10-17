#!/usr/bin/python3
"""Disposable claude, codex, pi and opencode CLIs for the side-question tests. No model is
contacted.

Installed under a fixture HOME as `.local/bin/<name>`, where `harness::launch_path` looks
first. Each run appends `{name, args, cwd}` to `$HOME/calls.jsonl`, writes its pid to
`$HOME/<name>.pid`, and answers the way the installed CLI was seen to: Claude's JSON result
report, Codex's last-message file, pi's printed reply and OpenCode's JSON events. pi saves its
fork under the `--session-dir` it was given, and OpenCode names a fork of its own in its events,
so a test can see each removed.

`FAKE_BTW` selects the failure being exercised:
  ok      answer (default)
  error   claude only: report an error result and exit 1
  slow    answer only after ten minutes; OpenCode names its fork first
"""
import json
import os
import pathlib
import sys
import time

name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
mode = os.environ.get("FAKE_BTW", "ok")
home = pathlib.Path(os.environ["HOME"])

with open(home / "calls.jsonl", "a") as calls:
    calls.write(json.dumps({"name": name, "args": args, "cwd": os.getcwd()}) + "\n")
(home / f"{name}.pid").write_text(str(os.getpid()))


def after(flag):
    return args[args.index(flag) + 1]


if "--help" in args:
    # OpenCode prints its help on stderr.
    print("--fork-session --no-session-persistence --permission-mode --print --fork --session-dir "
          "--tools --ephemeral --output-last-message --sandbox --agent --format fork",
          file=sys.stderr if name == "opencode" else sys.stdout)
    sys.exit(0)

if name == "opencode" and args[:2] == ["session", "delete"]:
    print(f"Session {args[2]} deleted")
    sys.exit(0)

question = args[-1]
if mode == "slow" and name != "opencode":
    time.sleep(600)

if name == "claude":
    if mode == "error":
        print(json.dumps({"type": "result", "is_error": True, "result": "Prompt is too long"}))
        sys.exit(1)
    print(json.dumps({"type": "result", "is_error": False,
                      "result": f"claude answers {question!r} from {after('--resume')}\n"}))
elif name == "codex":
    pathlib.Path(after("--output-last-message")).write_text(
        f"codex answers {question!r} from {args[-2]}")
elif name == "pi":
    forks = pathlib.Path(after("--session-dir"))
    (forks / "fork.jsonl").write_text(json.dumps({"type": "session", "id": "fork"}) + "\n")
    print(f"pi answers {question!r} from {pathlib.Path(after('--fork')).name}")
elif name == "opencode":
    source, fork = after("--session"), "ses_fork1"
    for event in [
        # A line naming the source, as a resumed session's own events may.
        {"type": "step_start", "sessionID": source},
        {"type": "step_start", "sessionID": fork},
        {"type": "text", "sessionID": fork, "part": {"type": "text", "text": "Reading"}},
        {"type": "tool_use", "sessionID": fork, "part": {"type": "tool"}},
        {"type": "step_start", "sessionID": fork},
        {"type": "text", "sessionID": fork,
         "part": {"type": "text", "text": f"opencode answers {question!r} from {source}"}},
        {"type": "step_finish", "sessionID": fork, "part": {"reason": "stop"}},
    ]:
        print(json.dumps(event), flush=True)
        if mode == "slow" and event["sessionID"] == fork:
            time.sleep(600)
else:
    print(f"unexpected fixture name {name}", file=sys.stderr)
    sys.exit(2)

#!/usr/bin/python3
"""A stand-in for a harness whose session lives only in its terminal. No model is contacted.

Installed under a fixture HOME as `.local/bin/gemini`. It answers the launch probe, fills more
than one screen so some of its output scrolls off, then echoes what it is typed, which is all
a terminal-only client shows cones: a screen and nothing on disk.
"""
import signal
import sys

args = sys.argv[1:]
if "--help" in args:
    print("--prompt-interactive  start with a prompt\n--model  choose a model")
    sys.exit(0)
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
for row in range(1, 101):
    print(f"row-{row:03}")
print(f"fixture ready: {' '.join(args)}", flush=True)
for line in sys.stdin:
    print("RECEIVED:" + line.strip(), flush=True)

//! Sessions in native homes other than the harness's own: the ones `homes` in jobs.yaml names
//! and the ones live clients of this user run against. Each is listed, read and controlled in
//! the home that recorded it. A fake native CLI stands in for Claude; no model is contacted.
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

/// Answers `stop` and records the id and the native home it was given, so a stop sent to the
/// wrong home is visible in the wrong home's log.
const FAKE: &str = r#"#!/bin/sh
if [ "$1" = stop ] && [ "$2" = --help ]; then
    echo "Usage: claude stop <id>"
    exit 0
fi
home=${CLAUDE_CONFIG_DIR:?a stop must name the native home of its session}
printf '%s\n' "$* $home" >> "$home/calls.log"
echo "stopped $2"
"#;

struct Fixture {
    root: tempfile::TempDir,
    children: Vec<Child>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("cones-homes-")
            .tempdir_in("/tmp")
            .unwrap();
        let bin = root.path().join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("claude"), FAKE).unwrap();
        fs::set_permissions(bin.join("claude"), fs::Permissions::from_mode(0o700)).unwrap();
        for dir in [".claude/sessions", "state"] {
            fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        fs::write(root.path().join("jobs.yaml"), "version: 4\njobs: []\n").unwrap();
        Self {
            root,
            children: Vec::new(),
        }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    /// A process cones may read about and must not touch.
    fn bystander(&mut self) -> u32 {
        let child = Command::new("/bin/sleep").arg("600").spawn().unwrap();
        let pid = child.id();
        self.children.push(child);
        pid
    }

    /// A Claude client as `ps` shows one, running against `home` under `user`'s HOME.
    fn client(&mut self, user: &Path, home: &Path) -> u32 {
        let program = self.path().join("standin/claude");
        fs::create_dir_all(program.parent().unwrap()).unwrap();
        fs::write(
            &program,
            "#!/usr/bin/python3\nimport time\ntime.sleep(600)\n",
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let child = Command::new(&program)
            .env("HOME", user)
            .env("CLAUDE_CONFIG_DIR", home)
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        self.children.push(child);
        pid
    }

    /// A live registry entry in `home`, with the conversation Claude keeps beside it.
    fn session(&self, home: &Path, id: &str, kind: &str, pid: u32, said: &str) {
        fs::create_dir_all(home.join("sessions")).unwrap();
        fs::write(
            home.join(format!("sessions/{id}.json")),
            json!({"sessionId": id, "pid": pid, "kind": kind, "cwd": self.path(), "status": "busy"})
                .to_string(),
        )
        .unwrap();
        let project = self
            .path()
            .to_string_lossy()
            .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        let dir = home.join("projects").join(project);
        fs::create_dir_all(&dir).unwrap();
        let line = json!({
            "type": "user", "uuid": "11111111-1111-4111-8111-111111111111",
            "sessionId": id, "cwd": self.path(), "timestamp": "2026-10-13T09:00:00Z",
            "message": {"role": "user", "content": said},
        });
        fs::write(dir.join(format!("{id}.jsonl")), format!("{line}\n")).unwrap();
    }

    fn cones(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cones"))
            .env("HOME", self.path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env_remove("PI_CODING_AGENT_DIR")
            .args(["--jobs", self.path().join("jobs.yaml").to_str().unwrap()])
            .args(["--state-dir", self.path().join("state").to_str().unwrap()])
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.cones(args);
        assert!(
            out.status.success(),
            "cones {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// The Claude sessions `cones ls` reports, by id.
    fn sessions(&self) -> Vec<Value> {
        self.ok(&["ls", "--json"])
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|row| row["session"]["harness"] == "claude")
            .map(|row| row["session"].clone())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn ids(rows: &[Value]) -> Vec<&str> {
    let mut ids: Vec<_> = rows
        .iter()
        .filter_map(|r| r["session_id"].as_str())
        .collect();
    ids.sort();
    ids
}

const OWN: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const TASK: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const OTHER: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

/// A home `homes.claude` names is read beside Claude's own: its session is a full row, its
/// conversation is what `cones show` prints, and a stop goes to the home that holds it.
#[test]
fn a_configured_home_is_listed_read_and_stopped_in_its_own_home() {
    let mut f = Fixture::new();
    let own = f.path().join(".claude");
    let task = f.path().join("task-home");
    let (first, second) = (f.bystander(), f.bystander());
    f.session(&own, OWN, "bg", first, "tidy the changelog");
    f.session(&task, TASK, "bg", second, "rotate the staging keys");
    assert_eq!(ids(&f.sessions()), [OWN], "an unnamed home is not read");

    let listed = format!("[\"{}\"]", task.display());
    f.ok(&["config", "set", "homes.claude", &listed]);
    let rows = f.sessions();
    assert_eq!(ids(&rows), [OWN, TASK]);
    let row = rows.iter().find(|r| r["session_id"] == TASK).unwrap();
    assert_eq!(row["state"], "active", "{row}");
    assert_eq!(row["title"], "rotate the staging keys", "{row}");
    assert_eq!(
        row["transcript_path"]
            .as_str()
            .map(PathBuf::from)
            .map(|p| p.starts_with(&task)),
        Some(true),
        "{row}"
    );

    let shown = f.ok(&["show", TASK]);
    assert!(shown.contains("rotate the staging keys"), "{shown}");

    f.ok(&["stop", TASK]);
    f.ok(&["stop", OWN]);
    let log = |home: &Path| fs::read_to_string(home.join("calls.log")).unwrap_or_default();
    // `claude stop` takes the short id, the first block of the UUID.
    assert_eq!(log(&task), format!("stop bbbbbbbb {}\n", task.display()));
    assert_eq!(log(&own), format!("stop aaaaaaaa {}\n", own.display()));

    // Taking the home out of the file takes its sessions out of the list.
    f.ok(&["config", "unset", "homes.claude"]);
    assert_eq!(ids(&f.sessions()), [OWN]);
}

/// A relative home would be read against whatever directory cones was started in.
#[test]
fn a_home_that_is_not_absolute_or_names_no_harness_is_refused() {
    let f = Fixture::new();
    for (key, value) in [
        ("homes.claude", "[task-home]"),
        ("homes.nothing", "[/tmp/x]"),
    ] {
        let out = f.cones(&["config", "set", key, value]);
        assert!(!out.status.success(), "{key} {value} was accepted");
    }
    assert_eq!(
        fs::read_to_string(f.path().join("jobs.yaml")).unwrap(),
        "version: 4\njobs: []\n",
        "a refused setting changed the file"
    );
}

/// A live client of this user that points Claude at another home is this fleet's: cones reads
/// that home with no configuration. A client under another HOME is another fleet's and stays out.
#[test]
fn a_live_client_of_this_user_names_a_home_that_is_then_read() {
    let mut f = Fixture::new();
    let (task, other) = (f.path().join("task-home"), f.path().join("other-home"));
    let elsewhere = f.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let user = f.path().to_owned();
    let mine = f.client(&user, &task);
    let theirs = f.client(&elsewhere, &other);
    f.session(&task, TASK, "interactive", mine, "rotate the staging keys");
    f.session(&other, OTHER, "interactive", theirs, "not this fleet");

    // The python3 shim hides its environment until it re-executes the real interpreter.
    let deadline = Instant::now() + Duration::from_secs(30);
    let rows = loop {
        let rows = f.sessions();
        if !rows.is_empty() {
            break rows;
        }
        assert!(
            Instant::now() < deadline,
            "the client's home was never read"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    assert_eq!(ids(&rows), [TASK]);
    assert_eq!(rows[0]["pid"], mine);
    assert_eq!(rows[0]["title"], "rotate the staging keys");
    let shown = f.ok(&["show", TASK]);
    assert!(shown.contains("rotate the staging keys"), "{shown}");
    // By now the other client's environment is readable too, and it is still not listed.
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(ids(&f.sessions()), [TASK]);
}

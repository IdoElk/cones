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
        self.stand_in(
            "claude",
            &[],
            &[("HOME", user), ("CLAUDE_CONFIG_DIR", home)],
        )
    }

    /// A client of `harness` as `ps` shows one, started in the fixture's folder.
    fn stand_in(&mut self, harness: &str, args: &[&str], env: &[(&str, &Path)]) -> u32 {
        let program = self.path().join("standin").join(harness);
        fs::create_dir_all(program.parent().unwrap()).unwrap();
        fs::write(
            &program,
            "#!/usr/bin/python3\nimport time\ntime.sleep(600)\n",
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let mut command = Command::new(&program);
        command
            .args(args)
            .current_dir(self.path().canonicalize().unwrap())
            .stdin(Stdio::null());
        for (name, value) in env {
            command.env(name, value);
        }
        let child = command.spawn().unwrap();
        let pid = child.id();
        self.children.push(child);
        pid
    }

    /// The rows `cones ls` reports for `harness`, once one of them satisfies `ready`. The
    /// python3 shim hides a stand-in's environment until it re-executes the real interpreter.
    fn rows_when(&self, harness: &str, ready: impl Fn(&Value) -> bool) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let rows: Vec<Value> = self
                .ok(&["ls", "--json"])
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .filter(|row| row["session"]["harness"] == harness)
                .map(|row| row["session"].clone())
                .collect();
            if rows.iter().any(&ready) || Instant::now() > deadline {
                return rows;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
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

/// pi keeps a conversation in a file under its home, named after the folder it runs in. A
/// client pointed at another home is attributed to the file in that home, so its row has the
/// conversation's title and `cones show` reads it.
#[test]
fn a_pi_client_in_another_home_is_a_row_with_its_conversation() {
    let mut f = Fixture::new();
    let task = f.path().join("task-pi");
    let cwd = f.path().canonicalize().unwrap();
    let user = f.path().to_owned();
    let pid = f.stand_in(
        "pi",
        &[],
        &[("HOME", &user), ("PI_CODING_AGENT_DIR", &task)],
    );
    let folder = cwd
        .to_string_lossy()
        .trim_start_matches('/')
        .replace('/', "-");
    let dir = task.join(format!("sessions/--{folder}--"));
    fs::create_dir_all(&dir).unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let header = json!({"type": "session", "version": 3, "id": TASK, "timestamp": now, "cwd": cwd});
    let said = json!({"type": "message", "id": "m1", "parentId": null, "timestamp": now,
        "message": {"role": "user", "content": [{"type": "text", "text": "rotate the staging keys"}]}});
    fs::write(
        dir.join(format!("{TASK}.jsonl")),
        format!("{header}\n{said}\n"),
    )
    .unwrap();

    let rows = f.rows_when("pi", |row| row["pid"] == pid && row["title"].is_string());
    let mine: Vec<_> = rows.iter().filter(|row| row["pid"] == pid).collect();
    assert_eq!(mine.len(), 1, "{rows:?}");
    assert_eq!(mine[0]["title"], "rotate the staging keys", "{}", mine[0]);
    assert_eq!(
        mine[0]["transcript_path"]
            .as_str()
            .map(|p| Path::new(p).starts_with(&task)),
        Some(true),
        "{}",
        mine[0]
    );
    let shown = f.ok(&["show", TASK]);
    assert!(shown.contains("rotate the staging keys"), "{shown}");
}

/// OpenCode keeps every conversation in one database under its data home. A client pointed
/// at another data home, showing a session that database holds, is a row with that session's
/// title, read from that database.
#[test]
fn an_opencode_client_in_another_data_home_is_a_row_with_its_session() {
    let mut f = Fixture::new();
    let data = f.path().join("task-xdg");
    let cwd = f.path().canonicalize().unwrap();
    fs::create_dir_all(data.join("opencode")).unwrap();
    let db = data.join("opencode/opencode.db");
    let sql = include_str!("../assets/harnesses/fixtures/opencode.sql")
        .replace("/fixture", &cwd.to_string_lossy());
    let mut sqlite = Command::new("sqlite3")
        .arg(&db)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(sqlite.stdin.as_mut().unwrap(), sql.as_bytes()).unwrap();
    assert!(sqlite.wait().unwrap().success());
    // OpenCode reads `--session` from its own arguments, so the stand-in must be named
    // opencode itself: a script's interpreter would come first on the command line.
    let mut client = {
        use std::os::unix::process::CommandExt;
        Command::new("node")
            .arg0("opencode")
            .args(["-", "--session", "ses_fixture"])
            .current_dir(&cwd)
            .env("HOME", f.path())
            .env("XDG_DATA_HOME", &data)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap()
    };
    std::io::Write::write_all(
        &mut client.stdin.take().unwrap(),
        b"setTimeout(() => {}, 600000)",
    )
    .unwrap();
    let pid = client.id();
    f.children.push(client);

    let rows = f.rows_when("opencode", |row| {
        row["pid"] == pid && row["title"].is_string()
    });
    let mine: Vec<_> = rows.iter().filter(|row| row["pid"] == pid).collect();
    assert_eq!(mine.len(), 1, "{rows:?}");
    assert_eq!(mine[0]["title"], "Named OpenCode", "{}", mine[0]);
    assert_eq!(
        mine[0]["transcript_path"].as_str().map(PathBuf::from),
        Some(db),
        "{}",
        mine[0]
    );
}

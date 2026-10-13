//! Detached `cones launch` against disposable homes and fixture launch targets.
//! No model is contacted and no native home outside the fixture is read or written.
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};

/// A fixture HOME with its own native homes, state directory and launch targets.
struct Fixture {
    root: tempfile::TempDir,
    jobs: PathBuf,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        // Short prefix: the terminal host puts its socket under /tmp and macOS bounds the path.
        let root = tempfile::Builder::new()
            .prefix("cones-launch-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = root.path();
        for dir in ["state", "claude", "pi", "work", ".local/bin"] {
            fs::create_dir_all(path.join(dir)).unwrap();
        }
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_harness.py");
        for name in ["claude", "pi", "codex", "kimi"] {
            let target = path.join(".local/bin").join(name);
            std::os::unix::fs::symlink(&fixture, &target).unwrap();
        }
        let jobs = path.join("jobs.yaml");
        // Only the two harnesses under test are offered, so no other native home is scanned
        // and the machine's own clients cannot reach these assertions.
        fs::write(
            &jobs,
            "version: 4\ndefaults:\n  codex_enabled: false\n  opencode_enabled: false\n  \
             gemini_enabled: false\n  cursor_enabled: false\n  copilot_enabled: false\n  \
             amp_enabled: false\n  droid_enabled: false\n  kimi_enabled: false\njobs: []\n",
        )
        .unwrap();
        let work = path.join("work").canonicalize().unwrap();
        Self { root, jobs, work }
    }

    /// The same fixture offering Codex alone, with a native home of its own. That home makes
    /// cones scan the machine's Codex clients too, so these tests select rows by the ids and
    /// the folder they created.
    fn codex() -> Self {
        let f = Self::offering("codex");
        fs::create_dir_all(f.codex_home()).unwrap();
        f
    }

    /// The same fixture offering `harness` alone.
    fn offering(harness: &str) -> Self {
        let f = Self::new();
        let offered = fs::read_to_string(&f.jobs).unwrap().replace(
            &format!("{harness}_enabled: false"),
            "claude_enabled: false\n  pi_enabled: false",
        );
        fs::write(&f.jobs, offered).unwrap();
        f
    }

    /// Copilot as npm installs it: a loader on the launch path that waits on the native
    /// binary of a platform package, which `platform` places relative to the loader's own.
    fn copilot(platform: &str) -> Self {
        let f = Self::offering("copilot");
        let package = f.path().join(".local/lib/node_modules/@github/copilot");
        let native = package.join(platform);
        fs::create_dir_all(&native).unwrap();
        let loader = package.join("npm-loader.js");
        fs::write(
            &loader,
            "#!/usr/bin/python3\nimport glob, os, subprocess, sys\n\
             here = os.path.dirname(os.path.realpath(__file__))\n\
             native = glob.glob(here + '/../copilot-darwin-*/copilot') + \
             glob.glob(here + '/node_modules/@github/copilot-darwin-*/copilot')\n\
             sys.exit(subprocess.call([native[0], *sys.argv[1:]]))\n",
        )
        .unwrap();
        fs::set_permissions(&loader, fs::Permissions::from_mode(0o755)).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_harness.py");
        std::os::unix::fs::symlink(fixture, native.join("copilot")).unwrap();
        std::os::unix::fs::symlink(&loader, f.path().join(".local/bin/copilot")).unwrap();
        f
    }

    fn codex_home(&self) -> PathBuf {
        self.path().join("codex")
    }

    /// The threads the fixture's daemon made, in the order it made them.
    fn codex_threads(&self) -> Vec<String> {
        fs::read_to_string(self.codex_home().join("fixture-threads"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn state(&self) -> PathBuf {
        self.path().join("state")
    }

    fn cones(&self, mode: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cones"))
            .args([
                "--jobs",
                self.jobs.to_str().unwrap(),
                "--state-dir",
                self.state().to_str().unwrap(),
            ])
            .args(args)
            .env("HOME", self.path())
            .env("FAKE_LAUNCH", mode)
            .env("CONES_TEST_FAST", "1")
            .env("CLAUDE_CONFIG_DIR", self.path().join("claude"))
            .env("PI_CODING_AGENT_DIR", self.path().join("pi"))
            .env("CODEX_HOME", self.codex_home())
            .env_remove("OPENCODE_DB")
            .env_remove("OPENCODE_TUI_CONFIG")
            .output()
            .unwrap()
    }

    fn launch(&self, mode: &str, harness: &str, prompt: &str) -> Output {
        self.cones(
            mode,
            &[
                "launch",
                "--dir",
                self.work.to_str().unwrap(),
                "--harness",
                harness,
                prompt,
            ],
        )
    }

    /// The rows `cones ls` reports for this fixture's folder.
    fn rows(&self) -> Vec<Value> {
        let out = self.cones(
            "ok",
            &["ls", "--dir", self.work.to_str().unwrap(), "--json"],
        );
        assert!(out.status.success(), "{}", stderr(&out));
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn recovery(&self) -> Vec<Value> {
        let path = self.state().join("launches.jsonl");
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The stand-in daemons behind this fixture's background records.
    fn background_pids(&self) -> Vec<u32> {
        let Ok(entries) = fs::read_dir(self.path().join("claude/sessions")) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| {
                let record: Value = serde_json::from_slice(&fs::read(e.ok()?.path()).ok()?).ok()?;
                record["pid"].as_u64().map(|pid| pid as u32)
            })
            .collect()
    }

    /// Every terminal host this fixture owns, so each test cleans up what it started.
    fn hosted_pids(&self) -> Vec<u32> {
        let Ok(entries) = fs::read_dir(self.state().join("terminals")) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| {
                let path = e.ok()?.path();
                (path.extension()? == "json").then_some(())?;
                let record: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
                record["session"]["pid"].as_u64().map(|pid| pid as u32)
            })
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // A detached launch is meant to outlive its launcher, so the test has to end it.
        let daemon = fs::read(self.codex_home().join("app-server-daemon/app-server.pid"))
            .ok()
            .and_then(|pid| serde_json::from_slice::<Value>(&pid).ok())
            .and_then(|pid| pid["pid"].as_u64())
            .map(|pid| pid as u32);
        for pid in self
            .hosted_pids()
            .into_iter()
            .chain(self.background_pids())
            .chain(daemon)
        {
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        }
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_owned()
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("{what} did not happen within 10s");
}

#[test]
fn a_background_launch_returns_the_exact_session_id_the_roster_carries() {
    let f = Fixture::new();
    let out = f.launch("ok", "claude", "count the flaky tests");
    assert!(out.status.success(), "{}", stderr(&out));
    let id = stdout(&out);

    // Exact, not the eight-character id the harness printed: the whole session id.
    assert_eq!(id.len(), 36, "expected a full session id, got {id:?}");
    assert!(
        uuid::Uuid::parse_str(&id).is_ok(),
        "{id} is not a session id"
    );
    assert!(
        stderr(&out).contains("native session id"),
        "the reply must say which kind of id it returned: {}",
        stderr(&out)
    );

    // Accepted by the command an agent reads the folder with.
    let rows = f.rows();
    let row = rows
        .iter()
        .find(|r| r["session"]["session_id"] == id.as_str())
        .unwrap_or_else(|| panic!("cones ls does not carry {id}: {rows:?}"));
    assert_eq!(row["session"]["harness"], "claude");
    assert_eq!(
        row["session"]["cwd"].as_str().map(PathBuf::from),
        Some(f.work.clone())
    );
}

#[test]
fn recovery_is_written_before_the_launch_and_completed_with_the_resolved_id() {
    let f = Fixture::new();
    let id = stdout(&f.launch("ok", "claude", "fix the parser"));
    let records = f.recovery();
    let submitted = &records[0];
    assert_eq!(submitted["event"], "launch.submitted");
    assert_eq!(submitted["level"], "recovery");
    assert_eq!(submitted["data"]["harness"], "claude");
    assert_eq!(submitted["data"]["prompt"], "fix the parser");
    assert_eq!(
        submitted["data"]["cwd"].as_str().map(PathBuf::from),
        Some(f.work.clone())
    );
    // The submitted record predates any native id, so it carries only cones' own key.
    assert!(submitted["data"]["session_id"].is_null());

    let identified = &records[1];
    assert_eq!(identified["event"], "launch.identified");
    assert_eq!(identified["data"]["session_id"], id.as_str());
    assert_eq!(
        identified["data"]["operation_id"], submitted["data"]["operation_id"],
        "both records must name the same launch"
    );
    assert_eq!(records.len(), 2, "{records:?}");
}

#[test]
fn concurrent_identical_prompts_in_one_folder_get_their_own_identities() {
    let f = Fixture::new();
    let prompt = "the very same instruction";
    let launches: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..3)
            .map(|_| scope.spawn(|| f.launch("ok", "claude", prompt)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut ids: Vec<String> = launches
        .iter()
        .map(|out| {
            assert!(out.status.success(), "{}", stderr(out));
            stdout(out)
        })
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        3,
        "three launches of one prompt in one folder must be three identities"
    );

    let carried: Vec<String> = f
        .rows()
        .iter()
        .filter_map(|r| r["session"]["session_id"].as_str().map(str::to_owned))
        .collect();
    for id in &ids {
        assert!(carried.contains(id), "cones ls lost {id}: {carried:?}");
    }
}

#[test]
fn a_harness_that_prints_no_identity_is_reported_as_unnamed_rather_than_guessed() {
    let f = Fixture::new();
    let out = f.launch("no-id", "claude", "work with no id");
    assert!(!out.status.success(), "{}", stdout(&out));
    let error = stderr(&out);
    assert!(
        error.contains("no background id") && error.contains("started, somewhere"),
        "the error must quote what the harness printed instead: {error}"
    );
    assert!(stdout(&out).is_empty(), "no id may be printed");

    // The launch happened, so the prompt and folder stay recoverable.
    let records = f.recovery();
    assert_eq!(records[0]["event"], "launch.submitted");
    assert_eq!(records[1]["event"], "launch.unnamed");
    assert!(records[1]["data"]["session_id"].is_null());
    assert!(
        records[1]["data"]["error"]
            .as_str()
            .unwrap()
            .contains("no background id"),
        "{records:?}"
    );
    // A session that started but could not be named is still the user's to find.
    assert_eq!(
        f.rows()
            .iter()
            .filter(|r| r["session"]["harness"] == "claude")
            .count(),
        1,
        "the unnamed session must still be listed"
    );
}

#[test]
fn a_native_launch_failure_is_reported_with_its_status_and_diagnostic() {
    let f = Fixture::new();
    let out = f.launch("fail", "claude", "this will not start");
    assert!(!out.status.success());
    let error = stderr(&out);
    assert!(
        error.contains("exited 3") && error.contains("fixture refused this launch"),
        "{error}"
    );
    assert!(stdout(&out).is_empty());
    assert_eq!(f.recovery()[1]["event"], "launch.unnamed");
}

#[test]
fn a_terminal_harness_keeps_running_after_its_launcher_exits() {
    let f = Fixture::new();
    let out = f.launch("ok", "pi", "hold this terminal");
    assert!(out.status.success(), "{}", stderr(&out));
    let id = stdout(&out);
    assert!(!id.is_empty());

    // The launcher has already returned; the native client belongs to a detached host.
    let hosted = f.hosted_pids();
    assert_eq!(hosted.len(), 1, "one host record per launch: {hosted:?}");
    let pid = hosted[0];
    assert!(alive(pid), "the native client died with its launcher");

    let rows = f.rows();
    let row = rows
        .iter()
        .find(|r| r["session"]["session_id"] == id.as_str())
        .unwrap_or_else(|| panic!("cones ls does not carry {id}: {rows:?}"));
    assert_eq!(row["session"]["harness"], "pi");
    assert_eq!(row["session"]["pid"].as_u64(), Some(u64::from(pid)));

    // Still alive a moment later: nothing about reading the roster ends it.
    assert!(alive(pid));

    // The host's own stop path ends the native process, which is what cleanup relies on.
    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    wait_until("the fixture client to exit", || !alive(pid));
}

/// A hosted client that leaves during the launch is not a worker. The launch has to say so
/// with what the client showed, since the host and its scrollback go with the client.
fn assert_no_worker(f: &Fixture, out: &Output, shown: &[&str]) {
    assert!(!out.status.success(), "{}", stdout(out));
    assert!(stdout(out).is_empty(), "no id may be printed");
    let error = stderr(out);
    for text in shown {
        assert!(error.contains(text), "missing {text:?}: {error}");
    }
    assert!(
        !error.contains("running detached"),
        "nothing is running: {error}"
    );
    wait_until("the host to retire", || f.hosted_pids().is_empty());
    let rows = f.rows();
    assert!(rows.is_empty(), "nothing may be listed: {rows:?}");
    let records = f.recovery();
    assert_eq!(records[1]["event"], "launch.unnamed");
    assert!(records[1]["data"]["session_id"].is_null());
    let recorded = records[1]["data"]["error"].as_str().unwrap();
    assert!(recorded.contains(shown[0]), "{recorded}");
}

#[test]
fn a_hosted_client_that_exits_on_its_sign_in_screen_fails_the_launch_with_that_screen() {
    let f = Fixture::new();
    let out = f.launch("login", "pi", "work nobody will do");
    assert_no_worker(
        &f,
        &out,
        &[
            "pi exited 1",
            "No API key found. Starting login flow...",
            "Would you like to log in to the fixture? [(y)es, (n)o]:",
        ],
    );
}

#[test]
fn a_hosted_client_that_refuses_to_start_fails_the_launch_with_its_diagnostic() {
    let f = Fixture::new();
    let out = f.launch("fail", "pi", "this will not start");
    assert_no_worker(&f, &out, &["pi exited 3", "fixture refused this launch"]);
}

#[test]
fn a_hosted_client_waiting_on_a_screen_is_named_with_what_it_shows() {
    let f = Fixture::new();
    let out = f.launch("stuck", "pi", "work it has not taken");
    // It is alive, so it is named: cones cannot tell a sign-in question from a working client.
    assert!(out.status.success(), "{}", stderr(&out));
    let id = stdout(&out);
    let hosted = f.hosted_pids();
    assert_eq!(hosted.len(), 1, "{hosted:?}");
    assert!(alive(hosted[0]));
    assert!(
        f.rows()
            .iter()
            .any(|r| r["session"]["session_id"] == id.as_str()),
        "cones ls does not carry {id}"
    );
    // What it can say is that the harness reported nothing, and what the client shows.
    let said = stderr(&out);
    assert!(
        said.contains("pi has reported no conversation")
            && said.contains("Would you like to log in to the fixture? [(y)es, (n)o]:"),
        "{said}"
    );
}

#[test]
fn two_hosted_launches_in_one_folder_stay_two_sessions() {
    let f = Fixture::new();
    let first = stdout(&f.launch("ok", "pi", "same words"));
    let second = stdout(&f.launch("ok", "pi", "same words"));
    assert_ne!(
        first, second,
        "two hosted clients must not share an identity"
    );
    let hosted = f.hosted_pids();
    assert_eq!(hosted.len(), 2, "{hosted:?}");
    assert_ne!(hosted[0], hosted[1]);

    let carried: Vec<String> = f
        .rows()
        .iter()
        .filter_map(|r| r["session"]["session_id"].as_str().map(str::to_owned))
        .collect();
    for id in [&first, &second] {
        assert!(carried.contains(id), "cones ls lost {id}: {carried:?}");
    }
}

#[test]
fn a_codex_launch_returns_the_thread_its_daemon_made_and_leaves_no_client_behind() {
    let f = Fixture::codex();
    let out = f.launch("ok", "codex", "rebase onto main\nand keep the tests green");
    assert!(out.status.success(), "{}", stderr(&out));
    let id = stdout(&out);
    assert_eq!(
        f.codex_threads(),
        std::slice::from_ref(&id),
        "the thread the daemon made"
    );
    assert!(
        stderr(&out).contains("native session id"),
        "{}",
        stderr(&out)
    );

    // The daemon holds the thread, so the launching client is gone and the folder has one
    // row for one agent, which the dashboard joins like any other daemon thread.
    let rows = f.rows();
    let codex: Vec<&Value> = rows
        .iter()
        .filter(|r| r["session"]["harness"] == "codex")
        .collect();
    assert_eq!(codex.len(), 1, "{codex:?}");
    assert_eq!(codex[0]["session"]["session_id"], id.as_str());
    assert_eq!(codex[0]["session"]["kind"], "daemon");
    assert_eq!(codex[0]["session"]["title"], "rebase onto main");
    assert_eq!(
        f.hosted_pids(),
        [] as [u32; 0],
        "the client was left hosted"
    );

    let recovery = f.recovery();
    assert_eq!(recovery[0]["event"], "launch.submitted");
    assert_eq!(recovery[1]["event"], "launch.identified");
    assert_eq!(recovery[1]["data"]["session_id"], id.as_str());
}

#[test]
fn two_codex_threads_for_one_prompt_are_refused_rather_than_chosen_between() {
    let f = Fixture::codex();
    let out = f.launch("twice", "codex", "same words");
    assert!(!out.status.success());
    assert_eq!(stdout(&out), "", "an id was guessed");
    assert!(stderr(&out).contains("will not choose"), "{}", stderr(&out));
    assert_eq!(f.codex_threads().len(), 2);
    // Unnamed is a reporting failure: the client is still hosted and still running.
    let hosted = f.hosted_pids();
    assert_eq!(hosted.len(), 1, "{hosted:?}");
    assert!(alive(hosted[0]));
    let recovery = f.recovery();
    assert_eq!(recovery[1]["event"], "launch.unnamed");
    assert!(recovery[1]["data"]["session_id"].is_null());
}

/// Codex reports nothing that ties a thread to the launch that asked for it, so two launches
/// of one prompt into one folder could each be handed the other's thread. The ledger every
/// launch writes first is what shows the overlap, and an overlap is refused.
#[test]
fn a_codex_launch_overlapping_another_of_the_same_prompt_is_refused_and_a_later_one_is_named() {
    let f = Fixture::codex();
    let ledger = f.state().join("launches.jsonl");
    let line = |event: &str| {
        serde_json::json!({
            "v": 1, "timestamp": chrono::Utc::now().to_rfc3339(), "pid": 1,
            "dashboard_id": null, "event": event, "level": "recovery",
            "data": {
                "operation_id": "codex:start:rival", "harness": "codex",
                "cwd": f.work, "prompt": "same words\nwith its own second line",
            },
        })
        .to_string()
            + "\n"
    };
    fs::write(&ledger, line("launch.submitted")).unwrap();

    let out = f.launch("ok", "codex", "same words");
    assert!(!out.status.success());
    assert_eq!(stdout(&out), "", "an id was guessed");
    assert!(stderr(&out).contains("will not choose"), "{}", stderr(&out));
    let refused = f.codex_threads();
    assert_eq!(refused.len(), 1, "the refused launch still made its thread");
    let hosted = f.hosted_pids();
    assert_eq!(hosted.len(), 1, "{hosted:?}");
    assert!(alive(hosted[0]), "an unnamed launch must be left running");
    assert_eq!(f.recovery().last().unwrap()["event"], "launch.unnamed");

    // Another prompt was never in question, whatever else is waiting.
    let out = f.launch("ok", "codex", "other words");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(f.codex_threads()[1], stdout(&out));

    // Once the rival is settled, the same prompt is named again, and by its own thread: the
    // refused launch's thread is older than this launch and is not a candidate.
    let mut settled = fs::read_to_string(&ledger).unwrap();
    settled.push_str(&line("launch.unnamed"));
    fs::write(&ledger, settled).unwrap();
    let out = f.launch("ok", "codex", "same words");
    assert!(out.status.success(), "{}", stderr(&out));
    let threads = f.codex_threads();
    assert_eq!(threads.len(), 3);
    assert_eq!(threads[2], stdout(&out));
    assert_ne!(threads[2], refused[0]);
    assert_eq!(
        f.recovery().last().unwrap()["data"]["session_id"],
        threads[2].as_str()
    );
}

/// Codex keeps nothing for a client that has not taken a turn, so the only id such a launch
/// could return is one that stops resolving at the first turn.
#[test]
fn a_codex_launch_with_no_prompt_is_refused_before_anything_is_started() {
    let f = Fixture::codex();
    let out = f.cones(
        "ok",
        &[
            "launch",
            "--dir",
            f.work.to_str().unwrap(),
            "--harness",
            "codex",
        ],
    );
    assert!(!out.status.success());
    assert_eq!(stdout(&out), "", "an id was returned");
    assert!(stderr(&out).contains("needs a prompt"), "{}", stderr(&out));
    assert_eq!(f.hosted_pids(), [] as [u32; 0], "a client was started");
    assert_eq!(f.codex_threads(), [] as [String; 0]);
    assert!(f.recovery().is_empty(), "{:?}", f.recovery());
}

#[test]
fn a_launch_with_no_prompt_detaches_and_is_named_like_any_other() {
    // A worker started with nothing to do is waiting for input, not a failed launch: it is
    // named, listed and left alone.
    for harness in ["claude", "pi"] {
        let f = Fixture::new();
        let out = f.cones(
            "ok",
            &[
                "launch",
                "--dir",
                f.work.to_str().unwrap(),
                "--harness",
                harness,
            ],
        );
        assert!(out.status.success(), "{harness}: {}", stderr(&out));
        let id = stdout(&out);
        assert!(!id.is_empty(), "{harness} returned no identifier");
        assert_eq!(f.recovery()[0]["data"]["prompt"], "");
        let carried: Vec<String> = f
            .rows()
            .iter()
            .filter_map(|r| r["session"]["session_id"].as_str().map(str::to_owned))
            .collect();
        assert!(carried.contains(&id), "{harness}: lost {id}: {carried:?}");
        for pid in f.hosted_pids() {
            assert!(
                alive(pid),
                "{harness}: the waiting client was not left running"
            );
        }
    }
}

#[test]
fn a_copilot_launch_is_one_row_for_its_native_client_wherever_npm_put_it() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    // Beside the loader's package, and nested inside it as Copilot 1.0.95 installs.
    for platform in [
        format!("../copilot-darwin-{arch}"),
        format!("node_modules/@github/copilot-darwin-{arch}"),
    ] {
        let f = Fixture::copilot(&platform);
        let out = f.launch("ok", "copilot", "tidy the imports");
        assert!(out.status.success(), "{platform}: {}", stderr(&out));
        let id = stdout(&out);
        let hosted = f.hosted_pids();
        assert_eq!(hosted.len(), 1, "{platform}: {hosted:?}");
        let pid = hosted[0];
        assert_eq!(id, format!("copilot-{pid}"), "{platform}");

        // The loader waits on a second process of the same name. Hosting it would list
        // one agent twice, and only the loader's row would stop.
        let rows = f.rows();
        let copilot: Vec<&Value> = rows
            .iter()
            .filter(|r| r["session"]["harness"] == "copilot")
            .collect();
        assert_eq!(copilot.len(), 1, "{platform}: {copilot:?}");
        assert_eq!(copilot[0]["session"]["session_id"], id.as_str());
        assert_eq!(copilot[0]["session"]["pid"].as_u64(), Some(u64::from(pid)));
        assert_eq!(copilot[0]["session"]["title"], "tidy the imports");

        // The returned id is the one `cones stop` takes, and stopping leaves nothing behind.
        let stopped = f.cones("ok", &["stop", &id]);
        assert!(stopped.status.success(), "{platform}: {}", stderr(&stopped));
        wait_until("the native client to exit", || !alive(pid));
        assert_eq!(f.hosted_pids(), [] as [u32; 0], "{platform}");
        assert_eq!(f.rows(), [] as [Value; 0], "{platform}");
    }
}

#[test]
fn a_harness_with_no_interactive_instruction_refuses_one_and_starts_without() {
    // Kimi 2 prints a `--prompt` answer and exits, and refuses the flag beside `--auto`, so
    // an instruction would start a client that is gone before anyone opens it.
    let f = Fixture::offering("kimi");
    for extra in [&[][..], &["--print-command"][..]] {
        let mut args = vec!["launch", "--dir", f.work.to_str().unwrap()];
        args.extend(["--harness", "kimi"]);
        args.extend(extra);
        args.push("fix the parser");
        let out = f.cones("ok", &args);
        assert!(!out.status.success(), "{extra:?}: {}", stdout(&out));
        assert_eq!(
            stdout(&out),
            "",
            "{extra:?}: an id or a command was printed"
        );
        assert!(
            stderr(&out).contains("kimi cannot open an interactive session with an instruction"),
            "{extra:?}: {}",
            stderr(&out)
        );
    }
    assert!(f.recovery().is_empty(), "a refused launch was recorded");
    assert!(
        f.hosted_pids().is_empty(),
        "a refused launch started a client"
    );

    let out = f.cones(
        "ok",
        &[
            "launch",
            "--dir",
            f.work.to_str().unwrap(),
            "--harness",
            "kimi",
        ],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let hosted = f.hosted_pids();
    assert_eq!(hosted.len(), 1, "{hosted:?}");
    assert_eq!(stdout(&out), format!("kimi-{}", hosted[0]));
    let rows = f.rows();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["session"]["session_id"], stdout(&out).as_str());
    assert_eq!(rows[0]["session"]["harness"], "kimi");
    assert!(alive(hosted[0]), "the waiting client was not left running");
}

#[test]
fn print_command_starts_nothing_and_keeps_naming_the_folder() {
    let f = Fixture::new();
    for harness in ["claude", "pi"] {
        let out = f.cones(
            "ok",
            &[
                "launch",
                "--dir",
                f.work.to_str().unwrap(),
                "--harness",
                harness,
                "--print-command",
                "a prompt",
            ],
        );
        assert!(out.status.success(), "{}", stderr(&out));
        let printed = stdout(&out);
        assert!(
            printed.starts_with(&format!("cd '{}' &&", f.work.display())),
            "{printed}"
        );
        assert!(printed.contains("a prompt"), "{printed}");
        assert!(
            printed.contains(&format!("/.local/bin/{harness}")),
            "{printed}"
        );
    }
    assert!(
        f.hosted_pids().is_empty(),
        "--print-command must start nothing"
    );
    assert!(
        f.recovery().is_empty(),
        "--print-command must record no launch"
    );
    assert!(
        f.rows().is_empty(),
        "--print-command must leave the folder empty"
    );
}

/// Codex's command names its daemon's address, and asking for it used to start the daemon.
/// With none running the print refuses and names the command that starts one; with one running
/// it prints the address that daemon reports.
#[test]
fn print_command_for_codex_starts_no_daemon_and_prints_a_running_one() {
    let f = Fixture::codex();
    let pid_file = f.codex_home().join("app-server-daemon/app-server.pid");
    let print = || {
        f.cones(
            "ok",
            &[
                "launch",
                "--dir",
                f.work.to_str().unwrap(),
                "--harness",
                "codex",
                "--print-command",
                "a prompt",
            ],
        )
    };
    let refused = print();
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(
        stderr(&refused).contains("codex app-server daemon start"),
        "{}",
        stderr(&refused)
    );
    assert!(!pid_file.exists(), "--print-command started the daemon");

    // A real launch starts the daemon, after which the same print has an address to name.
    let launched = f.launch("ok", "codex", "rebase onto main");
    assert!(launched.status.success(), "{}", stderr(&launched));
    let daemon = fs::read_to_string(&pid_file).unwrap();
    let printed = print();
    assert!(printed.status.success(), "{}", stderr(&printed));
    assert!(
        stdout(&printed).contains("'--remote' 'unix://"),
        "{}",
        stdout(&printed)
    );
    assert_eq!(fs::read_to_string(&pid_file).unwrap(), daemon);
    assert_eq!(f.codex_threads().len(), 1, "the print made a thread");
}

/// A prompt carrying a one-time secret must not be kept by cones. The launch is still recorded,
/// so it can be found and a retry is visible, with the prompt absent from both records and
/// from every other file under the state directory.
#[test]
fn no_prompt_record_keeps_the_prompt_out_of_everything_cones_writes() {
    let f = Fixture::new();
    let secret = "deploy with token-9f3c1e7a";
    let out = f.cones(
        "ok",
        &[
            "launch",
            "--dir",
            f.work.to_str().unwrap(),
            "--harness",
            "claude",
            "--no-prompt-record",
            secret,
        ],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let id = stdout(&out);
    let records = f.recovery();
    assert_eq!(records.len(), 2, "{records:?}");
    assert_eq!(records[0]["event"], "launch.submitted");
    assert_eq!(records[1]["event"], "launch.identified");
    assert_eq!(records[1]["data"]["session_id"], id.as_str());
    for record in &records {
        assert!(record["data"]["prompt"].is_null(), "{record}");
        assert_eq!(record["data"]["harness"], "claude");
    }
    let mut stack = vec![f.state()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = fs::read(&path) {
                assert!(
                    !String::from_utf8_lossy(&bytes).contains("token-9f3c1e7a"),
                    "{} kept the prompt",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn a_disabled_or_unknown_harness_is_refused_before_anything_starts() {
    let f = Fixture::new();
    for (harness, expected) in [
        ("codex", "turned off in the configuration"),
        ("nonesuch", "unknown harness nonesuch"),
    ] {
        let out = f.launch("ok", harness, "no");
        assert!(!out.status.success(), "{harness} was not refused");
        assert!(stderr(&out).contains(expected), "{}", stderr(&out));
    }
    assert!(f.recovery().is_empty());
    assert!(f.hosted_pids().is_empty());
}

#[test]
fn a_launch_directory_that_is_not_a_folder_is_refused() {
    let f = Fixture::new();
    let file = f.path().join("notadir");
    fs::write(&file, "x").unwrap();
    for dir in [file.to_str().unwrap(), "/no/such/folder/here"] {
        let out = f.cones(
            "ok",
            &["launch", "--dir", dir, "--harness", "claude", "hello"],
        );
        assert!(!out.status.success(), "{dir} was accepted");
        assert!(
            stderr(&out).contains("launch directory") || stderr(&out).contains("is not a folder"),
            "{}",
            stderr(&out)
        );
    }
    assert!(f.recovery().is_empty());
}

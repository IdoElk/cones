//! `cones btw ID QUESTION`, against fake claude, codex, pi and opencode CLIs under a disposable
//! HOME. No model is contacted.
//!
//! Each conversation is a native history fixture. The artifact is the answer `cones btw`
//! prints, the argv the fake CLI logged in `calls.jsonl`, and the conversation's own files,
//! which must be byte for byte what they were.
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const CLAUDE: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const CODEX: &str = "019a12fc-17d9-7af2-bc5a-92de2ec06071";
const PI: &str = "bbbbbbbb-2222-4222-8222-222222222222";
const OPENCODE: &str = "ses_fixture";

struct Fixture {
    root: tempfile::TempDir,
    work: PathBuf,
    jobs: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("cones-btw-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = root.path();
        for dir in [
            "state",
            "claude",
            "codex",
            "pi",
            "data/opencode",
            "work",
            ".local/bin",
        ] {
            fs::create_dir_all(path.join(dir)).unwrap();
        }
        let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_btw.py");
        for name in ["claude", "codex", "pi", "opencode"] {
            std::os::unix::fs::symlink(&fake, path.join(".local/bin").join(name)).unwrap();
        }
        // The default settings skip permission prompts, which a side question must not take.
        let jobs = path.join("jobs.yaml");
        fs::write(&jobs, "version: 4\njobs: []\n").unwrap();
        let work = path.join("work").canonicalize().unwrap();
        Self { root, work, jobs }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn cones(&self, mode: &str, args: &[&str]) -> Output {
        self.command(mode, args).output().unwrap()
    }

    fn command(&self, mode: &str, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cones"));
        command
            .args(["--jobs", self.jobs.to_str().unwrap(), "--state-dir"])
            .arg(self.path().join("state"))
            .args(args)
            .env("HOME", self.path())
            .env("FAKE_BTW", mode)
            .env("CLAUDE_CONFIG_DIR", self.path().join("claude"))
            .env("CODEX_HOME", self.path().join("codex"))
            .env("PI_CODING_AGENT_DIR", self.path().join("pi"))
            .env("XDG_DATA_HOME", self.path().join("data"))
            .env_remove("OPENCODE_DB")
            .env_remove("CLAUDE_CODE_USE_BEDROCK");
        command
    }

    /// One conversation per harness, in the fixture's work folder. Returns each one's files.
    fn conversations(&self) -> Vec<PathBuf> {
        let work = self.work.to_str().unwrap();
        let escaped = work.replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        let claude = self
            .path()
            .join("claude/projects")
            .join(escaped)
            .join(format!("{CLAUDE}.jsonl"));
        let codex = self
            .path()
            .join("codex/sessions/2026/10/17")
            .join(format!("rollout-2026-10-17T10-00-00-{CODEX}.jsonl"));
        let pi = self
            .path()
            .join("pi/sessions/--work--")
            .join(format!("2026-10-17T10-00-00-000Z_{PI}.jsonl"));
        let records = [
            (
                &claude,
                vec![
                    json!({"type":"user","sessionId":CLAUDE,"cwd":work,"timestamp":"2026-10-17T10:00:00Z","message":{"content":"Remember kiwi"}}),
                    json!({"type":"assistant","timestamp":"2026-10-17T10:00:01Z","message":{"id":"m1","model":"fixture","content":[{"type":"text","text":"ok"}]}}),
                ],
            ),
            (
                &codex,
                vec![
                    json!({"type":"session_meta","timestamp":"2026-10-17T10:00:00Z","payload":{"id":CODEX,"session_id":CODEX,"cwd":work,"timestamp":"2026-10-17T10:00:00Z","source":"cli"}}),
                    json!({"type":"event_msg","timestamp":"2026-10-17T10:00:01Z","payload":{"item":{"type":"UserMessage","content":[{"type":"text","text":"Remember kiwi"}]}}}),
                ],
            ),
            (
                &pi,
                vec![
                    json!({"type":"session","id":PI,"cwd":work,"timestamp":"2026-10-17T10:00:00Z"}),
                    json!({"type":"message","timestamp":"2026-10-17T10:00:01Z","message":{"role":"user","content":[{"type":"text","text":"Remember kiwi"}]}}),
                ],
            ),
        ];
        for (path, lines) in records {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
            fs::write(path, text).unwrap();
        }
        let db = self.path().join("data/opencode/opencode.db");
        let mut sqlite = Command::new("sqlite3")
            .arg(&db)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let sql = include_str!("../assets/harnesses/fixtures/opencode.sql");
        let sql =
            format!("{sql}\nUPDATE session SET directory = '{work}' WHERE id = '{OPENCODE}';\n");
        sqlite
            .stdin
            .take()
            .unwrap()
            .write_all(sql.as_bytes())
            .unwrap();
        assert!(sqlite.wait().unwrap().success());
        vec![claude, codex, pi, db]
    }

    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.path().join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The asking calls, without the help probes.
    fn asked(&self) -> Vec<Value> {
        self.calls()
            .into_iter()
            .filter(|call| !call["args"].to_string().contains("--help"))
            .collect()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn args(call: &Value) -> Vec<String> {
    call["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_owned())
        .collect()
}

/// Every harness answers from a fork of the conversation under its own read-only setting, with
/// no skip-permissions flag although the settings skip prompts. The conversation's files are
/// untouched, pi's saved fork and Codex's answer file are gone with their directory, and
/// OpenCode's fork is deleted by the id its events name while the source never is.
#[test]
fn every_harness_answers_from_a_read_only_fork_it_keeps_nothing_of() {
    let f = Fixture::new();
    let files = f.conversations();
    let before: Vec<Vec<u8>> = files.iter().map(|p| fs::read(p).unwrap()).collect();
    let question = "--what was the word?";
    let cases = [
        (
            CLAUDE,
            "claude",
            format!("claude answers '{question}' from {CLAUDE}"),
        ),
        (
            CODEX,
            "codex",
            format!("codex answers '{question}' from {CODEX}"),
        ),
        (
            PI,
            "pi",
            format!(
                "pi answers '{question}' from {}",
                files[2].file_name().unwrap().to_string_lossy()
            ),
        ),
        (
            OPENCODE,
            "opencode",
            format!("opencode answers '{question}' from {OPENCODE}"),
        ),
    ];
    for (id, harness, answer) in &cases {
        let out = f.cones("ok", &["btw", "--", id, question]);
        assert!(out.status.success(), "{harness}: {}", text(&out.stderr));
        assert_eq!(text(&out.stdout), format!("{answer}\n"), "{harness}");
    }
    let asked = f.asked();
    let by = |name: &str| -> Vec<&Value> { asked.iter().filter(|c| c["name"] == name).collect() };
    for name in ["claude", "codex", "pi"] {
        assert_eq!(by(name).len(), 1, "{name}: {asked:?}");
        assert_eq!(by(name)[0]["cwd"], f.work.to_str().unwrap(), "{name}");
    }
    assert_eq!(
        args(by("claude")[0]),
        [
            "-p",
            "--resume",
            CLAUDE,
            "--fork-session",
            "--no-session-persistence",
            "--permission-mode",
            "plan",
            "--output-format",
            "json",
            "--",
            question
        ]
    );
    let codex = args(by("codex")[0]);
    let answer = PathBuf::from(&codex[7]);
    assert_eq!(
        codex,
        [
            "exec",
            "--sandbox",
            "read-only",
            "fork",
            "--ephemeral",
            "--skip-git-repo-check",
            "--output-last-message",
            answer.to_str().unwrap(),
            "--",
            CODEX,
            question
        ]
    );
    let pi = args(by("pi")[0]);
    let forks = PathBuf::from(&pi[4]);
    assert_eq!(
        pi,
        [
            "--print",
            "--fork",
            files[2].canonicalize().unwrap().to_str().unwrap(),
            "--session-dir",
            forks.to_str().unwrap(),
            "--tools",
            "read,grep,find,ls",
            "--",
            question
        ]
    );
    for scratch in [answer.parent().unwrap(), forks.as_path()] {
        assert!(!scratch.exists(), "{} was left behind", scratch.display());
    }
    let opencode = by("opencode");
    assert_eq!(opencode.len(), 2, "{opencode:?}");
    assert_eq!(
        args(opencode[0]),
        [
            "run",
            "--session",
            OPENCODE,
            "--fork",
            "--agent",
            "plan",
            "--format",
            "json",
            "--",
            question
        ]
    );
    assert_eq!(args(opencode[1]), ["session", "delete", "ses_fork1"]);
    for (file, was) in files.iter().zip(&before) {
        assert_eq!(&fs::read(file).unwrap(), was, "{} changed", file.display());
    }
}

/// `--model` reaches each harness through its own model flag, ahead of the question.
#[test]
fn a_model_override_is_the_harness_s_own_flag() {
    let f = Fixture::new();
    f.conversations();
    for (id, flag) in [(CLAUDE, "--model"), (CODEX, "-m"), (PI, "--model")] {
        let out = f.cones("ok", &["btw", "--model", "fixture-model", id, "why?"]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        let call = f.asked().pop().unwrap();
        let args = args(&call);
        let at = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[at - 2..at], [flag, "fixture-model"], "{args:?}");
    }
}

/// A harness that reports an error answers nothing: the command fails with the harness's own
/// words and prints no answer.
#[test]
fn a_harness_error_is_reported_and_no_answer_is_printed() {
    let f = Fixture::new();
    f.conversations();
    let out = f.cones("error", &["btw", CLAUDE, "why?"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert!(
        text(&out.stderr).contains("claude could not answer: Prompt is too long"),
        "{}",
        text(&out.stderr)
    );
}

/// Past `--timeout` the fork is ended and the command exits 2 with `timeout`. OpenCode's fork
/// is deleted even then, by the id its events named before the answer.
#[test]
fn a_timeout_ends_the_fork_and_still_deletes_opencode_s() {
    let f = Fixture::new();
    f.conversations();
    for (id, name) in [(CLAUDE, "claude"), (OPENCODE, "opencode")] {
        let out = f.cones("slow", &["btw", "--timeout", "1", id, "why?"]);
        assert_eq!(out.status.code(), Some(2), "{name}: {}", text(&out.stderr));
        assert_eq!(text(&out.stdout), "timeout\n");
        let pid: i32 = fs::read_to_string(f.path().join(format!("{name}.pid")))
            .unwrap()
            .parse()
            .unwrap();
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "{name} {pid} still runs");
    }
    let deleted: Vec<Vec<String>> = f
        .asked()
        .iter()
        .filter(|c| c["args"][0] == "session")
        .map(args)
        .collect();
    assert_eq!(deleted, [["session", "delete", "ses_fork1"]]);
}

/// A harness with no verified read-only fork, and an id no conversation has, reach nothing.
#[test]
fn an_unknown_conversation_reaches_nothing() {
    let f = Fixture::new();
    f.conversations();
    let out = f.cones("ok", &["btw", "nonexistent-session", "why?"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("no session nonexistent-session"),
        "{}",
        text(&out.stderr)
    );
    let out = f.cones("ok", &["btw", CLAUDE, "  "]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("a question is needed"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(f.asked(), Vec::<Value>::new());
}

/// `cones_btw` over MCP runs `cones btw` and returns its answer.
#[test]
fn mcp_btw_returns_the_answer() {
    let f = Fixture::new();
    f.conversations();
    let mut child = f
        .command("ok", &["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    for message in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "fixture", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "cones_btw", "arguments": {"id": CLAUDE, "question": "why?"}}}),
    ] {
        writeln!(input, "{message}").unwrap();
    }
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    let result = serde_json::from_str::<Value>(&line).unwrap()["result"].clone();
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(
        result["content"][0]["text"],
        format!("claude answers 'why?' from {CLAUDE}")
    );
    let call = f.asked().pop().unwrap();
    assert_eq!(args(&call).last().unwrap(), "why?");
    drop(input);
    child.wait().unwrap();
}

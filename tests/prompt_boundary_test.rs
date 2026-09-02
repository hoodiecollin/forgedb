use std::path::{Path, PathBuf};
use std::process::Command;

use forgedb::ask::Askability;

const BIN: &str = env!("CARGO_BIN_EXE_forgedb");

const SCHEMA: &str = "Note {\n  id: +uuid\n  body: string\n}\n";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn repo_root(dir: &tempfile::TempDir) -> PathBuf {
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    root
}

fn ambiguous_root(dir: &tempfile::TempDir) -> PathBuf {
    let root = repo_root(dir);
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"backend\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("package.json"),
        "{ \"name\": \"storefront\", \"version\": \"1.0.0\" }",
    );
    write(&root.join("schema.forge"), SCHEMA);
    root
}

fn run_traced(cwd: &Path, home: &Path, trace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("FORGEDB_HOME", home)
        .env("FORGEDB_ASK_TRACE", trace)
        .output()
        .expect("forgedb binary runs")
}

fn trace_lines(trace: &Path) -> Vec<String> {
    std::fs::read_to_string(trace)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn s1_the_boundary_is_a_pure_predicate_over_four_booleans() {
    let mut permitted = Vec::new();
    for bits in 0u8..16 {
        let a = Askability {
            stdin_tty: bits & 1 != 0,
            stderr_tty: bits & 2 != 0,
            quiet: bits & 4 != 0,
            forbidden: bits & 8 != 0,
        };
        if a.may_ask() {
            permitted.push(a);
        }

        let expected = if a.forbidden {
            "forbidden"
        } else if a.quiet {
            "quiet"
        } else if !a.stdin_tty {
            "no-stdin-tty"
        } else if !a.stderr_tty {
            "no-stderr-tty"
        } else {
            "terminal"
        };
        assert_eq!(a.reason(), expected, "{a:?}");
        assert_eq!(
            a.may_ask(),
            a.reason() == "terminal",
            "`terminal` and `may_ask` must never disagree: {a:?}"
        );
    }

    assert_eq!(
        permitted,
        vec![Askability {
            stdin_tty: true,
            stderr_tty: true,
            quiet: false,
            forbidden: false,
        }],
        "exactly one of sixteen rows may ask"
    );
}

#[test]
fn s1b_each_clause_vetoes_on_its_own() {
    let open = Askability {
        stdin_tty: true,
        stderr_tty: true,
        quiet: false,
        forbidden: false,
    };
    assert!(open.may_ask());

    assert!(!Askability { stdin_tty: false, ..open }.may_ask());
    assert!(!Askability { stderr_tty: false, ..open }.may_ask());
    assert!(!Askability { quiet: true, ..open }.may_ask());
    assert!(!Askability { forbidden: true, ..open }.may_ask());
}

#[test]
fn s2_a_piped_invocation_never_asks_and_names_a_remedy() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let base = tmp.path().canonicalize().unwrap();

    for side in ["one", "two"] {
        let root = base.join(side);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        write(&root.join("forgedb.toml"), "[project]\nid = \"copied\"\n");
        write(&root.join("schema.forge"), SCHEMA);
    }
    let first = run_traced(
        &base.join("one"),
        home.path(),
        &trace,
        &["generate", "rust", "--output", base.join("one/generated").to_str().unwrap()],
    );
    assert!(first.status.success(), "{}", combined(&first));

    let out = run_traced(
        &base.join("two"),
        home.path(),
        &trace,
        &["generate", "rust", "--output", base.join("two/generated").to_str().unwrap()],
    );

    assert!(!out.status.success(), "a taken id is still refused");
    let msg = combined(&out);
    assert!(msg.contains("already held"), "{msg}");
    assert!(msg.contains("copied"), "names the contested id: {msg}");
    assert!(msg.contains("[project].id"), "names the key to change: {msg}");
    assert!(
        msg.contains(&base.join("one").display().to_string()),
        "names the root that holds it: {msg}"
    );
    assert!(
        !msg.contains("forgedb project name") && !msg.contains("forgedb project claim"),
        "the deleted remedy commands must not reappear in a diagnostic: {msg}"
    );

    assert!(
        trace_lines(&trace).is_empty(),
        "generate reaches no asking boundary at all: {:?}",
        trace_lines(&trace)
    );
}

#[test]
fn s3_machine_readable_stdout_forbids_rather_than_merely_silencing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"clean\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    let out = run_traced(
        &root,
        home.path(),
        &trace,
        &["build", "--plan", "--print-artifact", "server"],
    );
    let msg = combined(&out);

    let lines = trace_lines(&trace);
    assert!(
        lines.contains(&"forbid".to_string()),
        "the forbid() CALL SITE must run, not merely exist: {lines:?}\n{msg}"
    );
    assert!(
        !lines.contains(&"terminal".to_string()),
        "a machine-readable stdout may never ask: {lines:?}"
    );
}

#[test]
fn s3b_a_plain_build_does_not_forbid() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"clean\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    run_traced(&root, home.path(), &trace, &["build", "--plan"]);

    let lines = trace_lines(&trace);
    assert!(
        !lines.contains(&"forbid".to_string()),
        "forbidding is per-mode, not global: {lines:?}"
    );
    assert!(
        lines.is_empty(),
        "a plain build reaches no asking boundary at all: {lines:?}"
    );
}

#[test]
fn s4_dev_forbids_before_the_watch_loop() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands/dev.rs"))
        .expect("dev.rs is readable");

    let forbid = src
        .find("ask::forbid()")
        .expect("dev.rs calls ask::forbid()");
    let watch = src
        .find("auto_watch(")
        .expect("dev.rs calls auto_watch()");
    assert!(
        forbid < watch,
        "a prompt raised by a save-triggered regeneration is a hang with no \
         visible cause — the terminal is showing watch output, not a question"
    );
}

#[test]
#[ignore = "tier 2: spawns `forgedb dev` and waits on the watcher"]
fn s4b_dev_stays_forbidden_across_a_regeneration() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"watched\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    let mut child = Command::new(BIN)
        .args([
            "dev",
            "--debounce",
            "50",
            "--output",
            root.join("generated").to_str().unwrap(),
        ])
        .current_dir(&root)
        .env("FORGEDB_HOME", home.path())
        .env("FORGEDB_ASK_TRACE", &trace)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("dev starts");

    std::thread::sleep(std::time::Duration::from_millis(1500));
    write(
        &root.join("schema.forge"),
        "Note {\n  id: +uuid\n  body: string\n  title: string\n}\n",
    );
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let _ = child.kill();
    let _ = child.wait();

    let lines = trace_lines(&trace);
    assert!(
        lines.contains(&"forbid".to_string()),
        "the loop is entered with asking latched off: {lines:?}"
    );
    assert!(
        !lines.contains(&"terminal".to_string()),
        "nothing in a watch loop may ask: {lines:?}"
    );
    assert!(
        root.join("generated").exists(),
        "the watcher actually regenerated — otherwise this asserts nothing"
    );
}

#[test]
fn s5_the_language_server_cannot_reach_an_asker() {
    let manifest = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/lsp-server/Cargo.toml"
    ))
    .expect("the LSP crate's manifest is readable");
    for line in manifest.lines() {
        let line = line.trim();
        assert!(
            !line.starts_with("forgedb ") && !line.starts_with("forgedb="),
            "the language server must not depend on the root crate, whose \
             identity resolution can ask questions: {line}"
        );
    }

    let launcher = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/lsp.rs"
    ))
    .expect("lsp.rs is readable");
    for needle in ["govern", "identify", "ask::"] {
        assert!(
            !launcher.contains(needle),
            "`forgedb lsp` must hand the process over before resolving a \
             project — it found `{needle}`"
        );
    }
}

#[test]
fn s19c_the_prompt_widget_is_reachable_only_through_the_boundary() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for file in rust_files(&src_dir) {
        if file.file_name().and_then(|n| n.to_str()) == Some("ask.rs") {
            continue;
        }
        if std::fs::read_to_string(&file)
            .unwrap_or_default()
            .contains("TerminalPrompt")
        {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "the prompt widget must be reachable only through `ask::prompt()`, which \
         is what checks whether asking is allowed at all:\n  {}",
        offenders.join("\n  ")
    );

    let ask_rs = std::fs::read_to_string(src_dir.join("ask.rs")).expect("src/ask.rs");
    assert_eq!(
        ask_rs.matches("Box::new(TerminalPrompt)").count(),
        1,
        "`TerminalPrompt` has exactly one constructor"
    );
    let start = ask_rs
        .find("pub fn prompt() -> Option<Box<dyn Prompt>> {")
        .expect("`prompt()` is that constructor");
    let body = &ask_rs[start..start + 400];
    assert!(
        body.contains("Askability::detect()") && body.contains("may_ask()"),
        "the construction is gated by the boundary in the same function: {body}"
    );
}

#[test]
fn s19b_terminal_detection_has_one_definition() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for file in rust_files(&src_dir) {
        if file.file_name().and_then(|n| n.to_str()) == Some("ask.rs") {
            continue;
        }
        if std::fs::read_to_string(&file)
            .unwrap_or_default()
            .contains("is_terminal()")
        {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "terminal detection belongs to `Askability::detect()` alone:\n  {}",
        offenders.join("\n  ")
    );
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out
}

#[test]
#[ignore = "tier 2: allocates a pty via script(1)"]
fn s20_the_widget_renders_on_stderr_and_the_answer_lands() {
    let build_fixture = |dir: &Path| {
        write(&dir.join("forgedb.toml"), "[project]\nid = \"ptyprompt\"\n");
        write(&dir.join("schema.forge"), "Post {\n  id: +uuid\n  title: string\n}\n");
        let baseline = std::process::Command::new(BIN)
            .current_dir(dir)
            .env("FORGEDB_HOME", dir.join(".home"))
            .args(["migrate", "create", "baseline", "--schema", "schema.forge"])
            .output()
            .expect("baseline migrate create");
        assert!(baseline.status.success(), "{}", combined(&baseline));
        write(
            &dir.join("schema.forge"),
            "Post {\n  id: +uuid\n  title: string\n  slug: string\n}\n",
        );
    };

    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = repo_root(&tmp);
    build_fixture(&root);
    let trace = tmp.path().join("ask.trace");
    let stdout_log = tmp.path().join("stdout.log");

    let session = pty_run(
        &root,
        home.path(),
        &trace,
        "\nplaceholder\n",
        &format!(
            "{BIN} migrate create 'add slug' --schema schema.forge > {}; echo FORGEDB_EXIT=$?",
            stdout_log.display()
        ),
    );

    assert_eq!(
        trace_lines(&trace),
        vec!["terminal".to_string()],
        "this test is worthless without a real pty, so that is asserted before \
         anything else — a harness that failed to allocate one would take the \
         piped path and pass while proving nothing.\n{session}"
    );

    let captured = std::fs::read_to_string(&stdout_log).unwrap_or_default();
    assert!(
        !captured.contains("What should existing rows get?"),
        "the question leaked into a captured stdout:\n{captured}"
    );
    assert!(
        session.contains("What should existing rows get?"),
        "…and it must have been on stderr, which the pty saw:\n{session}"
    );
    assert!(
        session.contains("FORGEDB_EXIT=0"),
        "an answered prompt completes the run:\n{session}"
    );

    let record = std::fs::read_dir(root.join("migrations"))
        .expect("migrations/")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|x| x.to_str()) == Some("json")
            && std::fs::read_to_string(p).unwrap_or_default().contains("slug"))
        .expect("a record naming the answered field");
    let body = std::fs::read_to_string(&record).unwrap();
    assert!(
        body.contains("placeholder"),
        "the typed constant must be recorded as the answer:\n{body}"
    );
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for n in chars.by_ref() {
                if ('@'..='~').contains(&n) {
                    break;
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

fn pty_run(cwd: &Path, home: &Path, trace: &Path, input: &str, command: &str) -> String {
    for args in [
        vec!["-q", "/dev/null", "sh", "-c", command],
        vec!["-qec", command, "/dev/null"],
    ] {
        let mut child = Command::new("script")
            .args(&args)
            .current_dir(cwd)
            .env("FORGEDB_HOME", home)
            .env("FORGEDB_ASK_TRACE", trace)
            .env("TERM", "xterm")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("script(1) is available");
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("stdin");
            let _ = stdin.write_all(unescape(input).as_bytes());
            let _ = stdin.flush();
        }
        let out = child.wait_with_output().expect("script(1) runs");
        let text = combined(&out);
        if std::fs::metadata(trace).is_ok() {
            return text;
        }
    }
    panic!("neither `script(1)` invocation form produced a session");
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\x1b", "\x1b")
}

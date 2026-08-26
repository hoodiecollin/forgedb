//! May ForgeDB ask? — the #367 boundary scenarios (gate #371, S1–S5).
//!
//! **The non-interactive path is the feature here, not an edge case.** Every
//! decision point has defined behaviour with no terminal, because the contexts
//! that have none are the ones ForgeDB itself ships: the `Dockerfile` `init`
//! scaffolds, `docker build`, the reclose workflows, `dev`'s watch loop,
//! `build --print-artifact` inside a `$(…)` capture, and the language server.
//! A prompt that can hang in any of those is a defect, not a rough edge.
//!
//! The subprocess scenarios below inherit that property by construction —
//! `std::process::Command` pipes stdio, so they *always* take the
//! non-interactive branch. That is exactly why `FORGEDB_ASK_TRACE` is shipped
//! code rather than a test fixture: without it, "did not ask because forbidden"
//! and "did not ask because piped" are indistinguishable from outside, and a
//! test asserting the first passes when only the second is true.

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

/// A root two ecosystem manifests name, with a schema and **no** `forgedb.toml`
/// — the shape that makes `identify` ambiguous. An adopted repository carrying
/// both a `Cargo.toml` and a `package.json` is ordinary, which is why this path
/// is routinely reached.
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

/// Run the CLI with an isolated cache home and a trace file, from an explicit
/// directory.
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

// ---------------------------------------------------------------------------
// S1 — the boundary is a pure predicate
// ---------------------------------------------------------------------------

/// All sixteen combinations, exhaustively.
///
/// This is the **only** place the interactive condition is exercised over its
/// whole domain, and it can be because the predicate takes four booleans rather
/// than calling `IsTerminal` itself. A boundary fused to the terminal would
/// have exactly one testable row here: the one the harness happens to be in.
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

        // `reason()` names the FIRST failing clause in a fixed order, so a
        // trace line is a stable, greppable fact rather than "whichever check
        // the compiler got to first".
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

/// Each clause is an independent veto.
///
/// Asserted separately from the truth table because the four exist for four
/// different reasons and a later simplification must not collapse them:
/// `stdin` is what stops a `docker build` blocking forever (a prompt *reads*
/// stdin), `stderr` is about the question being visible at all, `quiet` is the
/// user asking for silence, and `forbidden` is ForgeDB knowing its own stdout.
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

// ---------------------------------------------------------------------------
// S2 — a piped invocation never asks, and says so
// ---------------------------------------------------------------------------

/// The contract for every scripted invocation: an identical diagnostic and an
/// identical non-zero exit whether or not a terminal is attached.
///
/// **Retargeted by #479.** This drove the *ambiguity* case — two ecosystem
/// manifests naming one root — which no longer exists, because a manifest name
/// is no longer an identity source. The surviving refusal is the id collision,
/// and the shape of the assertion is unchanged: refuse, name the cause, name the
/// remedy. What is deliberately gone is the trailing `forgedb project name …`
/// command, because the remedy is now a one-key edit in a file the user owns
/// rather than an act ForgeDB performs on their behalf.
#[test]
fn s2_a_piped_invocation_never_asks_and_names_a_remedy() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let base = tmp.path().canonicalize().unwrap();

    // Two roots carrying one id — what copying a project directory produces.
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
    // The remedy is an edit, not a command — asserted as an ABSENCE so that
    // reintroducing a `forgedb project <verb>` remedy has to come back through
    // this test rather than past it.
    assert!(
        !msg.contains("forgedb project name") && !msg.contains("forgedb project claim"),
        "the deleted remedy commands must not reappear in a diagnostic: {msg}"
    );

    // Nothing asked, and nothing even evaluated whether it could: `generate`
    // constructs no prompt, which is a stronger statement than "it decided not
    // to ask" and is why this is empty rather than ["no-stdin-tty"].
    assert!(
        trace_lines(&trace).is_empty(),
        "generate reaches no asking boundary at all: {:?}",
        trace_lines(&trace)
    );
}

// ---------------------------------------------------------------------------
// S3 — machine-readable stdout is *forbidden*, not merely piped
// ---------------------------------------------------------------------------

/// The trap this scenario exists for: the `Build` arm already calls
/// `ui::set_verbosity(false, true)` in these modes, which *incidentally*
/// satisfies the quiet clause. A plain "did not prompt" assertion therefore
/// passes with the `ask::forbid()` call **deleted**, and would keep passing
/// until someone changed how `--quiet` interacts with prompts — at which point
/// `docker build` would hang on a line the scaffolded `Dockerfile` runs.
///
/// So this asserts the *reason*, not the outcome. Mutation-checked both ways:
/// deleting `ask::forbid()` makes it RED; deleting the `set_verbosity` line
/// leaves it GREEN.
#[test]
fn s3_machine_readable_stdout_forbids_rather_than_merely_silencing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let root = repo_root(&tmp);
    write(&root.join("forgedb.toml"), "[project]\nid = \"clean\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    // `--plan` keeps this cheap. It conflicts with `--print-artifact` and the
    // command exits non-zero because of it — deliberately: the conflict is
    // checked inside `build::run`, which is reached only AFTER the arm has
    // forbidden asking and resolved the identity, so the trace is already
    // written. Dropping `--plan` would compile the whole generated app to
    // assert one line of a trace file.
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
    // **#479 narrowed what this can observe.** A second assertion stood here on
    // a `"forbidden"` line, proving that when askability was later evaluated the
    // forbid clause — not `--quiet` — was the vetoing one. That line came from
    // `ask::asker()`, which every `build` constructed in order to resolve
    // identity. Identity no longer asks anything, so nothing evaluates
    // askability during a build and there is no longer a decision to attribute.
    //
    // The trap the assertion guarded is unchanged and still caught: `"forbid"`
    // above is emitted by `forbid()` ITSELF, so it is absent exactly when the
    // call site is deleted, and `--quiet` cannot produce it.
}

/// The same arm without a machine-readable flag does **not** forbid.
///
/// Without this, `s3` would still pass if `forbid()` were called
/// unconditionally at the top of `main` — which would silently make every
/// future prompt unreachable.
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
    // Empty rather than `["no-stdin-tty"]` since #479: a plain build evaluates
    // askability nowhere, because nothing in `generate`/`build` asks. The
    // discrimination this scenario exists for survives intact — `s3` sees
    // `"forbid"` and this one must not.
    assert!(
        lines.is_empty(),
        "a plain build reaches no asking boundary at all: {lines:?}"
    );
}

// ---------------------------------------------------------------------------
// S4 — `dev` forbids before entering the watch loop
// ---------------------------------------------------------------------------

/// Structural: the `forbid()` call precedes the `auto_watch(` **call token**.
///
/// Anchored on the call, never on a comment or a binding name (#281): a guard
/// that matches a label passes when the work it labels has moved.
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

/// Runtime: a real `dev` process regenerates at least once, and asking stays
/// forbidden for the whole of it.
///
/// Tier 2 (`#[ignore]`) because it spawns a watcher and waits on the
/// filesystem. Its tier-1 sibling above is structural, which is what makes this
/// one's cost optional rather than load-bearing.
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

    // Wait for the loop to be up, then trigger a save.
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

// ---------------------------------------------------------------------------
// S5 — the language server can never reach an asker
// ---------------------------------------------------------------------------

/// `crates/lsp-server` owns stdin as its JSON-RPC channel: reading one byte
/// from it corrupts the protocol, and a prompt there would deadlock an editor
/// with no output anywhere.
///
/// This is satisfied *today* by construction — the LSP crate does not depend on
/// the root crate, and `forgedb lsp` hands the process over before resolving
/// anything. Which is precisely why it needs a guard: nothing currently stops
/// that changing, and the failure would be invisible until an editor hung.
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

// ---------------------------------------------------------------------------
// S19 — the widget is constructed in exactly one place
// ---------------------------------------------------------------------------
//
// #479 merged the two halves of this scenario. It asserted the property twice —
// once for `TerminalAsk` (identity's widget) and once for `TerminalPrompt`
// (migrations') — because there were two traits in front of one boundary.
// Deleting the identity questions left one widget, so the cross-file sweep that
// lived in `s19` moved into `s19c` below rather than being dropped: the sweep is
// the half that catches a NEW construction site, which is the failure the whole
// scenario exists for.

/// The prompt widget is constructed in exactly one place, past the boundary.
///
/// The boundary is only worth anything if it cannot be walked around. `prompt()`
/// is the one function that constructs `TerminalPrompt` and the one that
/// consults `may_ask()`. A second construction site anywhere would be a prompt
/// with no boundary in front of it — and it would look completely ordinary at
/// the call site, which is why this is a guard rather than a convention.
///
/// Three assertions: no file outside `ask.rs` names the widget at all, `ask.rs`
/// constructs it exactly once, and that construction is gated by `may_ask()` in
/// the same function.
///
/// Anchored on the **constructor expression**, not on a comment or a binding
/// name (#281): a guard that matches a label passes when the work it labels has
/// moved somewhere else.
///
/// It is a **test-file** guard rather than a `#[cfg(test)]` one inside
/// `ask.rs`, and that is not a preference: a module counting occurrences of a
/// literal in its own source counts its own assertion too, and reads 2 where it
/// means 1.
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

/// `std::io::IsTerminal` has exactly one call site, for the same reason.
///
/// A second one would be a second definition of "is this interactive" — and the
/// two would agree until the day one of them grew a clause.
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

// ---------------------------------------------------------------------------
// S20 — the widget itself
// ---------------------------------------------------------------------------

/// **Tier 2, and it drives a REAL terminal** — gate 2 recorded "no harness in
/// this repo can allocate a pty" as a limit, and that turned out to be false:
/// `script(1)` allocates one, on both BSD and util-linux. So the widget is
/// covered by an assertion rather than by a procedure someone has to remember
/// to run.
///
/// **Retargeted by #479.** This drove the identity prompt, which no longer
/// exists. It is retargeted rather than deleted because it is the only pty
/// coverage in the repository, and every property below is about the *widget and
/// the boundary* rather than about the question — they apply unchanged to the
/// migration prompt, which is now the one consumer.
///
/// What it proves, none of which any tier-1 scenario can see:
///
/// 1. At a terminal the boundary says `terminal` — so this test genuinely got a
///    pty. **Asserted first**, because a harness that silently failed to
///    allocate one would otherwise take the piped path and pass vacuously.
/// 2. The question renders on **stderr**: stdout is redirected to a file inside
///    the pty session, and the question must not be in it.
/// 3. The keystrokes actually drove the widget — the typed constant reaches the
///    migration record, which no piped path could have produced.
///
/// What is deliberately NOT asserted is a decline. The identity widget treated
/// `ESC` as "no answer" and fell through to the non-interactive diagnostic; this
/// one does not, by design — `ask.rs` resolves a cancelled menu to the first
/// option, because every question #374 raises is answered before anything is
/// written and a "no answer" here would be indistinguishable from a session that
/// should already have errored.
#[test]
#[ignore = "tier 2: allocates a pty via script(1)"]
fn s20_the_widget_renders_on_stderr_and_the_answer_lands() {
    // A required add with no `@default` — the differ cannot prove a value, so a
    // terminal is asked and a pipe is refused. Same fixture shape as
    // `migrate_answers_test`'s scenario 6.
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

    // Select the first option ("a constant value"), then type the constant.
    //
    // Deterministic input matters more than brevity here: every option on this
    // menu except the escape hatch asks a follow-up, so a session that sends one
    // keystroke and stops does not decline — it BLOCKS on the second question,
    // which is a hang rather than a failure.
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

    // The question is on STDERR: the redirect above captured stdout, and it must
    // not hold it. `forgedb migrate create … > log` has to still show the
    // question to the person running it.
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

    // The answer reached the record — which is what proves the keystrokes drove
    // the widget rather than the run having taken some non-interactive path.
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
            // CSI and friends: skip until a final byte in @..~
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

/// Run a shell command inside a real pty, feeding `input` to it.
///
/// `script(1)` is the portable-enough way to get one: BSD/macOS takes the
/// command as argv, util-linux takes it as `-c`. Both forms are tried, and a
/// failure to obtain a pty is caught by the caller's `terminal` assertion
/// rather than by a silent skip here.
fn pty_run(cwd: &Path, home: &Path, trace: &Path, input: &str, command: &str) -> String {
    // BSD: `script -q /dev/null sh -c '<command>'`
    // util-linux: `script -qec '<command>' /dev/null`
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

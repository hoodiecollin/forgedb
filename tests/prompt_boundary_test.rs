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

/// The contract for every scripted invocation: identical diagnostic, identical
/// non-zero exit, and — the only change — a **command** that records the answer.
#[test]
fn s2_a_piped_invocation_never_asks_and_names_a_command() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let root = ambiguous_root(&tmp);

    let out = run_traced(
        &root,
        home.path(),
        &trace,
        &[
            "generate",
            "rust",
            "--output",
            root.join("generated").to_str().unwrap(),
        ],
    );

    assert!(!out.status.success(), "ambiguity is still refused");
    let msg = combined(&out);
    // Today's diagnostic, unchanged.
    assert!(msg.contains("cannot pick a project name"), "{msg}");
    assert!(msg.contains("backend") && msg.contains("storefront"), "{msg}");
    assert!(msg.contains("[project].name"), "{msg}");
    // …plus the persisting act, carrying the schema the failing invocation
    // resolved. Without `--schema` the same command run from another directory
    // in a monorepo resolves a DIFFERENT project — copy-pasteable and wrong.
    assert!(msg.contains("forgedb project name"), "{msg}");
    assert!(msg.contains("--schema"), "names the resolved schema: {msg}");

    assert_eq!(
        trace_lines(&trace),
        vec!["no-stdin-tty".to_string()],
        "a piped invocation decided ONCE, and decided it could not ask"
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
    write(&root.join("forgedb.toml"), "[project]\nname = \"clean\"\n");
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
        lines.contains(&"forbidden".to_string()),
        "…and the latch must be what decided it — `quiet` here would mean the \
         explicit forbid is doing nothing and a `--quiet` change could \
         reintroduce the hang: {lines:?}\n{msg}"
    );
    assert!(
        !lines.contains(&"terminal".to_string()),
        "a machine-readable stdout may never ask: {lines:?}"
    );
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
    write(&root.join("forgedb.toml"), "[project]\nname = \"clean\"\n");
    write(&root.join("schema.forge"), SCHEMA);

    run_traced(&root, home.path(), &trace, &["build", "--plan"]);

    let lines = trace_lines(&trace);
    assert!(
        !lines.contains(&"forbid".to_string()),
        "forbidding is per-mode, not global: {lines:?}"
    );
    assert_eq!(
        lines,
        vec!["no-stdin-tty".to_string()],
        "piped, and that is the only reason it did not ask: {lines:?}"
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
    write(&root.join("forgedb.toml"), "[project]\nname = \"watched\"\n");
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

/// The boundary is only worth anything if it cannot be walked around.
///
/// `ask::asker()` is the one function that constructs `TerminalAsk`, and it is
/// the one that consults `may_ask()`. A second construction site anywhere would
/// be a prompt with no boundary in front of it — and it would look completely
/// ordinary at the call site, which is why this is a guard rather than a
/// convention.
///
/// Anchored on the **constructor expression**, not on a comment or a binding
/// name (#281): a guard that matches a label passes when the work it labels has
/// moved somewhere else.
#[test]
fn s19_the_widget_is_constructed_in_exactly_one_place() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for file in rust_files(&src_dir) {
        if file.file_name().and_then(|n| n.to_str()) == Some("ask.rs") {
            continue;
        }
        if std::fs::read_to_string(&file)
            .unwrap_or_default()
            .contains("TerminalAsk")
        {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "the prompt widget must be reachable only through `ask::asker()`, which \
         is what checks whether asking is allowed at all:\n  {}",
        offenders.join("\n  ")
    );

    let ask_rs = std::fs::read_to_string(src_dir.join("ask.rs")).unwrap();
    assert_eq!(
        ask_rs.matches("Box::new(TerminalAsk)").count(),
        1,
        "exactly one construction site"
    );
    // …and it is inside `asker()`, past `may_ask()`.
    let asker_fn = ask_rs
        .split("pub fn asker()")
        .nth(1)
        .expect("asker() exists");
    let body = &asker_fn[..asker_fn.find("\n}\n").expect("a function body")];
    assert!(
        body.contains("may_ask()") && body.contains("Box::new(TerminalAsk)"),
        "the construction is gated by the boundary in the same function: {body}"
    );
}

/// The **second** question kind shares the first one's boundary (#374).
///
/// `migrate create` asks a different shape of question — pick one of N, or
/// yes/no — and needs its own trait, because `Asker`'s `Question` is a closed
/// two-variant enum about project identity. What it must NOT have is its own
/// *boundary*: a second definition of "is this interactive" would agree with
/// this one until the day either grew a clause.
///
/// So the row is here, beside `TerminalAsk`'s, and it asserts the same two
/// things: `prompt()` is the only constructor of `TerminalPrompt`, and it is
/// gated by `Askability::may_ask()` in the same function.
///
/// It is a **test-file** guard rather than a `#[cfg(test)]` one inside
/// `ask.rs`, and that is not a preference: a module counting occurrences of a
/// literal in its own source counts its own assertion too, and reads 2 where it
/// means 1.
#[test]
fn s19c_the_migrate_prompt_shares_the_one_boundary() {
    let ask_rs = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ask.rs"),
    )
    .expect("src/ask.rs");
    assert_eq!(
        ask_rs.matches("Box::new(TerminalPrompt)").count(),
        1,
        "`TerminalPrompt` has exactly one constructor, like `TerminalAsk`"
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
/// What it proves, none of which any tier-1 scenario can see:
///
/// 1. At a terminal the boundary says `terminal` — so this test genuinely got a
///    pty. **Asserted first**, because a harness that silently failed to
///    allocate one would otherwise take the piped path and pass vacuously.
/// 2. The question renders on **stderr**: stdout is redirected to a file inside
///    the pty session, and the question must not be in it.
/// 3. Answering persists, and a second run asks nothing.
/// 4. `ESC` is a decline — the unchanged diagnostic and the unchanged exit
///    status (10, `ConfigDiagnostic`), not a third outcome.
#[test]
#[ignore = "tier 2: allocates a pty via script(1)"]
fn s20_the_widget_renders_on_stderr_and_cancels_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = ambiguous_root(&tmp);
    let trace = tmp.path().join("ask.trace");
    let stdout_log = tmp.path().join("stdout.log");
    let out_dir = root.join("generated");

    // Answer the select by taking its default (the first candidate, from
    // `Cargo.toml`).
    let session = pty_run(
        &root,
        home.path(),
        &trace,
        "\n",
        &format!(
            "{BIN} generate rust --output {} > {}",
            out_dir.display(),
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

    let config = std::fs::read_to_string(root.join("forgedb.toml"))
        .expect("answering the prompt persisted the answer");
    assert!(config.contains("name = \"backend\""), "{config}");

    // The question is on STDERR: the redirect above captured stdout, and it
    // must not hold it. `forgedb generate > build.log` has to still show the
    // question to the person running it.
    let captured = std::fs::read_to_string(&stdout_log).unwrap_or_default();
    assert!(
        !captured.contains("Which name is this project"),
        "the question leaked into a captured stdout:\n{captured}"
    );
    assert!(
        session.contains("Which name is this project"),
        "…and it must have been on stderr, which the pty saw:\n{session}"
    );

    // A second, wholly non-interactive run asks nothing.
    let again = run_traced(
        &root,
        home.path(),
        &trace,
        &[
            "generate",
            "rust",
            "--force",
            "--output",
            out_dir.to_str().unwrap(),
        ],
    );
    assert!(again.status.success(), "{}", combined(&again));

    // ESC declines: the unchanged diagnostic, and the unchanged exit status.
    let tmp2 = tempfile::tempdir().unwrap();
    let home2 = tempfile::tempdir().unwrap();
    let root2 = ambiguous_root(&tmp2);
    let trace2 = tmp2.path().join("ask.trace");
    let escaped = pty_run(
        &root2,
        home2.path(),
        &trace2,
        "\x1b",
        &format!(
            "{BIN} generate rust --output {}; echo FORGEDB_EXIT=$?",
            root2.join("generated").display()
        ),
    );
    assert_eq!(trace_lines(&trace2), vec!["terminal".to_string()]);
    assert!(
        escaped.contains("cannot pick a project name"),
        "ESC produces the UNCHANGED diagnostic:\n{escaped}"
    );
    assert!(
        escaped.contains("FORGEDB_EXIT=10"),
        "…and the unchanged exit status (ConfigDiagnostic = 10):\n{escaped}"
    );
    assert!(
        !root2.join("forgedb.toml").exists(),
        "a decline writes nothing"
    );
}

/// The other widget path: a dead claim, offered a take-over.
///
/// Tier 2 for the same reason as S20, and worth its own scenario because the
/// *offered answers differ by holder liveness* — which is the sharpest statement
/// of why this decision is not expressible as a flag, and is the branch a
/// scripted `Asker` exercises without ever rendering.
///
/// It also asserts the rendered prose has no absurd whitespace run. That is not
/// fussiness: these strings are the only user-facing text in the issue with no
/// other assertion on their content, and a mangled multi-line literal renders
/// perfectly plausibly to `cargo build` while reading as broken to a human.
#[test]
#[ignore = "tier 2: allocates a pty via script(1)"]
fn s20b_a_dead_claim_is_offered_a_take_over_at_a_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("ask.trace");
    let base = tmp.path().canonicalize().unwrap();
    let a = base.join("a");
    std::fs::create_dir_all(a.join(".git")).unwrap();
    write(&a.join("forgedb.toml"), "[project]\nname = \"moved\"\n");
    write(&a.join("schema.forge"), SCHEMA);

    let first = run_traced(
        &a,
        home.path(),
        &trace,
        &[
            "generate",
            "rust",
            "--output",
            base.join("gen1").to_str().unwrap(),
        ],
    );
    assert!(first.status.success(), "{}", combined(&first));

    // Move the project. Nothing tells the ledger.
    let b = base.join("b");
    std::fs::rename(&a, &b).unwrap();
    std::fs::remove_file(&trace).unwrap();

    let session = pty_run(
        &b,
        home.path(),
        &trace,
        "\n",
        &format!(
            "{BIN} generate rust --force --output {}",
            base.join("gen2").display()
        ),
    );
    assert_eq!(
        trace_lines(&trace),
        vec!["terminal".to_string()],
        "no pty, no scenario:\n{session}"
    );
    assert!(
        session.contains("no longer exists"),
        "the terminal path says the holding root is gone:\n{session}"
    );
    assert!(
        session.contains("Take over the claim"),
        "…and offers the take-over, which a LIVE holder is never offered:\n{session}"
    );
    assert!(
        session.contains("unmounted"),
        "…carrying the caveat that made automatic reaping wrong:\n{session}"
    );

    // Rendered prose, not source shape: a collapsed multi-line literal compiles
    // and renders as a wall of spaces.
    for line in strip_ansi(&session).lines() {
        assert!(
            !line.trim_end().contains("        "),
            "a rendered prompt line carries a whitespace run, which means a \
             multi-line string literal lost its continuations: {line:?}"
        );
    }

    // Accepting wrote the LEDGER and left the config alone.
    assert_eq!(
        std::fs::read_to_string(b.join("forgedb.toml")).unwrap(),
        "[project]\nname = \"moved\"\n",
        "a take-over never touches the project's config"
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join("ledger/moved.claim"))
            .unwrap()
            .trim(),
        b.to_string_lossy(),
        "…and the ledger now points at us"
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

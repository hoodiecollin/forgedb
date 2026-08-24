//! Resolution with an asker — the #367 scenarios (gate #371, S13–S15, S18).
//!
//! **These are the scenarios the whole four-layer shape exists for.** No
//! terminal is involved anywhere: `ScriptedAsk` below is an ordinary `Asker`,
//! so the *decision* and the *act* run in-process with an answer supplied, and
//! the widget is not in the picture at all. That is the property gate 1 asked
//! for — if the persisting act and the askable decision were fused to the
//! prompt, the interactive path would ship with no test that ever executed it,
//! because the repo's subprocess harness pipes stdio by construction.
//!
//! Note gate 2 placed these in `tests/project_identity_test.rs`. They are here
//! instead for one concrete reason: they touch the claim ledger, so they need
//! `FORGEDB_HOME` redirected, and the only in-process way to do that is
//! `std::env::set_var` — which is process-global and unsound to run concurrently
//! with another thread building a `Command` env. `project_identity_test.rs` is
//! full of such threads. **Every** test in THIS binary takes the same lock, so
//! no read races a write. `scenario_14` stays exactly where it is, untouched
//! (S16).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use forgedb::ask::NeverAsk;
use forgedb::cache;
use forgedb::commands::init::{self, InitOptions};
use forgedb::project::{self, Answer, Asker, Chain, IdSource, Question};
use forgedb::Result;

const SCHEMA: &str = "Note {\n  id: +uuid\n  body: string\n}\n";

// ---------------------------------------------------------------------------
// A scripted asker — the widget's stand-in, and the point of the `Asker` seam
// ---------------------------------------------------------------------------

/// Answers one question with a canned reply and records what it was asked.
///
/// The recording is not decoration: S13 asserts the *payload* travelled from
/// `identify_or_ask` rather than being re-derived at the prompt, which is the
/// drift class this repo keeps getting bitten by. Two derivations of "which
/// manifests name this root" would agree today and diverge silently later.
struct ScriptedAsk {
    answer: Option<Answer>,
    consent: bool,
    seen: Mutex<Vec<Question>>,
}

impl ScriptedAsk {
    fn name(n: &str) -> ScriptedAsk {
        ScriptedAsk {
            answer: Some(Answer::Name(n.to_string())),
            consent: true,
            seen: Mutex::new(Vec::new()),
        }
    }
    fn take_over() -> ScriptedAsk {
        ScriptedAsk {
            answer: Some(Answer::TakeOverClaim),
            consent: true,
            seen: Mutex::new(Vec::new()),
        }
    }
    /// A user who was asked and said no. Must be indistinguishable from a
    /// context that could not ask at all.
    fn declines() -> ScriptedAsk {
        ScriptedAsk {
            answer: None,
            consent: false,
            seen: Mutex::new(Vec::new()),
        }
    }
    fn asked(&self) -> Vec<Question> {
        self.seen.lock().unwrap().clone()
    }
}

impl Asker for ScriptedAsk {
    fn ask(&self, q: &Question) -> Result<Option<Answer>> {
        self.seen.lock().unwrap().push(q.clone());
        Ok(self.answer.clone())
    }
    fn confirm_edit(&self, _path: &Path) -> Result<bool> {
        Ok(self.consent)
    }
}

// ---------------------------------------------------------------------------
// Test plumbing
// ---------------------------------------------------------------------------

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
    _dir: tempfile::TempDir,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { std::env::set_var(cache::HOME_ENV, v) },
            None => unsafe { std::env::remove_var(cache::HOME_ENV) },
        }
    }
}

/// Point `FORGEDB_HOME` at a fresh tempdir for the duration of one test.
fn scoped_home() -> EnvGuard {
    let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var_os(cache::HOME_ENV);
    let dir = tempfile::tempdir().expect("tempdir");
    unsafe { std::env::set_var(cache::HOME_ENV, dir.path()) };
    EnvGuard {
        _lock: lock,
        previous,
        _dir: dir,
    }
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn repo_root(dir: &tempfile::TempDir) -> PathBuf {
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    root
}

/// A root two ecosystem manifests name, with a schema and no `forgedb.toml`.
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

// ---------------------------------------------------------------------------
// S13 — the resolution asks, and the answer persists
// ---------------------------------------------------------------------------

/// **The scenario the entire layering exists for**, and it runs with no
/// terminal in the process.
///
/// The second half is the part that makes this issue worth doing: a *later,
/// independent, non-interactive* resolution returns the same id having asked
/// nothing. An answer that lived in one `argv` would not survive that — the next
/// `forgedb generate` in the scaffolded `Dockerfile` would resolve a different
/// project, get its own cache, its own `Cargo.lock` and its own `target/`, and
/// say nothing about it.
#[test]
fn s13_the_resolution_asks_and_the_answer_persists() {
    let _home = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = ambiguous_root(&tmp);
    let schema = root.join("schema.forge");

    let asker = ScriptedAsk::name("picked");
    let chain = Chain::walk_from_schema(&schema).unwrap();
    let id = project::identify_and_claim_with(&chain, &asker).unwrap();

    assert_eq!(id.name, "picked");
    assert_eq!(
        id.source,
        IdSource::Explicit,
        "the answer became a declared name, not a remembered one"
    );

    // The payload travelled from `identify_or_ask` rather than being re-derived
    // at the prompt: both pairs, in MANIFESTS order.
    let asked = asker.asked();
    assert_eq!(asked.len(), 1, "asked exactly once: {asked:?}");
    match &asked[0] {
        Question::WhichName {
            candidates,
            root: asked_root,
            schema_hint,
        } => {
            assert_eq!(
                candidates,
                &vec![
                    ("Cargo.toml", "backend".to_string()),
                    ("package.json", "storefront".to_string()),
                ]
            );
            assert_eq!(asked_root, &root);
            assert_eq!(
                schema_hint.as_deref(),
                Some(schema.as_path()),
                "the question carries the schema, so the remedy it renders \
                 resolves THIS project"
            );
        }
        other => panic!("wrong question: {other:?}"),
    }

    // It was written where identity is keyed, and it parses.
    let config = root.join("forgedb.toml");
    let text = std::fs::read_to_string(&config).expect("a config was created");
    assert!(text.contains("name = \"picked\""), "{text}");

    // …and a second, wholly non-interactive resolution agrees, asking nothing.
    let again = project::identify_and_claim(&Chain::walk(&root).unwrap()).unwrap();
    assert_eq!(again.name, "picked");
    assert_eq!(again.source, IdSource::Explicit);
}

// ---------------------------------------------------------------------------
// S14 — a declined answer is exactly today's error
// ---------------------------------------------------------------------------

/// Declining must not be a third behaviour.
///
/// "Could not ask" and "was asked and said no" are the same path by
/// construction — `Asker::ask` returns `Ok(None)` for both — and this asserts
/// the strings are byte-identical rather than merely similar. A prompt only ever
/// fills an answer that is otherwise absent; it never changes what happens when
/// the answer stays absent.
#[test]
fn s14_a_declined_answer_is_exactly_the_non_interactive_error() {
    let _home = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let root = ambiguous_root(&tmp);
    let schema = root.join("schema.forge");

    let declined = project::identify_and_claim_with(
        &Chain::walk_from_schema(&schema).unwrap(),
        &ScriptedAsk::declines(),
    )
    .expect_err("a declined answer is still an error");

    let never = project::identify_and_claim_with(
        &Chain::walk_from_schema(&schema).unwrap(),
        &NeverAsk,
    )
    .expect_err("so is no answer at all");

    assert_eq!(declined.to_string(), never.to_string());
    assert!(
        !root.join("forgedb.toml").exists(),
        "a decline writes nothing"
    );
}

// ---------------------------------------------------------------------------
// S15 — a stale-claim answer writes the ledger and NOT the config
// ---------------------------------------------------------------------------

/// The C1 line, asserted from the new direction.
///
/// The ledger records *who currently holds an id* — detection state, in a
/// directory GC may empty at any time. A chosen *name* is a resolution and may
/// never live there, or wiping the cache would resurrect a resolved collision as
/// a silent merge of two projects. So a take-over writes the ledger and leaves
/// the config alone; the project keeps its name.
#[test]
fn s15_a_take_over_writes_the_ledger_and_not_the_config() {
    let _home = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let a = base.join("a");
    std::fs::create_dir_all(a.join(".git")).unwrap();
    let config_text = "[project]\nname = \"shared\"\n";
    write(&a.join("forgedb.toml"), config_text);
    write(&a.join("schema.forge"), SCHEMA);

    // `a` claims the id, then moves away. Nothing removes its claim.
    let first =
        project::identify_and_claim(&Chain::walk_from_schema(&a.join("schema.forge")).unwrap())
            .unwrap();
    assert_eq!(first.name, "shared");
    let b = base.join("b");
    std::fs::rename(&a, &b).unwrap();

    let asker = ScriptedAsk::take_over();
    let id = project::identify_and_claim_with(
        &Chain::walk_from_schema(&b.join("schema.forge")).unwrap(),
        &asker,
    )
    .unwrap();

    assert_eq!(id.name, "shared", "the project kept its name");
    assert_eq!(
        std::fs::read_to_string(b.join("forgedb.toml")).unwrap(),
        config_text,
        "a take-over writes the LEDGER; the config is not touched"
    );

    // The question told the asker what it needed in order to have an answer set
    // at all — whether the holding root still exists. That is a fact about the
    // filesystem the user cannot know when they type a command, which is the
    // strongest reason this decision is not expressible as a flag.
    match &asker.asked()[0] {
        Question::Collision {
            holder_exists,
            held_by,
            ..
        } => {
            assert!(!holder_exists, "the holder is gone");
            assert_eq!(held_by, &a);
        }
        other => panic!("wrong question: {other:?}"),
    }

    // The ledger now points at us, so a plain non-interactive run succeeds.
    let holder = project::held_by("shared").unwrap().expect("a holder");
    assert_eq!(holder.path, b);
    assert!(holder.exists);
    project::identify_and_claim(&Chain::walk_from_schema(&b.join("schema.forge")).unwrap())
        .unwrap();
}

/// A LIVE holder is never displaced by an answer, only by an explicit
/// `--force`. A path can be absent because a volume is unmounted; a path that is
/// *present* is a real second project.
#[test]
fn s15b_a_take_over_answer_cannot_displace_a_live_holder() {
    let _home = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    for side in ["one", "two"] {
        let root = base.join(side);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        write(&root.join("forgedb.toml"), "[project]\nname = \"clash\"\n");
        write(&root.join("schema.forge"), SCHEMA);
    }
    project::identify_and_claim(
        &Chain::walk_from_schema(&base.join("one/schema.forge")).unwrap(),
    )
    .unwrap();

    let err = project::identify_and_claim_with(
        &Chain::walk_from_schema(&base.join("two/schema.forge")).unwrap(),
        &ScriptedAsk::take_over(),
    )
    .expect_err("a live holder is not displaced by an answer");
    assert!(err.to_string().contains("--force"), "{err}");
    assert_eq!(
        project::held_by("clash").unwrap().unwrap().path,
        base.join("one"),
        "the ledger is unchanged"
    );
}

/// Answering a live collision with a NEW NAME resolves it — in this project's
/// own config, which is what survives a cache wipe.
#[test]
fn s15c_a_live_collision_is_resolved_by_a_name_in_our_own_config() {
    let _home = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    for side in ["one", "two"] {
        let root = base.join(side);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        write(&root.join("forgedb.toml"), "[project]\nname = \"clash\"\n");
        write(&root.join("schema.forge"), SCHEMA);
    }
    project::identify_and_claim(
        &Chain::walk_from_schema(&base.join("one/schema.forge")).unwrap(),
    )
    .unwrap();

    let id = project::identify_and_claim_with(
        &Chain::walk_from_schema(&base.join("two/schema.forge")).unwrap(),
        &ScriptedAsk::name("resolved"),
    )
    .unwrap();
    assert_eq!(id.name, "resolved");
    assert!(
        std::fs::read_to_string(base.join("two/forgedb.toml"))
            .unwrap()
            .contains("name = \"resolved\""),
        "the resolution is in the project's OWN config, never in the ledger"
    );
    // The other project is untouched.
    assert_eq!(
        project::held_by("clash").unwrap().unwrap().path,
        base.join("one")
    );
}

// ---------------------------------------------------------------------------
// S18 — `init` offers a new name; piped, it refuses
// ---------------------------------------------------------------------------

/// C12's `init` half: report a taken id where the name is being *chosen*,
/// rather than at the first `generate` — by which point the user has a
/// scaffolded tree whose name they now have to change.
///
/// The directory keeps the name the user typed. A project id and a directory
/// are different things, and a prompt that renamed the directory would be
/// answering a question nobody asked.
#[test]
fn s18_init_offers_a_new_name_when_the_id_is_taken() {
    let _home = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();

    // Something already holds `taken`.
    let holder = base.join("holder");
    std::fs::create_dir_all(holder.join(".git")).unwrap();
    write(&holder.join("forgedb.toml"), "[project]\nname = \"taken\"\n");
    write(&holder.join("schema.forge"), SCHEMA);
    project::identify_and_claim(&Chain::walk_from_schema(&holder.join("schema.forge")).unwrap())
        .unwrap();

    let cwd = std::env::current_dir().unwrap();
    // `init` resolves its target directory relative to the CWD, so the scenario
    // has to stand somewhere. Safe here: every test in this binary holds the
    // same lock.
    std::env::set_current_dir(&base).unwrap();
    let result = init::run_with(
        InitOptions {
            project_name: "taken".to_string(),
            project_name_override: None,
            template: None,
            rust: false,
            api_only: false,
            isolated: Some(true),
        },
        &ScriptedAsk::name("other"),
    );
    std::env::set_current_dir(cwd).unwrap();
    result.expect("an answered collision scaffolds");

    let scaffolded = base.join("taken");
    assert!(
        scaffolded.is_dir(),
        "the DIRECTORY keeps the name the user typed"
    );
    let config = std::fs::read_to_string(scaffolded.join("forgedb.toml")).unwrap();
    assert!(
        config.contains("name = \"other\""),
        "…and the PROJECT takes the answered name: {config}"
    );
}

/// The same `init`, with no answer available: today's refusal, unchanged, now
/// also naming the flag that answers it non-interactively.
#[test]
fn s18b_init_refuses_a_taken_id_when_it_cannot_ask() {
    let _home = scoped_home();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();

    let holder = base.join("holder");
    std::fs::create_dir_all(holder.join(".git")).unwrap();
    write(&holder.join("forgedb.toml"), "[project]\nname = \"taken\"\n");
    write(&holder.join("schema.forge"), SCHEMA);
    project::identify_and_claim(&Chain::walk_from_schema(&holder.join("schema.forge")).unwrap())
        .unwrap();

    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&base).unwrap();
    let result = init::run_with(
        InitOptions {
            project_name: "taken".to_string(),
            project_name_override: None,
            template: None,
            rust: false,
            api_only: false,
            isolated: Some(true),
        },
        &NeverAsk,
    );
    std::env::set_current_dir(cwd).unwrap();

    let err = result.expect_err("a taken id is refused when nothing can answer");
    let msg = err.to_string();
    assert!(msg.contains("already claimed"), "{msg}");
    assert!(msg.contains("--project-name"), "names the flag: {msg}");
    assert!(
        !base.join("taken").exists(),
        "the refusal happens before anything is scaffolded"
    );
}

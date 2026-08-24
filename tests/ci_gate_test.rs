//! The test gate guards itself (#390).
//!
//! ForgeDB ran no tests in CI at all. Every one of the eleven workflows was a release,
//! publish, deploy or scope-gate job, so the suite ran only when a person chose to run
//! it. That cost two defects before it was noticed: #381, whose generated driver called
//! an API deleted two weeks earlier, and #386, which panicked before reaching its
//! assertion. Both were found by hand.
//!
//! #390 adds the two jobs that close it. This file is the part that keeps them honest,
//! and it runs in TIER 1 — so the control that guards the tiers is itself gated.
//!
//! ## What this can and cannot prove
//!
//! It cannot prove a workflow runs; only GitHub can. What it CAN prove is the set of
//! properties that, when they break, break *silently* — where the job still runs, still
//! goes green, and simply covers less than it claims:
//!
//!   * the tier-2 command stays workspace-level, rather than degrading into a
//!     hand-maintained list of test binaries that looks complete and is not;
//!   * the one test deliberately excluded from tier 2 still matches its `--skip`
//!     pattern, and still has a home on the `main` surface;
//!   * tier 1 still builds the examples, which no test flag covers;
//!   * a failed nightly still has the permission and the step it needs to report.
//!
//! Every assertion below anchors on the token that does the WORK — the command string,
//! the skip pattern, the permission — never on a name or a comment that merely labels
//! it. A guard anchored on a label passes vacuously the moment the label moves.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// Collapse a shell continuation (`\` + newline + indent) so a recipe or a `run:` block
/// can be matched as one line. Without this every assertion below would depend on where
/// the author happened to wrap, which is not a property worth guarding.
fn unwrap_continuations(s: &str) -> String {
    let joined = s.replace("\\\n", " ");
    Regex::new(r"\s+").unwrap().replace_all(&joined, " ").trim().to_string()
}

/// The body of a `make` target, with continuations joined.
fn make_recipe(target: &str) -> String {
    let mk = read("Makefile");
    let head = format!("\n{target}:\n");
    let start = mk
        .find(&head)
        .unwrap_or_else(|| panic!("Makefile has no `{target}:` target"))
        + head.len();
    let rest = &mk[start..];
    // A recipe runs to the first line that is neither indented nor blank.
    let mut body = String::new();
    for line in rest.lines() {
        if line.starts_with('\t') || line.trim().is_empty() {
            body.push_str(line);
            body.push('\n');
        } else {
            break;
        }
    }
    unwrap_continuations(&body)
}

/// Every `#[ignore]`d test function in `tests/`, by name.
///
/// Derived from source rather than from a hardcoded list, so adding an ignored test
/// cannot silently fall outside these guards.
///
/// Deliberately NOT derived from `cargo test -- --ignored --list`: that would mean a
/// tier-1 test shelling out to a full workspace build. It also reports one entry more
/// than there are tests — a ```rust,ignore``` doc-block in crates/codegen/src/rust.rs,
/// which under `--ignored` reports ok in 0.00s having compiled nothing. That entry is
/// vacuous; counting it as a test is how an earlier draft of #390's design reached the
/// wrong conclusion about why this matters.
fn ignored_tests() -> BTreeSet<String> {
    let re = Regex::new(r"#\[ignore[^\]]*\]\s*\n\s*(?:pub\s+)?fn\s+(\w+)").unwrap();
    let dir = repo_root().join("tests");
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).expect("tests/ is readable") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("readable test file");
        for caps in re.captures_iter(&src) {
            out.insert(caps[1].to_string());
        }
    }
    assert!(
        !out.is_empty(),
        "found no #[ignore]d tests — the parser has drifted from the source, and every \
         assertion keyed on this set would now pass vacuously"
    );
    out
}

/// The `--skip <pattern>` arguments of a command line.
fn skip_patterns(cmd: &str) -> Vec<String> {
    let re = Regex::new(r"--skip\s+(\S+)").unwrap();
    re.captures_iter(cmd).map(|c| c[1].to_string()).collect()
}

/// A workflow's text **with whole-line `#` comments removed**.
///
/// Stripping is the default here, and that is not fastidiousness — it is a missing guard.
/// These workflows carry long headers explaining exactly what each assertion below
/// protects, so nearly every needle this file searches for also appears in prose a few
/// lines above the thing that does the work. Three assertions were originally written
/// against raw file text; the mutation harness found two of them still passing after the
/// command they guard had been broken, satisfied by the comment justifying the assertion.
///
/// A guard that its own rationale can satisfy is not a guard.
fn workflow(name: &str) -> String {
    let raw = read(&format!(".github/workflows/{name}"));
    raw.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A workflow's `on:` block, comments stripped.
///
/// Scoped to the trigger rather than matched file-wide for the usual reason: the
/// word `main` appears in nearly every one of these files, in prose explaining
/// exactly why the trigger says `main` — so a file-wide assertion is satisfied by
/// its own rationale.
///
/// Panics if the block is absent. A workflow with no `on:` never runs, and a
/// helper that returned `""` there would make every assertion below pass on it.
fn trigger_block(workflow_file: &str) -> String {
    let src = workflow(workflow_file);
    let start = src.find("\non:\n").unwrap_or_else(|| {
        panic!(
            "{workflow_file} has no top-level `on:` block — it can never run, and \
             every guard keyed to its triggers would pass vacuously"
        )
    }) + "\non:\n".len();
    let mut out = String::new();
    for line in src[start..].lines() {
        if !line.trim().is_empty() && !line.starts_with(char::is_whitespace) {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    assert!(
        !out.trim().is_empty(),
        "{workflow_file}'s `on:` block is empty"
    );
    out
}

/// The shell command of the single-line `run:` step with the given `name:`.
///
/// Scoped to one step on purpose: asserting against the whole workflow means any other
/// step — or any comment — can satisfy the assertion.
fn run_command(workflow_file: &str, step_name: &str) -> String {
    let src = workflow(workflow_file);
    let needle = format!("- name: {step_name}");
    let start = src.find(&needle).unwrap_or_else(|| {
        panic!("{workflow_file} has no step named {step_name:?} — the guard keyed to it is now vacuous")
    });
    let rest = &src[start..];
    let run = rest.find("run:").unwrap_or_else(|| {
        panic!("step {step_name:?} in {workflow_file} has no `run:`")
    });
    // Single-line `run:` — up to the newline, with continuations joined.
    let tail = &rest[run..];
    let end = tail.find('\n').unwrap_or(tail.len());
    unwrap_continuations(&tail[..end])
}

/// The shell of a step whose `run:` is a BLOCK SCALAR (`run: |`).
///
/// [`run_command`] reads to the end of the `run:` line, which for a block scalar
/// is the literal `|` — so a `!contains` guard built on it passes having examined
/// one character, and a `contains` guard fails for a reason that looks like the
/// step being wrong. That is latent rather than live today (both existing callers
/// key single-line steps), and it stays latent because the block-scalar form is
/// here rather than because nobody has reached for it.
///
/// Comments are stripped, like [`workflow`] does: these steps explain themselves
/// in prose, and a needle that its own rationale can satisfy is not a guard.
fn run_block(workflow_file: &str, step_name: &str) -> String {
    let src = read(&format!(".github/workflows/{workflow_file}"));
    let needle = format!("- name: {step_name}");
    let start = src.find(&needle).unwrap_or_else(|| {
        panic!("{workflow_file} has no step named {step_name:?} — the guard keyed to it is now vacuous")
    });
    // The step's own indent, and the slice that belongs to THIS step: up to the
    // next `- name:` at the same depth. Bounding first is load-bearing — an
    // unbounded `find("run: |")` on a step whose `run:` is single-line silently
    // returns the NEXT step's script, so the guard stays live and aimed at the
    // wrong subject. That is the widening class this repo has already paid for,
    // and it is why the miss below panics.
    let indent = src[..start]
        .rfind('\n')
        .map(|nl| start - nl - 1)
        .unwrap_or(0);
    let step_marker = format!("\n{}- name: ", " ".repeat(indent));
    let rest = &src[start..];
    let rest = match rest[1..].find(&step_marker) {
        Some(next) => &rest[..next + 1],
        None => rest,
    };

    let run = rest.find("run: |").unwrap_or_else(|| {
        panic!(
            "step {step_name:?} in {workflow_file} has no block-scalar `run: |`. \
             If it became a single-line `run:`, use `run_command` — this helper \
             would otherwise return the whole rest of the file."
        )
    });
    let body = &rest[run + "run: |".len()..];

    // A block scalar runs to the first line that is neither blank nor indented
    // deeper than the step's own `- name:` key.
    let mut out = String::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            out.push('\n');
            continue;
        }
        let this = line.len() - line.trim_start().len();
        if this <= indent {
            break;
        }
        if line.trim_start().starts_with('#') {
            continue;
        }
        out.push_str(line.trim_start());
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// S337 — the reclose LOADS what it built.
// ---------------------------------------------------------------------------

/// #337's delivered artifacts are only proven by loading them, and the reclose
/// is the only place that happens against registry-resolved substrate.
///
/// Each assertion anchors on the COMMAND that does the work — the interpreter
/// invocation, the compile — never on a step name or on a comment. Dropping one
/// must fail loudly here rather than leaving an arm that still passes while
/// covering less than it claims.
#[test]
fn s337_each_reclose_arm_loads_the_artifact_it_delivered() {
    for (step, needles) in [
        (
            "2/6 ffi — generate ffi, then forgedb build",
            vec!["cc -I generated/ffi", "generated/ffi/libforgedb.a", "./csmoke"],
        ),
        (
            "3/6 napi — generate node --runtime, then forgedb build",
            vec!["node nodesmoke.js", "generated/napi/forgedb.node"],
        ),
        (
            "4/6 pyo3 — generate python --runtime, then forgedb build",
            vec!["python3 pysmoke.py", "generated/pyo3/_forgedb_native.abi3.so"],
        ),
    ] {
        let body = run_block("substrate-reclose.yml", step);
        for needle in needles {
            assert!(
                body.contains(needle),
                "reclose step {step:?} no longer runs `{needle}` — the arm still \
                 builds, still goes green, and no longer proves the delivered \
                 artifact can be loaded:\n{body}"
            );
        }
    }
}

/// The Go reclose must exercise the `init()` check's CALL SITE, not merely a
/// matching pair.
///
/// A run where both halves agree proves the check does not false-positive. Only
/// a deliberately mismatched pair proves it runs at all (#345's lesson). The
/// mismatch is induced with a `[storage]` knob because that changes durability
/// semantics and not one exported symbol — the case the linker cannot see.
#[test]
fn s337_the_go_reclose_proves_the_init_check_executes() {
    let body = run_block("go-reclose.yml", "Reclose — generate, build, link, run");
    for needle in [
        // The induced mismatch: regenerate without rebuilding the archive.
        "fsync = \"always\"",
        "generate go --runtime --force",
        // …and the assertion that the binary REFUSES to run.
        "different schema",
        "forgedb build",
    ] {
        assert!(
            body.contains(needle),
            "the Go reclose no longer induces a fingerprint mismatch (`{needle}` is \
             gone), so the init() check is only ever exercised on a matching pair \
             — which proves it does not false-positive, and nothing else:\n{body}"
        );
    }
    // `forgedb build` must NOT be re-run between the regenerate and the go
    // build, or the pair matches again and the guard is vacuous.
    let after = body
        .split_once("generate go --runtime --force")
        .expect("the induced mismatch is present")
        .1;
    assert!(
        !after.contains("\"$FORGEDB\" build"),
        "the reclose rebuilds the archive after inducing the mismatch, so the two \
         halves agree again and the init() check is never triggered:\n{after}"
    );
}

// ---------------------------------------------------------------------------
// S5 — the tier-2 command stays workspace-level.
// ---------------------------------------------------------------------------

/// The specific regression #390's design was built around.
///
/// A "run all the ignored tests" target is most naturally written as a loop over the
/// per-scenario targets that already exist. That form covers 15 of the 29 ignored tests
/// and looks complete: it silently drops every one in `build_cache_compile_test`,
/// `in_tree_package_test`, `prompt_boundary_test`, `pyo3_component_compile_test`,
/// `placement_flip_test`, `migrate_tests` and `core_utoipa_gate_test`, because those
/// seven files have no target at all.
///
/// The count in that sentence is prose and drifts; the ASSERTIONS below derive their set
/// from source, which is why a new ignored test cannot fall outside them.
///
/// `--workspace` needs no list, and so cannot have a stale one. This asserts the
/// invocation has not been narrowed back down — by target, by package, or by both.
#[test]
fn tier_two_runs_the_whole_workspace_not_a_list_of_binaries() {
    let recipe = make_recipe("test-ignored");

    assert!(
        recipe.contains("--workspace"),
        "`make test-ignored` must be workspace-level; got: {recipe}"
    );
    assert!(
        recipe.contains("--ignored"),
        "`make test-ignored` must pass --ignored; got: {recipe}"
    );
    assert!(
        !recipe.contains("--test "),
        "`make test-ignored` restricts to specific test binaries (`--test`). That form \
         covers only the ignored tests whose file happens to have a per-scenario target, \
         and looks complete — the files with no target are exactly the ones it drops. \
         Got: {recipe}"
    );
    for narrowing in ["--package ", "-p ", "--lib", "--bins"] {
        assert!(
            !recipe.contains(narrowing),
            "`make test-ignored` narrows the run with `{narrowing}`, which silently \
             reduces coverage while still reporting success. Got: {recipe}"
        );
    }
}

// ---------------------------------------------------------------------------
// S2 — the skip pattern still matches exactly the test it names.
// ---------------------------------------------------------------------------

/// `--skip` fails silently in BOTH directions, which is why this is a test and not a
/// comment.
///
/// Rename the migrate test and the pattern matches *nothing*: migrate_tests quietly
/// rejoins the nightly, where it compiles against the PUBLISHED substrate and so goes
/// red for most of every cycle — for a completely correct reason, which is worse, because
/// the job is then permanently red and stops being read.
///
/// Loosen the pattern and it matches more than one test: those disappear from tier 2 and
/// the nightly still reports green.
#[test]
fn the_tier_two_skip_matches_exactly_one_real_test() {
    let recipe = make_recipe("test-ignored");
    let patterns = skip_patterns(&recipe);

    assert_eq!(
        patterns.len(),
        1,
        "expected exactly one --skip in `make test-ignored`; every additional one removes \
         a test from the only job that runs it. Got: {patterns:?}"
    );

    let pattern = &patterns[0];
    let ignored = ignored_tests();
    let matched: Vec<_> = ignored.iter().filter(|t| t.contains(pattern)).collect();

    assert_eq!(
        matched.len(),
        1,
        "`--skip {pattern}` matches {} of the {} ignored tests, not exactly 1: {matched:?}. \
         Zero means the test was renamed and migrate_tests has silently rejoined the \
         nightly; more than one means tests are being dropped from tier 2 while it still \
         reports green.",
        matched.len(),
        ignored.len(),
    );

    assert!(
        matched[0].contains("migrate"),
        "the skipped test should be the migrate one (it builds against the PUBLISHED \
         substrate, so it belongs on the `main` surface); got {}",
        matched[0]
    );
}

// ---------------------------------------------------------------------------
// S3 — excluding migrate_tests is a move, not a deletion.
// ---------------------------------------------------------------------------

/// Skipping a test in the only job that runs it is a deletion unless something else runs
/// it. `migrate_tests` leaves the nightly precisely because it measures this repo's
/// relationship to crates.io rather than the commit under test — which is what
/// `substrate-reclose` already measures, on `main`. If that job stops running it, the
/// skip above becomes silent dead coverage.
#[test]
fn migrate_tests_has_a_home_on_the_main_surface() {
    // Scoped to the one step's command. Asserting against the whole file let the COMMENT
    // above that step satisfy the `--ignored` check — the mutation harness caught it
    // surviving. See `without_comments`.
    let cmd = run_command("substrate-reclose.yml", "Run migrate_tests");

    assert!(
        cmd.contains("--test migrate_tests"),
        "migrate_tests is skipped by `make test-ignored`, so substrate-reclose.yml must \
         run it — otherwise nothing does and the skip is a deletion. Got: {cmd}"
    );
    assert!(
        cmd.contains("--ignored"),
        "substrate-reclose.yml runs migrate_tests WITHOUT --ignored, so libtest filters \
         the test out and the step passes having run nothing at all. Got: {cmd}"
    );
}

// ---------------------------------------------------------------------------
// S4 — tier 1 builds the examples.
// ---------------------------------------------------------------------------

/// The examples build is the assertion a reviewer skims past, and it has silently broken
/// the tree twice. `--lib`, `--bins`, `--tests` AND `--doc` all exclude examples, so
/// nothing in the test command compiles them: dropping this line costs no test results
/// and produces no warning.
#[test]
fn tier_one_runs_the_suite_and_builds_the_examples() {
    let recipe = make_recipe("test");

    assert!(
        recipe.contains("cargo test --workspace --no-fail-fast"),
        "`make test` must run the workspace suite with --no-fail-fast — without it cargo \
         halts at the first failing binary and hides every result behind it, so one break \
         reports as one failure when there may be twenty. Got: {recipe}"
    );
    assert!(
        recipe.contains("cargo build --workspace --examples"),
        "`make test` must build the examples. No test flag covers them — `--lib`, \
         `--bins`, `--tests` and `--doc` all exclude examples — so dropping this line \
         costs no test results and produces no warning. It has silently broken the tree \
         twice. Got: {recipe}"
    );

    // Same single-definition rule as the nightly: the workflow calls the target rather
    // than repeating it, so the two assertions above describe what CI actually runs.
    let wf = unwrap_continuations(&workflow("test.yml"));
    assert!(
        wf.contains("make test"),
        "test.yml must run `make test`. Inlining the commands creates a second definition \
         of tier 1, and the assertions above would then describe the Makefile while CI ran \
         something else"
    );
    assert!(
        !wf.contains("cargo test --workspace"),
        "test.yml has its own copy of the tier-1 command alongside the make target; the \
         two will drift"
    );
}

// ---------------------------------------------------------------------------
// S7 — the nightly and the aggregate cannot drift apart.
// ---------------------------------------------------------------------------

/// Two places that must agree will stop agreeing. The nightly invokes the `make` target
/// rather than repeating its command, so `test-ignored` is the single definition and
/// every assertion in this file applies to what CI actually runs.
#[test]
fn the_nightly_invokes_the_aggregate_target_rather_than_its_own_copy() {
    // Scoped to the suite step's command, not the whole workflow. Matching file-wide let
    // the "reproduce locally: `make test-ignored`" hint inside the auto-filed issue BODY
    // satisfy this — the mutation harness caught it surviving the run: line being gutted
    // to `echo`. Third instance of the same mistake in this file; hence `run_command`.
    let cmd = run_command("nightly-ignored.yml", "Tier 2 — the ignored suite");

    assert!(
        cmd.contains("make test-ignored"),
        "nightly-ignored.yml's suite step must run `make test-ignored`. Inlining the cargo \
         command creates a second definition, and every guard in this file would then be \
         asserting properties of the Makefile while CI ran something else. Got: {cmd}"
    );
    assert!(
        !cmd.contains("cargo test"),
        "nightly-ignored.yml's suite step has its own copy of the tier-2 command rather \
         than calling the target; the two will drift. Got: {cmd}"
    );
}

// ---------------------------------------------------------------------------
// S6 — a failing nightly can actually report.
// ---------------------------------------------------------------------------

/// A scheduled job that fails into an inbox nobody opens is the same non-control #390
/// replaces. The reporting step needs two things that are easy to lose independently:
/// the `issues: write` permission (without it the step fails with a 403 *after* the
/// suite has already failed, so the real failure is buried) and a `failure()` condition
/// (without it the step never runs, because a failed job skips subsequent steps).
#[test]
fn a_failing_nightly_has_the_permission_and_the_condition_to_report() {
    let nightly = workflow("nightly-ignored.yml");
    let flat = unwrap_continuations(&nightly);

    assert!(
        flat.contains("issues: write"),
        "nightly-ignored.yml must declare `issues: write`, or the reporting step 403s \
         after the suite has already failed and the real failure is buried under it"
    );
    assert!(
        flat.contains("failure()"),
        "nightly-ignored.yml has no `failure()`-conditioned step: by default a step after \
         a failed one is SKIPPED, so the failure would report nothing at all"
    );
    // Scoped to the `gh issue create` invocation. Matching anywhere in the file let the
    // `gh issue list --label bugfix` LOOKUP satisfy this — the mutation harness caught it
    // surviving the removal of the label from the thing that actually files the issue.
    let create = flat
        .split("gh issue create")
        .nth(1)
        .expect("nightly-ignored.yml must file an issue on failure (`gh issue create`)");
    let create = create.split(" fi").next().unwrap_or(create);

    assert!(
        create.contains("--label bugfix"),
        "the CREATED issue must carry the `bugfix` type label. pm-playbook PM010 requires \
         exactly one type label per work item, so an unlabelled auto-filed issue turns \
         `pm-playbook check` red — a new control whose side effect is breaking an existing \
         one. Got: {create}"
    );
}

// ---------------------------------------------------------------------------
// The nightly must run against `develop`, not against its own default branch.
// ---------------------------------------------------------------------------

/// `schedule:` only fires for workflow files on the DEFAULT branch, which here is `main`.
/// So this workflow lives on `main` and has to reach over to the branch it is meant to
/// watch. Without an explicit ref it would silently test `main` — a branch that moves
/// once a cycle — and find rot at the release instead of the night it appeared.
#[test]
fn the_nightly_checks_out_develop_explicitly() {
    let flat = unwrap_continuations(&workflow("nightly-ignored.yml"));

    assert!(
        flat.contains("ref: develop"),
        "nightly-ignored.yml must check out `develop` explicitly. It runs from `main` \
         (schedule: only fires on the default branch), so without a ref it tests `main` \
         and reports green all cycle while `develop` rots"
    );
}

// ---------------------------------------------------------------------------
// Housekeeping: the guards above are keyed to files that must exist.
// ---------------------------------------------------------------------------

/// Each assertion above reads a workflow file. If one were deleted, `read` would panic
/// with a filesystem error rather than a diagnosis — this names the actual problem.
#[test]
fn the_workflows_this_file_guards_still_exist() {
    for f in ["test.yml", "nightly-ignored.yml", "substrate-reclose.yml"] {
        let p = repo_root().join(".github/workflows").join(f);
        assert!(
            Path::new(&p).exists(),
            "{f} is missing. #390 added it as the control that runs the test suite; \
             deleting it removes the gate and nothing else will report that."
        );
    }
}

// ---------------------------------------------------------------------------
// S339-7 — every registry-resolving job is `main`-only.
// ---------------------------------------------------------------------------

/// #402's lesson, one issue old, turned into a test that runs on every branch.
///
/// A job that scaffolds outside the checkout resolves every `forgedb-*` from
/// crates.io. `develop` is *allowed* to carry a publish gap — that is the entire
/// point of holding the gap off the default branch — so such a job run there
/// fails BY DESIGN for most of every cycle. #402 measured the cost of getting it
/// wrong: nine runs, nine failures, six branches, and nobody read it, because
/// everyone who saw it red saw it red on a branch that had not caused it.
///
/// A permanently-red job is not a control. This guard runs in tier 1, on every
/// branch, so a re-widened trigger is caught a cycle before a `main`-only job
/// could report it — which is the one thing a `main`-only job cannot do for
/// itself.
#[test]
fn every_registry_resolving_job_runs_on_main_only() {
    for file in ["substrate-reclose.yml", "go-reclose.yml"] {
        let on = trigger_block(file);

        for event in ["push:", "pull_request:"] {
            assert!(
                on.contains(event),
                "{file}'s `on:` block no longer declares `{event}`. Got:\n{on}"
            );
        }

        // Anchored on the FILTER, never on the event name: a `push:` with no
        // `branches:` under it runs on every branch, and reads identically.
        let filters: Vec<&str> = on
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("branches:"))
            .collect();
        assert_eq!(
            filters.len(),
            2,
            "{file} must filter BOTH `push` and `pull_request` to a branch list; \
             found {} `branches:` line(s). An event with no filter runs on every \
             branch, including `develop`, where this job is red by design. Got:\n{on}",
            filters.len(),
        );
        for f in &filters {
            assert_eq!(
                *f, "branches: [main]",
                "{file} restricts a trigger to `{f}` rather than `branches: [main]`. \
                 Any other branch here puts a registry-resolving job on a surface \
                 that carries the publish gap. Got:\n{on}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// S339-11 — `run_block` cannot pass vacuously.
// ---------------------------------------------------------------------------

/// The helper every guard below leans on, tested directly.
///
/// `run_command` reads to the end of the `run:` line, so on a block scalar it
/// returns the literal `run: |` — one token, containing none of the script. Every
/// `!contains` assertion built on that passes having examined nothing. This is
/// the guard on the guard: if `run_block` ever degrades to that shape, the
/// failure lands here rather than as six silently-vacuous assertions.
#[test]
fn run_block_returns_the_whole_script_not_the_scalar_header() {
    // A multi-line step: both the first command and the LAST line must be there.
    // Truncating to the first line is the plausible regression, and it would
    // leave every guard passing on a prefix.
    let ffi = run_block(
        "substrate-reclose.yml",
        "2/6 ffi — generate ffi, then forgedb build",
    );
    assert!(
        ffi.lines().filter(|l| !l.trim().is_empty()).count() > 1,
        "run_block returned a single line for a block-scalar step — it has \
         degraded to `run_command`'s behaviour and every guard keyed on it is now \
         vacuous. Got:\n{ffi}"
    );
    assert!(
        ffi.contains("set -euxo pipefail"),
        "run_block dropped the first line of the script:\n{ffi}"
    );
    assert!(
        ffi.contains("./csmoke"),
        "run_block dropped the LAST line of the script — a guard keyed on a \
         trailing command would pass vacuously:\n{ffi}"
    );

    // Comments are stripped, so a needle's own rationale cannot satisfy it.
    assert!(
        !ffi.contains("#337: the delivered half"),
        "run_block no longer strips comments; a guard can now be satisfied by the \
         prose explaining it:\n{ffi}"
    );
}

/// A step name that does not exist must PANIC naming the step, never degrade to
/// a wider slice.
///
/// `find(..).unwrap_or(0)` widens to the whole file: the assertion stays live,
/// aimed at the wrong subject, and gets *easier* to satisfy as it becomes
/// meaningless. That is the failure mode this repo has already paid for.
#[test]
#[should_panic(expected = "no step named \"a step that does not exist\"")]
fn run_block_panics_on_a_missing_step_rather_than_widening() {
    let _ = run_block("substrate-reclose.yml", "a step that does not exist");
}

/// A step whose `run:` is SINGLE-LINE must panic too, rather than silently
/// returning the next step's script.
///
/// `Build the forgedb CLI` is single-line and is followed by a block-scalar
/// step, so an unbounded search for `run: |` finds the neighbour's body and
/// returns it — a guard keyed to the first step would then be asserting
/// properties of the second, live and wrong.
#[test]
#[should_panic(expected = "has no block-scalar `run: |`")]
fn run_block_refuses_a_single_line_step_rather_than_taking_its_neighbours() {
    let _ = run_block("substrate-reclose.yml", "Build the forgedb CLI");
}

// ---------------------------------------------------------------------------
// S339-8 — every reclose job sets FORGEDB_HOME outside the checkout, and says so
//          in an assertion rather than a comment.
// ---------------------------------------------------------------------------

/// The build cache is where every generated package compiles now, so "outside
/// the checkout" attaches to the CACHE, not to the app directory.
///
/// The repo root `Cargo.toml` has `members` and no `exclude`, so a cache under
/// `${{ github.workspace }}` is swallowed by this repo's own workspace: the
/// generated packages inherit the 1.96 pin and resolve `forgedb-*` BY PATH. The
/// reclose then measures the checkout instead of the registry — silently, and
/// while passing, which is the one failure mode it cannot survive.
///
/// Both halves are required. The export alone is a value someone can change; the
/// refusal is what makes a wrong value loud.
#[test]
fn every_reclose_job_sets_forgedb_home_outside_the_checkout() {
    for (file, step) in [
        (
            "substrate-reclose.yml",
            "Scaffold a project outside the checkout",
        ),
        ("go-reclose.yml", "Reclose — generate, build, link, run"),
    ] {
        let body = run_block(file, step);
        assert!(
            body.contains("export FORGEDB_HOME="),
            "{file} / {step:?} no longer exports FORGEDB_HOME, so the cache lands \
             in `~/.forgedb` — warm across runs, and a warm cache resolves nothing \
             from the registry:\n{body}"
        );
        assert!(
            body.contains("case \"$FORGEDB_HOME\" in") && body.contains("${GITHUB_WORKSPACE}"),
            "{file} / {step:?} dropped the refusal that FORGEDB_HOME is not inside \
             the checkout. Without it a cache under the workspace resolves \
             `forgedb-*` by path and this job measures the checkout while \
             passing:\n{body}"
        );
    }
}

/// The cold-cache pair, in the job that owns it.
///
/// Anchored on the `test ! -e` invocations, never on the comment above them:
/// this file's header explains at length why the cache must be cold, in the same
/// words a file-wide search would find. `run_block` strips comments for exactly
/// this reason.
#[test]
fn the_bare_reclose_proves_its_cache_was_cold() {
    let scaffold = run_block(
        "substrate-reclose.yml",
        "Scaffold a project outside the checkout",
    );
    for needle in [
        "test ! -e \"$HOME/.forgedb\"",
        "test ! -e \"$FORGEDB_HOME\"",
        "test ! -e \"$FORGEDB_HOME/projects\"",
    ] {
        assert!(
            scaffold.contains(needle),
            "the reclose no longer asserts `{needle}` before generating. A warm \
             cache masks a publish gap outright — the substrate is already \
             resolved, so a version that no longer exists on crates.io is never \
             looked up and the job goes green on an uninstallable \
             branch:\n{scaffold}"
        );
    }

    // …and at the far end, where a leak into the default home would otherwise
    // only be discovered by the NEXT run finding it warm.
    let last = run_block(
        "substrate-reclose.yml",
        "6/6 transform — migrate build over a real lineage",
    );
    assert!(
        last.contains("test ! -e \"$HOME/.forgedb\""),
        "the reclose's last arm no longer asserts the default cache home is still \
         absent. Something writing there makes the next run warm, and the check \
         then measures its own leftovers:\n{last}"
    );
}

// ---------------------------------------------------------------------------
// S339-9 — no cache action may name the ForgeDB home.
// ---------------------------------------------------------------------------

/// The single edit that makes this job fast AND makes it prove nothing.
///
/// Caching `~/.forgedb` (or `$FORGEDB_HOME`, or handing it to rust-cache as a
/// `cache-directories` entry) restores a resolved lockfile and a compiled
/// substrate from a previous run. Every registry lookup this job exists to
/// perform then does not happen, and the publish gap — the ONE thing it detects
/// — becomes invisible while the job reports green and finishes in a third of
/// the time. It is attractive precisely because it is the obvious optimization.
#[test]
fn no_cache_action_names_the_forgedb_home() {
    for file in ["substrate-reclose.yml", "go-reclose.yml"] {
        let wf = workflow(file);

        assert!(
            !wf.contains("uses: actions/cache"),
            "{file} uses `actions/cache`. This job's entire value is that it \
             resolves from crates.io on every run; a restored cache is how it \
             goes green while the branch is uninstallable."
        );

        // rust-cache is allowed — it caches the CHECKOUT's target dir, which this
        // job builds the CLI in. What it must never be handed is the ForgeDB home.
        for (idx, _) in wf.match_indices("uses: Swatinem/rust-cache") {
            // Scoped to that step's own `with:` block: the next `- ` at the step's
            // depth ends it. Matching file-wide would let an unrelated mention of
            // `.forgedb` anywhere satisfy — or break — this.
            let tail = &wf[idx..];
            let step = tail
                .find("\n      - ")
                .map(|e| &tail[..e])
                .unwrap_or(tail);
            for needle in [".forgedb", "FORGEDB_HOME", "forgedb-home", "cache-directories"] {
                assert!(
                    !step.contains(needle),
                    "{file} hands `{needle}` to rust-cache. Restoring the ForgeDB \
                     home makes the substrate resolve from a previous run rather \
                     than from the registry, and the publish gap this job exists \
                     to detect becomes invisible:\n{step}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// S339-6 — exactly one manifest reaches the consumer's tree.
// ---------------------------------------------------------------------------

/// An EQUALITY, not a superset — and that distinction is the whole guard.
///
/// A superset assertion passes through the exact change it exists to catch: an
/// `[placement].rust_package` default flipping on adds a second manifest and
/// reads as fine, and so does any new emitter that starts writing a cargo
/// package into a tree ForgeDB does not own.
///
/// The plan for #339 originally pinned the placement MODE in the app's config
/// instead. That is unimplementable — #338's surface is one optional key whose
/// absence is the opt-out, and there is no affirmative "cache-only" spelling to
/// write. Asserting the OUTCOME needs no spelling, catches strictly more, and
/// fails with the diff.
#[test]
fn the_bare_job_asserts_an_exact_manifest_set() {
    let body = run_block("substrate-reclose.yml", "0/6 generate all — emit every cache package");

    assert!(
        body.contains("find \"$APP\" -name Cargo.toml"),
        "the reclose no longer enumerates the manifests under the app, so a \
         second cargo package appearing in a user's tree would be \
         invisible:\n{body}"
    );
    assert!(
        body.contains("diff -u"),
        "the manifest check no longer COMPARES the two sets. A `grep`/`test -f` \
         form is a superset assertion, and a superset passes through the exact \
         change this exists to catch — a placement default flipping on, or a new \
         emitter nobody told CI about:\n{body}"
    );
    assert!(
        body.contains("generated/rust-sdk/Cargo.toml"),
        "the expected set no longer names the one manifest ForgeDB legitimately \
         writes into the app (the class-A REST client crate). An empty expected \
         set would fail always; a missing one would compare against \
         nothing:\n{body}"
    );
}

// ---------------------------------------------------------------------------
// S339-1/2/10 — the parent-workspace job's steps still do their work.
// ---------------------------------------------------------------------------

/// The step NAMES are the stable interface every guard here keys on; the
/// assertions are keyed to the COMMAND inside each, never to the name and never
/// to the comment above it.
///
/// This job is green on day one by design — it is a regression and coverage
/// guard, not a bug-finder — which is exactly the condition under which a step
/// can be quietly gutted and nobody notices: it was passing before and it is
/// passing after.
#[test]
fn the_parent_workspace_job_still_does_its_work() {
    // The foreign root itself. Without a `[workspace]` table above the app this
    // job is a slower copy of the one beside it.
    let setup = run_block(
        "substrate-reclose.yml",
        "Scaffold a foreign workspace root, and an app beneath it",
    );
    for needle in ["[workspace]", "members = [\"consumer\"]", "sha256sum Cargo.toml"] {
        assert!(
            setup.contains(needle),
            "the parent-workspace job no longer builds a foreign cargo workspace \
             root (`{needle}` is gone). Without one it tests the same bare \
             `mktemp -d` shape the job beside it already covers, and #330 case A \
             is invisible again:\n{setup}"
        );
    }

    // P1 — the cache resolves ITSELF, under a foreign root.
    let p1 = run_block("substrate-reclose.yml", "P1 foreign root — the cache is immune");
    for needle in [
        "cargo locate-project --workspace",
        "--manifest-path $ROOT/Cargo.toml",
        "test -f \"$ROOT/Cargo.lock\"",
        "test -d \"$ROOT/target\"",
    ] {
        assert!(
            p1.contains(needle),
            "P1 no longer proves the cache is immune to the foreign root \
             (`{needle}` is gone). That immunity is the epic's central claim — it \
             is why the cache directory makes #328 mostly dissolve — and this is \
             the only place it is checked against a real nested \
             workspace:\n{p1}"
        );
    }

    // P2 — the checksum comparison, and the `members` line by name.
    let p2 = run_block(
        "substrate-reclose.yml",
        "P2 foreign root — ForgeDB writes nothing it does not own",
    );
    assert!(
        p2.contains("sha256sum Cargo.toml") && p2.contains("$ROOT_SHA"),
        "P2 no longer COMPARES the foreign root's checksum against the one taken \
         before `init`. A `test -f` or a `grep` here would pass on a file ForgeDB \
         had rewritten:\n{p2}"
    );
    // Matched on the grep INVOCATION rather than on a literal spelling of the
    // pattern: the pattern is shell-escaped in the workflow and a switch to
    // `grep -F` would legitimately unescape it, which is not a regression.
    assert!(
        p2.lines().any(|l| {
            l.starts_with("grep ") && l.contains("members") && l.contains("consumer")
        }),
        "P2 no longer greps the foreign root for its `members` array. It is the \
         specific edit #338 refuses to make, and a checksum failure alone does not \
         say which line moved:\n{p2}"
    );
}

// ---------------------------------------------------------------------------
// S339-3 — the SDK arm BUILDS the crate; it does not grep it.
// ---------------------------------------------------------------------------

/// The generated Rust SDK is covered by string snapshots and by a
/// `syn::parse_file` at generation time. Parsing is not compiling — the emitted
/// crate names `reqwest`/`serde` types it has never been type-checked against —
/// and this step is the only place anything compiles it.
///
/// The plausible regression is not deletion but softening: a `test -f` on the
/// manifest, or a `grep` for a symbol, which look like coverage and are not. So
/// the assertions anchor on the compile and on the membership query, and
/// explicitly on `cargo build` running from the workspace ROOT rather than
/// inside the SDK directory — building in place is the shape #430 withdrew and
/// it has no green state under a foreign root.
#[test]
fn the_sdk_arm_builds_rather_than_greps() {
    let body = run_block(
        "substrate-reclose.yml",
        "P3 class-A output — the generated Rust SDK, adopted and built",
    );

    for needle in [
        // Adoption: one path dep from a crate the consumer owns.
        "forgedb-client = { path = \"../app/generated/rust-sdk\" }",
        // The compile, from the ROOT.
        "cargo build -p consumer",
        // Membership, asked of cargo rather than asserted about the filesystem.
        "cargo metadata --no-deps",
        // The client is CONSTRUCTED — a path dep alone exercises no public surface.
        "ForgeDbClient::new(",
        // The foreign root is still untouched after adoption.
        "$ROOT_SHA",
    ] {
        assert!(
            body.contains(needle),
            "the SDK arm no longer runs `{needle}`. Without it the step reads as \
             coverage of a crate nothing compiles — `RustSdkGenerator` has only \
             string snapshots and a parse check behind it:\n{body}"
        );
    }

    // Building INSIDE the SDK directory is the withdrawn shape (#430): it has no
    // green state under a foreign root, and a step that reintroduced it would be
    // red forever for a reason that is not ForgeDB's.
    assert!(
        !body.contains("cd \"$APP/generated/rust-sdk\""),
        "the SDK arm builds in place rather than by adoption. A package under a \
         foreign workspace root that is not a member cannot build, and the fix \
         that would make it — a `[workspace]` table in the generated package — is \
         withdrawn (#430), because a nested one that any member path-depends on \
         fails the entire workspace:\n{body}"
    );
}

// ---------------------------------------------------------------------------
// S339-4 — the in-tree arm pastes a line it EXTRACTED.
// ---------------------------------------------------------------------------

/// In-tree placement is the first surface where ForgeDB's substrate pins land in
/// the *user's own* build graph. #338's tier-2 coverage builds the consumer
/// workspace through `[patch.crates-io]` pointing at the checkout — which proves
/// the emitted package compiles and says nothing about registry resolution, a
/// patch table being precisely the thing that makes a registry lookup not happen.
///
/// Two properties, and the second is the one that fails quietly. The line the
/// consumer pastes must come out of the CLI's own output — the package name is
/// derived, so a literal spelling here is wrong at the first rename — and the
/// extraction must be asserted non-empty before use, because an empty one makes
/// the append a no-op and `cargo build` then passes having compiled nothing new.
/// That is not hypothetical: the first draft of the step extracted nothing,
/// because the CLI prefixes the line with an info glyph.
#[test]
fn the_in_tree_arm_pastes_a_line_it_extracted() {
    let body = run_block(
        "substrate-reclose.yml",
        "P4 in-tree — the consumer's own build graph resolves the substrate",
    );

    assert!(
        body.contains("rust_package"),
        "P4 no longer sets `[placement].rust_package`, so no in-tree package is \
         emitted and the step measures the cache again:\n{body}"
    );
    // Scoped to the ASSIGNMENT, not to the step. A file-wide `contains` here
    // survived a mutation that replaced the extraction with a hardcoded line:
    // `intree.log` still appeared, on the `tee` a few lines above. Both bounds
    // panic on a miss rather than widening to the rest of the step.
    let assign = {
        let after = body.split_once("DEP_LINE=").unwrap_or_else(|| {
            panic!("P4 no longer assigns DEP_LINE at all:\n{body}")
        }).1;
        after
            .split_once("\ntest -n")
            .unwrap_or_else(|| {
                panic!("P4's DEP_LINE assignment is no longer followed by its non-empty check:\n{body}")
            })
            .0
    };
    assert!(
        assign.contains("intree.log"),
        "P4's DEP_LINE is not derived from the CLI's output — the assignment reads \
         `{assign}`. The package name is derived (`<app>-core`), so a literal \
         spelling here is wrong at the first rename, and the `package =` key is not \
         optional: cargo matches a path dep's KEY against the package's own name."
    );
    assert!(
        body.contains("test -n \"$DEP_LINE\""),
        "P4 pastes the extracted line without asserting it is non-empty. An empty \
         extraction makes the append a no-op; `cargo build` then succeeds having \
         compiled nothing new, and the step reports green having proved \
         nothing:\n{body}"
    );
    assert!(
        body.contains("cargo build -p consumer"),
        "P4 no longer builds the consumer. Emitting the package proves it was \
         written, not that a user can compile it:\n{body}"
    );
    assert!(
        body.contains("$WORK/Cargo.lock") && body.contains("crates.io-index"),
        "P4 no longer inspects the CONSUMER's lockfile for the registry source. \
         That resolution is the entire property — the package compiling is \
         already covered by #338's own tier-2 tests, through a `[patch.crates-io]` \
         that makes the registry lookup not happen:\n{body}"
    );

    // The one thing that would make the whole step vacuous.
    assert!(
        !body.contains("patch.crates-io"),
        "P4 introduces a `[patch.crates-io]`. A patch table is exactly what makes \
         a registry lookup not happen, so the step would prove only what #338's \
         tier-2 tests already prove:\n{body}"
    );
}

// ---------------------------------------------------------------------------
// S339 — no reclose passes a flag the CLI has tombstoned.
// ---------------------------------------------------------------------------

/// The failure this exists to prevent, found by running it (#339's own dispatch).
///
/// #374 removed `migrate create --auto` and left the reclose's `6/6` arm calling
/// it. Nothing reported that, because the reclose runs on `main` and the removal
/// landed on `develop` — so the break sat there until someone dispatched the
/// workflow by hand, and its first real execution would otherwise have been the
/// release merge, where it is a release blocker.
///
/// A tombstoned flag is exactly the shape that rots this way: `refuse_removed_flag`
/// makes the CLI reject it with a good message, which means the workflow fails
/// LOUDLY — but only when it runs, once a cycle, on a branch nobody is looking at.
///
/// The flag set is DERIVED from the tombstone call sites, not written out again.
/// A second list here would drift from the first, which is the failure this
/// repo's guards exist to refuse.
#[test]
fn no_reclose_workflow_passes_a_tombstoned_cli_flag() {
    let src = read("src/commands/migrate/mod.rs");
    let re = Regex::new(r#"refuse_removed_flag\(\s*"(--[a-z0-9-]+)""#).unwrap();
    let tombstoned: BTreeSet<String> = re
        .captures_iter(&src)
        .map(|c| c[1].to_string())
        .collect();

    assert!(
        !tombstoned.is_empty(),
        "found no `refuse_removed_flag` call sites — the parser has drifted from \
         src/commands/migrate/mod.rs and this guard would now pass vacuously"
    );

    for file in ["substrate-reclose.yml", "go-reclose.yml"] {
        let wf = workflow(file);
        for line in wf.lines().map(str::trim) {
            if !line.contains("$FORGEDB") {
                continue;
            }
            for flag in &tombstoned {
                assert!(
                    !line.split_whitespace().any(|w| w == flag),
                    "{file} invokes the CLI with `{flag}`, which \
                     `refuse_removed_flag` rejects — so this step fails at run time, \
                     and it only runs on `main`, once a cycle. That is exactly how \
                     #374's removal sat broken in this workflow until #339 \
                     dispatched it by hand.\nOffending line: {line}"
                );
            }
        }
    }
}

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

fn unwrap_continuations(s: &str) -> String {
    let joined = s.replace("\\\n", " ");
    Regex::new(r"\s+").unwrap().replace_all(&joined, " ").trim().to_string()
}

fn make_recipe(target: &str) -> String {
    let mk = read("Makefile");
    let head = format!("\n{target}:\n");
    let start = mk
        .find(&head)
        .unwrap_or_else(|| panic!("Makefile has no `{target}:` target"))
        + head.len();
    let rest = &mk[start..];
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

fn skip_patterns(cmd: &str) -> Vec<String> {
    let re = Regex::new(r"--skip\s+(\S+)").unwrap();
    re.captures_iter(cmd).map(|c| c[1].to_string()).collect()
}

fn workflow(name: &str) -> String {
    let raw = read(&format!(".github/workflows/{name}"));
    raw.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

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
    let tail = &rest[run..];
    let end = tail.find('\n').unwrap_or(tail.len());
    unwrap_continuations(&tail[..end])
}

fn run_block(workflow_file: &str, step_name: &str) -> String {
    let src = read(&format!(".github/workflows/{workflow_file}"));
    let needle = format!("- name: {step_name}");
    let start = src.find(&needle).unwrap_or_else(|| {
        panic!("{workflow_file} has no step named {step_name:?} — the guard keyed to it is now vacuous")
    });
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

// REGRESSION(#486): this test used to assert the defect it was hiding.
// It required the literal `fsync = "always"` — the scaffold's own default — so the
// step it pinned changed no generated byte and induced nothing. The step ALSO
// appended a second `[storage]` table to a config that already had one, so
// `forgedb generate` died on a TOML duplicate-key error at exit 10 before any
// assertion ran. Green for two weeks while the guard was inert.
// The assertions below therefore anchor on PROPERTIES of the induction — the edit
// is in place, the value is DERIVED from the template rather than restated, and the
// fingerprint is proven to have moved — never on the knob's spelling. A `contains`
// on the spelling is satisfied by a step that induces nothing.
#[test]
fn s337_the_go_reclose_proves_the_init_check_executes() {
    let body = run_block("go-reclose.yml", "Reclose — generate, build, link, run");

    for needle in [
        "generate go --runtime --force",
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

    assert!(
        !body.contains("cat >> forgedb.toml"),
        "the reclose appends to forgedb.toml instead of editing it in place. The \
         scaffold already writes a `[storage]` table, so a second header is invalid \
         TOML: `forgedb generate` exits 10 and every assertion below it is \
         unreachable (#486):\n{body}"
    );

    let scaffold = read("src/templates.rs");
    let defaults: Vec<&str> = scaffold
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("# fsync = \"")
                .and_then(|r| r.strip_suffix('"'))
        })
        .collect();
    assert_eq!(
        defaults.len(),
        1,
        "expected exactly one `# fsync = \"...\"` default in src/templates.rs, found \
         {}: {defaults:?}. With none this cross-check passes vacuously; with several \
         it is comparing against an arbitrary one",
        defaults.len()
    );
    let written: Vec<&str> = body
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("grep -q '^fsync = \"")
                .and_then(|r| r.split('"').next())
        })
        .collect();
    assert_eq!(
        written.len(),
        1,
        "the reclose must verify exactly what it wrote to [storage].fsync — found {} \
         such assertions: {written:?}. Without one this test cannot tell which value \
         the step set, and (b) becomes unenforceable:\n{body}",
        written.len()
    );
    assert_ne!(
        written[0], defaults[0],
        "the reclose sets [storage].fsync to `{}`, which src/templates.rs already \
         scaffolds as the default. The regenerate then emits byte-identical source, \
         the archive is not stale, the smoke binary runs, and the step fails saying \
         init() did not execute — red for the wrong reason (#486):\n{body}",
        written[0]
    );

    assert!(
        body.contains(r#"if [ "$before_fp" = "$after_fp" ]"#),
        "the reclose no longer compares the generated fingerprint across the \
         regenerate, so it cannot distinguish the guard firing from there having \
         been nothing to catch — the two outcomes this step exists to tell apart \
         (#486):\n{body}"
    );

    let before = body.find("before_fp=").expect(
        "the reclose must capture the fingerprint BEFORE the regenerate; with no \
         baseline the comparison has nothing to compare against",
    );
    let regen = body
        .find("generate go --runtime --force")
        .expect("the induced mismatch is present");
    let after = body
        .find("after_fp=")
        .expect("the reclose must re-read the fingerprint after the regenerate");
    assert!(
        before < regen && regen < after,
        "the reclose reads the fingerprint in the wrong order (before={before}, \
         regenerate={regen}, after={after}); both reads must straddle the \
         regenerate:\n{body}"
    );

    let after_regen = body
        .split_once("generate go --runtime --force")
        .expect("the induced mismatch is present")
        .1;
    assert!(
        !after_regen.contains("\"$FORGEDB\" build"),
        "the reclose rebuilds the archive after inducing the mismatch, so the two \
         halves agree again and the init() check is never triggered:\n{after_regen}"
    );
}

#[test]
fn the_comment_rule_has_a_place_where_it_can_fail() {
    let recipe = make_recipe("comment-check");

    assert!(
        recipe.contains("scripts/strip-comments.ts"),
        "`make comment-check` must invoke the checker. A target that runs something else \
         reports green on a tree full of comments, which is worse than no target: the rule \
         then LOOKS enforced. Got: {recipe}"
    );
    assert!(
        recipe.contains("--check"),
        "`make comment-check` must pass --check. Without it the script defaults to `stats`, \
         which prints a count and EXITS ZERO — the report-is-not-a-gate shape (§5.5). \
         Got: {recipe}"
    );

    let cmd = run_command("test.yml", "No comments in ForgeDB's own source");
    assert!(
        cmd.contains("make comment-check"),
        "test.yml must run `make comment-check`, or the rule is enforced by memory. \
         Got: {cmd}"
    );

    let wf = unwrap_continuations(&workflow("test.yml"));
    assert!(
        wf.contains("setup-bun"),
        "the comment check runs under bun, so test.yml must install it. Without the setup \
         step the job fails on a missing interpreter rather than on a real finding, and \
         the usual repair is to delete the step"
    );
}

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

#[test]
fn migrate_tests_has_a_home_on_the_main_surface() {
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

#[test]
fn the_nightly_invokes_the_aggregate_target_rather_than_its_own_copy() {
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

#[test]
fn run_block_returns_the_whole_script_not_the_scalar_header() {
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

    assert!(
        !ffi.contains("#337: the delivered half"),
        "run_block no longer strips comments; a guard can now be satisfied by the \
         prose explaining it:\n{ffi}"
    );
}

#[test]
#[should_panic(expected = "no step named \"a step that does not exist\"")]
fn run_block_panics_on_a_missing_step_rather_than_widening() {
    let _ = run_block("substrate-reclose.yml", "a step that does not exist");
}

#[test]
#[should_panic(expected = "has no block-scalar `run: |`")]
fn run_block_refuses_a_single_line_step_rather_than_taking_its_neighbours() {
    let _ = run_block("substrate-reclose.yml", "Build the forgedb CLI");
}

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

        for (idx, _) in wf.match_indices("uses: Swatinem/rust-cache") {
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

#[test]
fn the_parent_workspace_job_still_does_its_work() {
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
    assert!(
        p2.lines().any(|l| {
            l.starts_with("grep ") && l.contains("members") && l.contains("consumer")
        }),
        "P2 no longer greps the foreign root for its `members` array. It is the \
         specific edit #338 refuses to make, and a checksum failure alone does not \
         say which line moved:\n{p2}"
    );
}

#[test]
fn the_sdk_arm_builds_rather_than_greps() {
    let body = run_block(
        "substrate-reclose.yml",
        "P3 class-A output — the generated Rust SDK, adopted and built",
    );

    for needle in [
        "forgedb-client = { path = \"../app/generated/rust-sdk\" }",
        "cargo build -p consumer",
        "cargo metadata --no-deps",
        "ForgeDbClient::new(",
        "$ROOT_SHA",
    ] {
        assert!(
            body.contains(needle),
            "the SDK arm no longer runs `{needle}`. Without it the step reads as \
             coverage of a crate nothing compiles — `RustSdkGenerator` has only \
             string snapshots and a parse check behind it:\n{body}"
        );
    }

    assert!(
        !body.contains("cd \"$APP/generated/rust-sdk\""),
        "the SDK arm builds in place rather than by adoption. A package under a \
         foreign workspace root that is not a member cannot build, and the fix \
         that would make it — a `[workspace]` table in the generated package — is \
         withdrawn (#430), because a nested one that any member path-depends on \
         fails the entire workspace:\n{body}"
    );
}

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

    assert!(
        !body.contains("patch.crates-io"),
        "P4 introduces a `[patch.crates-io]`. A patch table is exactly what makes \
         a registry lookup not happen, so the step would prove only what #338's \
         tier-2 tests already prove:\n{body}"
    );
}

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

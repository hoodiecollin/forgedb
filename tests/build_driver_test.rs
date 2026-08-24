//! **The pure half of the build driver (#335 step 6, plan #347 scenarios 24–28).**
//!
//! Everything here runs without spawning cargo, because everything here is a
//! *pure function*: [`forgedb::commands::build::driver::plan`] builds argument
//! vectors, [`forgedb::commands::build::driver::parse_artifacts`] reads a cargo
//! JSON stream, and `duplicate_artifact_names` reads a `cargo metadata`
//! document. The impure halves of the same scenarios live in
//! `tests/build_cache_compile_test.rs` and run the real thing.
//!
//! # Cargo is never mocked
//!
//! There is no fake cargo in this file and there must never be one. The defect
//! #335 fixes is a *misunderstanding of what cargo does* with the working
//! directory; a mock encodes the same misunderstanding and goes green. What is
//! here instead is (a) the arguments we would hand cargo, asserted verbatim, and
//! (b) cargo's own output, replayed.
//!
//! **Provenance of the streams below: they are RECORDINGS, not models.**
//! `tests/fixtures/cargo_stream_{native,wasm}.jsonl` are the verbatim stdout of
//! two real `cargo build --message-format=json-render-diagnostics` runs over the
//! dep-free workspace that `tests/fixtures/record_cargo_stream.sh` writes. A
//! hand-written stream is a *claim about what cargo emits*, and that claim is
//! exactly what `parse_artifacts` is trying to be right about — so a test built
//! on one proves only that the parser agrees with its author.
//!
//! The recording earned its keep immediately: three properties a careful author
//! would have got wrong are visible in it, and `tests/fixtures/README.md` names
//! them. The one that mattered here is that `json-render-diagnostics` puts **no
//! `compiler-message` on stdout at all**, and that a build script's
//! `compiler-artifact` carries `"executable": null` rather than a path.
//!
//! `build_cache_compile_test::scenario_27_a_real_cargo_stream_carries_three_distinguishable_kinds`
//! runs cargo live and re-asserts the same rules, so a future cargo that moves
//! the format fails *there* while these fixtures keep pinning today's rules.

use forgedb::commands::build::driver::{
    self, Artifact, BuildReport, Invocation, Profile, ReportedArtifact, Selected, TargetKind,
    WASM_TRIPLE,
};
use forgedb::naming::PackageKind;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const CACHE_ROOT: &str = "/home/u/.forgedb/projects/blog";
const APP: &str = "/home/u/.forgedb/projects/blog/apps/3f2a91c04d5e7b60";

fn sel(package: &str, kind: PackageKind) -> Selected {
    Selected {
        package: package.to_string(),
        kind,
    }
}

/// The whole plan, rendered as the shell lines `--plan` prints.
fn rendered(invocations: &[Invocation]) -> String {
    invocations
        .iter()
        .map(|i| i.command_line())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Purity — a structural guard, because purity is what makes this file possible
// ---------------------------------------------------------------------------

/// The driver's source, read at compile time so the guard below cannot drift
/// away from what it guards.
const DRIVER_SRC: &str = include_str!("../src/commands/build/driver.rs");

/// The body of a top-level `fn` in [`DRIVER_SRC`], from its signature line to
/// the first column-0 `}` after it.
///
/// Column-0 is the whole trick: it is the only closing brace that ends a
/// top-level item, so this needs no brace counting and cannot be fooled by the
/// braces inside doc comments and `format!` strings that fill this file.
fn body_of(signature: &str) -> &'static str {
    let start = DRIVER_SRC
        .find(signature)
        .unwrap_or_else(|| panic!("`{signature}` is gone from driver.rs"));
    let rest = &DRIVER_SRC[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("`{signature}` never closes at column 0"));
    &rest[..end]
}

/// **`plan` and `parse_artifacts` are pure, and stay pure.**
///
/// Not a style preference. Every scenario-24/26/27/28 assertion in this file
/// runs against paths that do not exist (`/home/u/.forgedb/…`,
/// `/tmp/forgedb-cargo-stream/…`) and spawns nothing — which is only possible
/// while these two functions decide everything from their arguments. The moment
/// one of them reads the disk or shells out, the cheap half of the driver's test
/// suite silently becomes an integration test that happens to pass on the
/// author's machine.
///
/// Anchored on the tokens that would *do* the work — `Command::new`, `fs::`,
/// `env::var` — never on a binding name.
#[test]
fn plan_and_parse_artifacts_touch_neither_the_disk_nor_a_process() {
    const IMPURE: &[&str] = &[
        "Command::new",
        "std::fs",
        "fs::read",
        "fs::write",
        "fs::metadata",
        "env::var",
        "env::current_dir",
        ".exists()",
        "File::open",
    ];
    for signature in [
        "pub fn plan(",
        "pub fn parse_artifacts(",
        "fn kind_from_filename(",
        "fn package_name_from_id(",
    ] {
        let body = body_of(signature);
        for token in IMPURE {
            assert!(
                !body.contains(token),
                "`{signature}` must stay pure, but its body contains `{token}`"
            );
        }
    }
}

/// The guard above is worthless if `body_of` silently returns nothing, so prove
/// it finds real code — and prove it stops at the right place by asserting it
/// does *not* swallow the impure function that follows `parse_artifacts`.
#[test]
fn the_purity_guard_reads_a_real_function_body() {
    let plan_body = body_of("pub fn plan(");
    assert!(
        plan_body.contains("--manifest-path"),
        "body_of read the wrong region:\n{plan_body}"
    );
    let parse_body = body_of("pub fn parse_artifacts(");
    assert!(parse_body.contains("compiler-artifact"), "{parse_body}");
    assert!(
        !parse_body.contains("duplicate_artifact_names"),
        "body_of ran past the end of parse_artifacts"
    );
    assert!(
        body_of("pub fn execute(").contains("Command::new"),
        "the impure control: `execute` DOES spawn cargo, so the token list the \
         purity guard uses must be capable of matching"
    );
}

// ---------------------------------------------------------------------------
// Scenario 24 — the profile floor is in the plan, as an ARGUMENT
// ---------------------------------------------------------------------------

/// **Scenario 24 (pure half).** *Given* a release build of any native package ·
/// *Then* the invocation carries `profile.release.panic="unwind"` as a
/// `--config` **argument**.
///
/// Cargo's `config.toml` beats the manifest, and `[profile.release.package.<p>]`
/// does not reach dependencies — so a hostile (or merely opinionated)
/// `$CARGO_HOME/config.toml` setting `panic = "abort"` breaks the `catch_unwind`
/// boundary in the generated `ffi`/`napi` wrappers, measured as `Abort trap: 6`
/// (exit 134) with `catch_unwind` never firing. A command-line `--config` beats
/// every config file.
///
/// **That it is an argument and not `CARGO_PROFILE_RELEASE_PANIC` is the
/// assertion, not an implementation detail.** An env var would work and would be
/// invisible in what `--plan` prints — the same invisible-mechanism shape the
/// hazard is made of. So `env` must stay empty and the floor must appear in the
/// rendered command line.
///
/// The proof that the floor actually *defeats* a planted config is
/// `build_cache_compile_test::scenario_24_*`, which plants one and runs cargo.
#[test]
fn scenario_24_the_release_plan_carries_a_visible_unwind_floor() {
    let plan = driver::plan(
        Path::new(CACHE_ROOT),
        &[sel("blog-3f2a-ffi", PackageKind::Ffi)],
        true,
    );
    assert_eq!(plan.len(), 1);
    let inv = &plan[0];

    let floor = "profile.release.panic=\"unwind\"";
    let idx = inv
        .args
        .iter()
        .position(|a| a == floor)
        .unwrap_or_else(|| panic!("no unwind floor in {:?}", inv.args));
    assert_eq!(
        inv.args[idx - 1],
        "--config",
        "the floor must be a `--config` value, not a bare argument"
    );

    assert!(
        inv.env.is_empty(),
        "the floor must not be an environment variable — an env var is invisible in \
         `--plan`, which is the whole reason a `--config` argument was chosen: {:?}",
        inv.env
    );
    assert!(
        rendered(&plan).contains("profile.release.panic"),
        "the floor must survive into what `--plan` prints:\n{}",
        rendered(&plan)
    );
}

/// The debug profile gets the same floor under its own name.
///
/// `dev` is the profile's NAME; `debug` is only the directory it lands in, and
/// `--config profile.debug.panic` would silently configure a profile that does
/// not exist.
#[test]
fn scenario_24_the_debug_floor_names_the_dev_profile() {
    let plan = driver::plan(
        Path::new(CACHE_ROOT),
        &[sel("blog-3f2a-ffi", PackageKind::Ffi)],
        false,
    );
    assert!(
        plan[0]
            .args
            .contains(&"profile.dev.panic=\"unwind\"".to_string()),
        "{:?}",
        plan[0].args
    );
    assert!(
        !plan[0].args.iter().any(|a| a.contains("profile.debug.")),
        "`debug` is a directory, not a profile name: {:?}",
        plan[0].args
    );
}

/// The wasm arm carries **no** panic floor.
///
/// Not an oversight: the floor exists to keep the `catch_unwind` boundary in the
/// generated `ffi`/`napi` wrappers real, and the browser replica has no such
/// boundary — `catch_unwind` is emitted by `crates/codegen/src/{ffi,napi}.rs`
/// and by neither `wasm.rs` nor anything it writes. Forcing unwinding there buys
/// nothing and costs unwind tables in a bundle that travels over the network.
///
/// **This comment previously said `-C panic=unwind` is a hard rustc error on
/// `wasm32-unknown-unknown`. That is false** — the pinned toolchain builds a
/// wasm cdylib cleanly under `--config 'profile.release.panic="unwind"'` *and*
/// under `…="abort"`, both exit 0. The decision above does not depend on it, but
/// a false reason in a doc comment is how the next author "fixes" the right
/// behaviour.
#[test]
fn scenario_24_the_wasm_arm_carries_no_panic_floor() {
    let plan = driver::plan(
        Path::new(CACHE_ROOT),
        &[sel("blog-3f2a-wasm", PackageKind::Wasm)],
        true,
    );
    assert_eq!(plan.len(), 1);
    assert!(
        !plan[0].args.iter().any(|a| a.contains("panic")),
        "{:?}",
        plan[0].args
    );
    // The size setting that used to be a `[profile.release]` table inside the
    // replica's OWN manifest — where cargo silently ignores it, because a
    // profile in a workspace member is not applied.
    assert!(
        plan[0]
            .args
            .contains(&"profile.release.opt-level=\"s\"".to_string()),
        "{:?}",
        plan[0].args
    );
}

// ---------------------------------------------------------------------------
// Scenario 25 — duplicate artifact names, refused before any compile
// ---------------------------------------------------------------------------

/// A `cargo metadata --no-deps` document with two packages, each declaring a
/// `[[bin]]` of the given name.
fn metadata_with_two_bins(a: &str, b: &str) -> serde_json::Value {
    serde_json::json!({
        "packages": [
            {
                "name": "blog-3f2a-transform-1-2",
                "targets": [ { "kind": ["bin"], "crate_types": ["bin"], "name": a } ]
            },
            {
                "name": "blog-3f2a-engine-1-2",
                "targets": [ { "kind": ["bin"], "crate_types": ["bin"], "name": b } ]
            }
        ]
    })
}

/// **Scenario 25 (pure half).** *Given* one app with both a `transform-*` and an
/// `engine-*` package whose bins share a name · *When* the pre-build guard runs ·
/// *Then* it errors **naming both packages**.
///
/// This ships broken today: cargo emits `warning: output filename collision`,
/// **exits 0**, and leaves one of the two files behind — while `migrate.rs`
/// resolves the transformer by a fixed name. So the CLI can run the *wrong hop*
/// over a user's data directory at exit 0, which is a data-corruption-class
/// failure hiding behind a warning. `cargo check` never links, so it cannot see
/// the condition at all; only a real `build` can, and only after doing the
/// damage. Hence a guard that needs no compile.
#[test]
fn scenario_25_duplicate_bin_names_are_named_with_both_packages() {
    let meta = metadata_with_two_bins("forgedb-transform", "forgedb-transform");
    let msg =
        driver::duplicate_artifact_names(&meta).expect("two bins with one name must be refused");

    assert!(
        msg.contains("blog-3f2a-transform-1-2") && msg.contains("blog-3f2a-engine-1-2"),
        "the error must name BOTH packages, not just the collision:\n{msg}"
    );
    assert!(
        msg.contains("forgedb-transform"),
        "the error must name the artifact:\n{msg}"
    );
}

/// The control that keeps the assertion above from being vacuous: with the bins
/// range-stamped, the same shape passes.
#[test]
fn scenario_25_range_stamped_bins_do_not_collide() {
    let meta = metadata_with_two_bins("forgedb-transform-1-2", "forgedb-engine-1-2");
    assert_eq!(driver::duplicate_artifact_names(&meta), None);
}

/// Two targets sharing a name in **different output classes** are not a
/// collision: `foo`, `libfoo.a` and `libfoo.dylib` are three different files.
///
/// Without this the guard would refuse every app that emits both `ffi` (a
/// staticlib) and a bin of the same stem — a false positive on a legal shape,
/// which is the way a guard gets deleted.
#[test]
fn scenario_25_a_bin_and_a_staticlib_of_one_name_are_not_a_collision() {
    let meta = serde_json::json!({
        "packages": [
            { "name": "p-one", "targets": [ { "kind": ["bin"], "name": "shared" } ] },
            { "name": "p-two", "targets": [ { "kind": ["staticlib", "rlib"], "name": "shared" } ] }
        ]
    });
    assert_eq!(driver::duplicate_artifact_names(&meta), None);
}

/// Two cdylibs of one name DO collide — the class is "dynamic library", not the
/// exact cargo `crate-type` spelling, because `cdylib` and `dylib` write the
/// same `lib<name>.dylib`/`.so`.
#[test]
fn scenario_25_cdylib_and_dylib_share_one_output_class() {
    let meta = serde_json::json!({
        "packages": [
            { "name": "p-one", "targets": [ { "kind": ["cdylib"], "name": "shared" } ] },
            { "name": "p-two", "targets": [ { "kind": ["dylib"], "name": "shared" } ] }
        ]
    });
    let msg = driver::duplicate_artifact_names(&meta).expect("a dylib/cdylib clash is a clash");
    assert!(msg.contains("p-one") && msg.contains("p-two"), "{msg}");
}

// ---------------------------------------------------------------------------
// Scenario 26 — `--plan` is `plan()`'s user-facing surface
// ---------------------------------------------------------------------------

/// **Scenario 26 (pure half).** Everything a reader needs in order to check the
/// build is in the rendered command line: the manifest, the profile, the floor
/// and the package set.
///
/// The rendering is shell-quoted because the floor's value is TOML and carries
/// double quotes. A plan a user cannot paste is a plan they will not check, and
/// an unchecked plan is exactly the drift `--plan` exists to prevent.
#[test]
fn scenario_26_the_rendered_plan_shows_manifest_profile_floor_and_packages() {
    let plan = driver::plan(
        Path::new(CACHE_ROOT),
        &[
            sel("blog-3f2a-core", PackageKind::Core),
            sel("blog-3f2a-server", PackageKind::Server),
        ],
        true,
    );
    let text = rendered(&plan);

    for needle in [
        "cargo build",
        "--manifest-path",
        "/home/u/.forgedb/projects/blog/Cargo.toml",
        "--message-format=json-render-diagnostics",
        "--release",
        "profile.release.panic",
        "-p blog-3f2a-core",
        "-p blog-3f2a-server",
    ] {
        assert!(
            text.contains(needle),
            "`--plan` never shows `{needle}`:\n{text}"
        );
    }
}

/// The quoting is real quoting: a `sh -c` of the rendered line must see the
/// floor as ONE argument whose value still carries its TOML quotes.
#[test]
fn scenario_26_the_floor_survives_shell_quoting() {
    let plan = driver::plan(Path::new(CACHE_ROOT), &[sel("p", PackageKind::Core)], true);
    let line = plan[0].command_line();
    assert!(
        line.contains(r#"'profile.release.panic="unwind"'"#),
        "the floor must be quoted as one shell word:\n{line}"
    );
}

/// `Invocation` can be read back: the `-p` set and the `--target`.
///
/// Anchored on the tokens that do the work (`-p`, `--target`), never on a
/// position or a binding name — the plan's argument order is free to change.
#[test]
fn scenario_26_an_invocation_reports_its_own_packages_and_triple() {
    let plan = driver::plan(
        Path::new(CACHE_ROOT),
        &[
            sel("a-core", PackageKind::Core),
            sel("a-wasm", PackageKind::Wasm),
        ],
        true,
    );
    assert_eq!(plan.len(), 2, "the wasm arm is forced: {plan:#?}");
    assert_eq!(plan[0].packages(), vec!["a-core".to_string()]);
    assert_eq!(plan[0].triple(), None);
    assert_eq!(plan[1].packages(), vec!["a-wasm".to_string()]);
    assert_eq!(plan[1].triple(), Some(WASM_TRIPLE));
}

// ---------------------------------------------------------------------------
// Scenario 27 — every artifact is carried with its kind
// ---------------------------------------------------------------------------

/// The **recorded** native stream: a `core` rlib (whose `package_id` cargo
/// abbreviates, because its directory is named after it), an `ffi` lib with
/// three crate types, a `server` bin, and the `custom-build` artifact of that
/// bin's build script — plus `build-script-executed` and `build-finished`.
///
/// See this file's module doc, and `tests/fixtures/README.md`, for how it was
/// recorded and why it is not written by hand.
const STREAM: &str = include_str!("fixtures/cargo_stream_native.jsonl");

/// The **recorded** wasm stream: `crate-type = ["cdylib"]` built for
/// `wasm32-unknown-unknown`, whose single filename is a `.wasm`.
///
/// Recorded separately because it is a separate cargo invocation — which is
/// precisely what [`driver::plan`] emits for the wasm arm (scenario 24).
const WASM_STREAM: &str = include_str!("fixtures/cargo_stream_wasm.jsonl");

/// Two `package_id` spellings the recorded workspace cannot produce, because it
/// is dep-free (no registry ids) and built by a modern cargo (no legacy form).
///
/// Marked synthetic on purpose: unlike [`STREAM`], nothing here is evidence of
/// what cargo emits — it is evidence that the *parser* accepts both historical
/// spellings, which is all it is asked to prove.
const SYNTHETIC_ID_VARIANTS: &str = concat!(
    r#"{"reason":"compiler-artifact","package_id":"registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0","target":{"kind":["lib"],"crate_types":["rlib"],"name":"serde"},"filenames":["/c/target/release/libserde.rlib"],"executable":null}"#,
    "\n",
    r#"{"reason":"compiler-artifact","package_id":"legacy-crate 0.1.0 (path+file:///c/legacy-crate)","target":{"kind":["lib"],"crate_types":["rlib"],"name":"legacy_crate"},"filenames":["/c/target/release/liblegacy_crate.rlib"],"executable":null}"#,
    "\n",
);

fn parsed() -> Vec<Artifact> {
    driver::parse_artifacts(STREAM)
}

fn of(package: &str) -> Vec<Artifact> {
    parsed()
        .into_iter()
        .filter(|a| a.package == package)
        .collect()
}

/// **Scenario 27 (pure half).** One `ffi` package reports **three** files, and
/// the three are distinguishable by kind.
///
/// All three exist on disk, so existence-checking cannot tell them apart — and
/// Go delivery needs the **staticlib** specifically. That is the whole reason
/// `TargetKind` is carried on `Artifact` instead of being re-derived by whoever
/// consumes the report.
#[test]
fn scenario_27_one_ffi_package_reports_three_distinguishable_kinds() {
    let ffi = of("blog-h-ffi");
    let kinds: Vec<TargetKind> = ffi.iter().map(|a| a.kind).collect();
    assert!(kinds.contains(&TargetKind::Staticlib), "{ffi:#?}");
    assert!(kinds.contains(&TargetKind::Cdylib), "{ffi:#?}");
    assert!(kinds.contains(&TargetKind::Rlib), "{ffi:#?}");

    let staticlib = ffi
        .iter()
        .find(|a| a.kind == TargetKind::Staticlib)
        .expect("a staticlib");
    assert_eq!(
        staticlib.path,
        PathBuf::from("/tmp/forgedb-cargo-stream/target/release/libblog_h_ffi.a")
    );
}

/// `.rmeta` is not an artifact.
///
/// Cargo reports it beside every `.rlib`; reporting it would make
/// `--print-artifact core` **ambiguous** and therefore a hard error on a
/// perfectly ordinary build.
#[test]
fn scenario_27_rmeta_is_not_reported() {
    assert!(
        !parsed()
            .iter()
            .any(|a| a.path.extension().is_some_and(|e| e == "rmeta")),
        "{:#?}",
        parsed()
    );
    let core = of("blog-h-core");
    assert_eq!(core.len(), 1, "a core rlib is ONE artifact: {core:#?}");
    assert_eq!(core[0].kind, TargetKind::Rlib);
}

/// A build script's own `compiler-artifact` message is not a deliverable.
///
/// **The recording corrected this test's premise.** It was written believing a
/// build script reports an `executable`; cargo 1.96 sends
/// `"executable": null` and puts the `build-script-build` path in `filenames`
/// only. The `custom-build` filter is therefore belt-and-braces over
/// `kind_from_filename` (an extensionless path classifies as nothing) rather
/// than the sole defense it was thought to be — but it is still the one that
/// holds if cargo starts populating `executable`, which it did in older
/// releases.
///
/// The first assertion is the anti-vacuity guard: without a `custom-build`
/// message in the fixture there is nothing here to filter and the second
/// assertion passes for free.
#[test]
fn scenario_27_a_build_script_is_not_an_artifact() {
    assert!(
        STREAM.contains(r#""kind":["custom-build"]"#),
        "the fixture must actually contain a build-script artifact message, \
         or this test asserts nothing"
    );
    assert!(
        !parsed()
            .iter()
            .any(|a| a.path.to_string_lossy().contains("build-script-build")),
        "{:#?}",
        parsed()
    );
}

/// A bin arrives via `executable`, exactly once, and its extensionless copy in
/// `filenames` does not double it.
#[test]
fn scenario_27_a_bin_is_reported_once() {
    let server = of("blog-h-server");
    assert_eq!(server.len(), 1, "{server:#?}");
    assert_eq!(server[0].kind, TargetKind::Bin);
    assert_eq!(
        server[0].path,
        PathBuf::from("/tmp/forgedb-cargo-stream/target/release/blog-h-server")
    );
}

/// The `package_id` spelling that omits the name is read correctly.
///
/// Cargo drops the `#<name>@` half whenever the last path segment already equals
/// the package name — which is *most* of ForgeDB's cache packages the moment a
/// directory is named after its package. A driver that understood only the other
/// spelling would report an empty artifact list from a successful build.
#[test]
fn scenario_27_a_nameless_package_id_still_yields_the_package() {
    assert!(
        STREAM.contains("/blog-h-core#0.1.0"),
        "the fixture must carry the abbreviated spelling for this to test anything"
    );
    assert!(
        parsed().iter().any(|a| a.package == "blog-h-core"),
        "the abbreviated package_id form was not read:\n{:#?}",
        parsed()
    );
}

/// The two `package_id` spellings the recorded workspace cannot produce.
///
/// A registry id (`registry+…#serde@1.0.0`) needs a dependency, and the legacy
/// `name version (source)` id needs a cargo older than the one recording. Both
/// are still reachable in the wild, so the parser is asked about them directly —
/// from [`SYNTHETIC_ID_VARIANTS`], which is labelled synthetic because it proves
/// something about the parser and nothing about cargo.
#[test]
fn scenario_27_the_historical_package_id_spellings_still_parse() {
    let got: Vec<String> = driver::parse_artifacts(SYNTHETIC_ID_VARIANTS)
        .into_iter()
        .map(|a| a.package)
        .collect();
    assert!(got.contains(&"serde".to_string()), "{got:?}");
    assert!(got.contains(&"legacy-crate".to_string()), "{got:?}");
}

/// **Scenario 27 (wasm half).** The replica's `.wasm` is a `cdylib`, and it is
/// the *only* artifact its invocation reports.
///
/// Recorded from a real `--target wasm32-unknown-unknown` build. `.wasm` is not
/// a suffix any host target produces, so without an explicit arm in
/// `kind_from_filename` the browser replica would build successfully and then
/// report nothing at all — a silent hole of exactly the class #335 exists to
/// close.
#[test]
fn scenario_27_the_wasm_arm_reports_one_cdylib() {
    let got = driver::parse_artifacts(WASM_STREAM);
    assert_eq!(got.len(), 1, "{got:#?}");
    assert_eq!(got[0].package, "blog-h-wasm");
    assert_eq!(got[0].kind, TargetKind::Cdylib);
    assert_eq!(
        got[0].path,
        PathBuf::from(
            "/tmp/forgedb-cargo-stream/target/wasm32-unknown-unknown/release/blog_h_wasm.wasm"
        )
    );
}

// ---------------------------------------------------------------------------
// Scenario 27 (cont.) — the report's selector
// ---------------------------------------------------------------------------

fn report_of(rows: &[(&str, TargetKind, &str)]) -> BuildReport {
    BuildReport {
        version: 1,
        project: PathBuf::from(CACHE_ROOT),
        app: PathBuf::from(APP),
        profile: Profile::Release,
        artifacts: rows
            .iter()
            .map(|(kind, target_kind, path)| ReportedArtifact {
                package: format!("blog-3f2a-{kind}"),
                kind: kind.to_string(),
                target_kind: *target_kind,
                path: PathBuf::from(*path),
                triple: "aarch64-apple-darwin".to_string(),
            })
            .collect(),
        delivered: Vec::new(),
    }
}

/// `--print-artifact ffi` selects the **staticlib**, not the rlib or the cdylib
/// of the same package.
#[test]
fn scenario_27_print_artifact_ffi_selects_the_staticlib() {
    let report = report_of(&[
        ("ffi", TargetKind::Cdylib, "/t/libblog_3f2a_ffi.dylib"),
        ("ffi", TargetKind::Rlib, "/t/libblog_3f2a_ffi.rlib"),
        ("ffi", TargetKind::Staticlib, "/t/libblog_3f2a_ffi.a"),
    ]);
    assert_eq!(
        report.print_artifact("ffi").unwrap(),
        Path::new("/t/libblog_3f2a_ffi.a")
    );
}

/// Each kind's primary target kind, asserted as a table so a new package kind
/// cannot be added without deciding what `--print-artifact` means for it.
#[test]
fn scenario_27_every_kind_has_one_primary_target_kind() {
    for (kind, want) in [
        (PackageKind::Core, TargetKind::Rlib),
        (PackageKind::Server, TargetKind::Bin),
        (PackageKind::Ffi, TargetKind::Staticlib),
        (PackageKind::Napi, TargetKind::Cdylib),
        (PackageKind::Pyo3, TargetKind::Cdylib),
        (PackageKind::Wasm, TargetKind::Cdylib),
        (PackageKind::Transform { from: 1, to: 2 }, TargetKind::Bin),
        (PackageKind::Engine { from: 1, to: 2 }, TargetKind::Bin),
    ] {
        assert_eq!(
            driver::primary_target_kind(&kind),
            want,
            "{} chose the wrong primary artifact",
            kind.dir()
        );
    }
}

/// Zero matches is a hard error naming what WAS found — never silence, and never
/// a guessed pick. Silence here is how a container ships the wrong binary.
#[test]
fn scenario_27_print_artifact_with_no_match_errors_and_names_the_inventory() {
    let report = report_of(&[("core", TargetKind::Rlib, "/t/libblog_3f2a_core.rlib")]);
    let err = report.print_artifact("server").unwrap_err().to_string();
    assert!(err.contains("matched nothing"), "{err}");
    assert!(
        err.contains("libblog_3f2a_core.rlib"),
        "the error must show what the build DID produce:\n{err}"
    );
}

/// More than one match is a hard error too.
#[test]
fn scenario_27_print_artifact_with_an_ambiguous_match_errors() {
    let report = report_of(&[
        ("ffi", TargetKind::Staticlib, "/t/a/libx.a"),
        ("ffi", TargetKind::Staticlib, "/t/b/libx.a"),
    ]);
    let err = report.print_artifact("ffi").unwrap_err().to_string();
    assert!(err.contains("ambiguous"), "{err}");
    assert!(
        err.contains("/t/a/libx.a") && err.contains("/t/b/libx.a"),
        "{err}"
    );
}

/// `--print-artifact` takes a **kind**, never a package name.
///
/// A package name is derived from the app's path (`naming::app_name`), so a
/// Dockerfile that baked one would break the moment the schema file is moved or
/// renamed — silently, in a file the user does not re-read.
#[test]
fn scenario_27_print_artifact_refuses_a_package_name() {
    let report = report_of(&[("server", TargetKind::Bin, "/t/acme_blog-server")]);
    let err = report
        .print_artifact("acme_blog-server")
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a ForgeDB package kind"), "{err}");
    assert!(
        err.contains("core, server"),
        "the legal set must be listed:\n{err}"
    );
}

/// The report round-trips through `serde_json`, and its `kind` values are
/// exactly `PackageKind::dir()` — the stable selector a Dockerfile may name.
#[test]
fn scenario_27_the_report_serializes_to_the_contract_shape() {
    let report = report_of(&[("ffi", TargetKind::Staticlib, "/t/libx.a")]);
    let json: serde_json::Value = serde_json::from_str(&report.to_json().unwrap()).unwrap();

    assert_eq!(json["version"], 1);
    assert_eq!(json["profile"], "release");
    assert_eq!(json["project"], CACHE_ROOT);
    assert_eq!(json["app"], APP);
    let a = &json["artifacts"][0];
    assert_eq!(a["kind"], "ffi");
    assert_eq!(a["target_kind"], "staticlib");
    assert_eq!(a["path"], "/t/libx.a");
    assert_eq!(a["triple"], "aarch64-apple-darwin");
    assert!(
        PackageKind::from_dir(a["kind"].as_str().unwrap()).is_some(),
        "`kind` must round-trip through PackageKind::from_dir"
    );
}

// ---------------------------------------------------------------------------
// Scenario 26 (cont.) — who owns stdout
// ---------------------------------------------------------------------------

/// **Scenario 26.** The predicate that decides whether stdout belongs to a
/// machine, asserted as a table.
///
/// `--report <file>` must NOT claim stdout: the document goes to the file, so
/// silencing the build's human output would make an ordinary `forgedb build
/// --report out.json` print nothing at all. Only `--report -` and
/// `--print-artifact` hand stdout over.
#[test]
fn scenario_26_only_the_flags_that_write_to_stdout_claim_it() {
    use forgedb::commands::build::stdout_is_machine_readable as claims;

    assert!(!claims(None, None), "a plain build keeps its human output");
    assert!(!claims(None, Some("artifacts.json")), "--report <file>");
    assert!(claims(None, Some("-")), "--report -");
    assert!(claims(Some("server"), None), "--print-artifact");
    assert!(
        claims(Some("server"), Some("artifacts.json")),
        "the Dockerfile combines both, and the path is still the only thing on stdout"
    );
}

/// The predicate has exactly ONE definition, and `main.rs` calls it.
///
/// It was two: a method on `BuildOptions` that nothing could reach — the CLI arm
/// has to silence output *before* it can assemble a `BuildOptions`, because
/// `identify_reported` and `reserve_in_cache` print — plus an inline copy of the
/// same condition in `main.rs`. Two copies of "is stdout spoken for" drift the
/// moment a third machine-readable flag is added, and the failure is a
/// Dockerfile's `$(…)` capturing a banner.
///
/// Anchored on the **call**, and on the absence of the re-derived condition —
/// never on a binding name.
#[test]
fn the_stdout_ownership_rule_is_not_re_derived_in_main() {
    const MAIN: &str = include_str!("../src/main.rs");
    assert!(
        MAIN.contains("commands::build::stdout_is_machine_readable("),
        "main.rs must CALL the predicate, not restate it"
    );
    assert!(
        !MAIN.contains(r#"report.as_deref() == Some("-")"#),
        "main.rs re-derives the stdout-ownership condition instead of calling it"
    );
}

// ---------------------------------------------------------------------------
// Scenario 28 — cargo is never pointed at the working directory
// ---------------------------------------------------------------------------

/// **Scenario 28 (pure half).** Every planned invocation names the **cache
/// workspace** as both its manifest and its working directory — never the
/// process's CWD.
///
/// This is the headline defect, reproduced end to end on `develop`: `forgedb
/// build` ran a bare `cargo build` with no `--manifest-path` and no `-p`, so in a
/// directory holding an unrelated crate it compiled *that* crate, printed
/// `✓ Compiled database (native)`, and exited 0.
///
/// Note the assertion is on `--manifest-path` **and** `cwd`. Either alone would
/// pass while the other was wrong, and `--current-dir` is not a cargo flag while
/// `-C` is nightly-gated on the pinned 1.96 — so the two together are the
/// available belt and braces.
#[test]
fn scenario_28_every_invocation_is_anchored_on_the_cache_not_the_cwd() {
    let plan = driver::plan(
        Path::new(CACHE_ROOT),
        &[
            sel("blog-3f2a-core", PackageKind::Core),
            sel("blog-3f2a-wasm", PackageKind::Wasm),
        ],
        true,
    );
    assert_eq!(plan.len(), 2);
    for inv in &plan {
        assert_eq!(inv.cwd, PathBuf::from(CACHE_ROOT), "{inv:?}");
        let idx = inv
            .args
            .iter()
            .position(|a| a == "--manifest-path")
            .unwrap_or_else(|| panic!("no --manifest-path in {:?}", inv.args));
        assert_eq!(
            inv.args[idx + 1],
            format!("{CACHE_ROOT}/Cargo.toml"),
            "{:?}",
            inv.args
        );
        assert!(
            !inv.packages().is_empty(),
            "a bare `cargo build` with no `-p` is what compiled the foreign crate: {:?}",
            inv.args
        );
    }
}

/// Nothing is built when nothing is selected — and in particular, an app whose
/// target set declares no cargo package must not fall back to "build whatever is
/// here".
#[test]
fn scenario_28_an_app_with_no_cargo_package_plans_no_cargo_at_all() {
    assert!(driver::plan(Path::new(CACHE_ROOT), &[], true).is_empty());
}

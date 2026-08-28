use forgedb::commands::build::driver::{
    self, Artifact, BuildReport, Invocation, Profile, ReportedArtifact, Selected, TargetKind,
    WASM_TRIPLE,
};
use forgedb::naming::PackageKind;
use std::path::{Path, PathBuf};

const CACHE_ROOT: &str = "/home/u/.forgedb/projects/blog";
const APP: &str = "/home/u/.forgedb/projects/blog/apps/3f2a91c04d5e7b60";

fn sel(package: &str, kind: PackageKind) -> Selected {
    Selected {
        package: package.to_string(),
        kind,
    }
}

fn rendered(invocations: &[Invocation]) -> String {
    invocations
        .iter()
        .map(|i| i.command_line())
        .collect::<Vec<_>>()
        .join("\n")
}

const DRIVER_SRC: &str = include_str!("../src/commands/build/driver.rs");

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
    assert!(
        plan[0]
            .args
            .contains(&"profile.release.opt-level=\"s\"".to_string()),
        "{:?}",
        plan[0].args
    );
}

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

#[test]
fn scenario_25_range_stamped_bins_do_not_collide() {
    let meta = metadata_with_two_bins("forgedb-transform-1-2", "forgedb-engine-1-2");
    assert_eq!(driver::duplicate_artifact_names(&meta), None);
}

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

#[test]
fn scenario_26_the_floor_survives_shell_quoting() {
    let plan = driver::plan(Path::new(CACHE_ROOT), &[sel("p", PackageKind::Core)], true);
    let line = plan[0].command_line();
    assert!(
        line.contains(r#"'profile.release.panic="unwind"'"#),
        "the floor must be quoted as one shell word:\n{line}"
    );
}

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

const STREAM: &str = include_str!("fixtures/cargo_stream_native.jsonl");

const WASM_STREAM: &str = include_str!("fixtures/cargo_stream_wasm.jsonl");

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

#[test]
fn scenario_27_the_historical_package_id_spellings_still_parse() {
    let got: Vec<String> = driver::parse_artifacts(SYNTHETIC_ID_VARIANTS)
        .into_iter()
        .map(|a| a.package)
        .collect();
    assert!(got.contains(&"serde".to_string()), "{got:?}");
    assert!(got.contains(&"legacy-crate".to_string()), "{got:?}");
}

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

#[test]
fn scenario_28_an_app_with_no_cargo_package_plans_no_cargo_at_all() {
    assert!(driver::plan(Path::new(CACHE_ROOT), &[], true).is_empty());
}

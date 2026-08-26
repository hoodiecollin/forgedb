use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Helper to create a temporary directory for testing
fn setup_test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

/// Build a `forgedb` CLI invocation scoped to `dir`.
///
/// Commands that auto-discover `schema.forge` do so from the process working
/// directory. Running the real binary as a subprocess with an explicit
/// `current_dir` gives each test its own working directory, so these tests are
/// hermetic and safe to run in parallel (no process-global `set_current_dir`).
fn forgedb_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    // #333: keep every claim this suite makes inside its own tempdir rather than
    // in the developer's real `~/.forgedb`.
    cmd.env("FORGEDB_HOME", dir.join(".forgedb-home"));
    cmd.current_dir(dir);
    cmd
}

/// Helper to create a basic schema file
fn create_test_schema(dir: &Path) {
    let schema = r#"User {
  id: +uuid
  email: ^&string @email
  created_at: +timestamp
}
"#;
    fs::write(dir.join("schema.forge"), schema).expect("Failed to write schema");
}

#[test]
fn test_init_command_creates_project_structure() {
    use forgedb::commands::init::{run, InitOptions};

    let temp_dir = setup_test_dir();
    let project_name = "test_project";
    let project_path = temp_dir.path().join(project_name);

    let options = InitOptions {
        project_name: project_path.to_string_lossy().to_string(),
        template: None,
        // Both are TOMBSTONES (#335 §15): they still exist so that setting one
        // is an error naming its replacement, so a fixture that sets either is
        // asserting the refusal, not the scaffold. What they used to select is
        // now `[generate].targets`.
        rust: false,
        api_only: false,
        // Explicit: these fixtures live in a tempdir with no enclosing project,
        // and pinning the answer keeps them independent of what is above them.
        isolated: Some(true),
    };

    let result = run(options);
    assert!(result.is_ok(), "Init command should succeed");

    // Check that directories were created
    assert!(project_path.exists());
    assert!(project_path.join("generated").exists());
    assert!(project_path.join("data/db").exists());

    // Check that files were created
    assert!(project_path.join("schema.forge").exists());
    assert!(project_path.join("forgedb.toml").exists());
    assert!(project_path.join(".gitignore").exists());
    assert!(project_path.join("README.md").exists());

    // ...and that the two the scaffold STOPPED writing are absent (#335 §15).
    // Asserted rather than merely deleted: `init` scaffolding no cargo package
    // is the change, and an absent assertion cannot notice it coming back.
    assert!(
        !project_path.join("Cargo.toml").exists(),
        "init scaffolded a cargo package; the generated Rust is compiled in \
         ForgeDB's build cache now"
    );
    assert!(
        !project_path.join("src").exists(),
        "init scaffolded a src/ directory; the server is a cache artifact"
    );
}

#[test]
fn test_init_with_blog_template() {
    use forgedb::commands::init::{run, InitOptions};

    let temp_dir = setup_test_dir();
    let project_name = "blog_project";
    let project_path = temp_dir.path().join(project_name);

    let options = InitOptions {
        project_name: project_path.to_string_lossy().to_string(),
        template: Some("blog".to_string()),
        // Both are TOMBSTONES (#335 §15): they still exist so that setting one
        // is an error naming its replacement, so a fixture that sets either is
        // asserting the refusal, not the scaffold. What they used to select is
        // now `[generate].targets`.
        rust: false,
        api_only: false,
        // Explicit: these fixtures live in a tempdir with no enclosing project,
        // and pinning the answer keeps them independent of what is above them.
        isolated: Some(true),
    };

    let result = run(options);
    assert!(result.is_ok(), "Init with blog template should succeed");

    // Read schema and verify it contains blog-specific models
    let schema_content =
        fs::read_to_string(project_path.join("schema.forge")).expect("Failed to read schema");
    assert!(schema_content.contains("Post"));
    assert!(schema_content.contains("Tag"));
}

/// #115 on-host deploy: the Rust scaffold path emits the systemd unit, its
/// `EnvironmentFile`, and an install README under `deploy/`, alongside the Docker
/// artifacts. All name-templated only (identity: inert ops packaging), so the
/// unit's `StateDirectory` and the env file's `FORGEDB_DATA` must agree.
#[test]
fn test_init_emits_onhost_systemd_deploy() {
    use forgedb::commands::init::{run, InitOptions};

    let temp_dir = setup_test_dir();
    let project_name = "acme_svc";
    let project_path = temp_dir.path().join(project_name);

    let options = InitOptions {
        project_name: project_path.to_string_lossy().to_string(),
        template: None,
        // Both are TOMBSTONES (#335 §15): they still exist so that setting one
        // is an error naming its replacement, so a fixture that sets either is
        // asserting the refusal, not the scaffold. What they used to select is
        // now `[generate].targets`.
        rust: false,
        api_only: false,
        // Explicit: these fixtures live in a tempdir with no enclosing project,
        // and pinning the answer keeps them independent of what is above them.
        isolated: Some(true),
    };
    run(options).expect("init should succeed");

    // The Docker path still emits (parity, not replaced).
    assert!(project_path.join("Dockerfile").exists());

    let deploy = project_path.join("deploy");
    let service =
        fs::read_to_string(deploy.join("acme_svc.service")).expect("systemd unit emitted");
    let env = fs::read_to_string(deploy.join("acme_svc.env")).expect("EnvironmentFile emitted");
    assert!(deploy.join("README.md").exists(), "install README emitted");

    // Load-bearing unit directives.
    assert!(service.contains("ExecStart=/usr/local/bin/acme_svc"));
    assert!(service.contains("EnvironmentFile=/etc/acme_svc/acme_svc.env"));
    assert!(service.contains("Type=exec"), "readiness model is exec, not notify");
    assert!(service.contains("DynamicUser=yes"), "non-root without a manual useradd");
    assert!(service.contains("StateDirectory=acme_svc"), "managed data dir");
    assert!(service.contains("KillSignal=SIGTERM"), "pairs with graceful shutdown");
    assert!(service.contains("NoNewPrivileges=yes"), "hardening present");
    assert!(service.contains("WantedBy=multi-user.target"));

    // The env file's data dir must match the unit's StateDirectory (/var/lib/<name>).
    assert!(env.contains("FORGEDB_DATA=/var/lib/acme_svc"), "env data dir agrees with StateDirectory");
    assert!(env.contains("FORGEDB_HOST=0.0.0.0"));

    // Identity guard: a single plain unit, never a templated <name>@.service that
    // would invite two writers against one data dir (the per-tenant template is
    // documented-only). No models/fields leak into the emitted artifacts.
    assert!(!deploy.join("acme_svc@.service").exists(), "no instance-template unit scaffolded");
}

#[test]
fn test_generate_command_creates_rust_code() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    let output = forgedb_cmd(temp_dir.path())
        .args(["generate", "rust", "--output", "generated", "--force"])
        .output()
        .expect("Failed to run forgedb generate");
    assert!(
        output.status.success(),
        "Generate command should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check that generated code exists
    let generated_file = temp_dir.path().join("generated/database.rs");
    assert!(
        generated_file.exists(),
        "Generated database.rs should exist"
    );

    // Verify generated code contains expected content
    let generated_content =
        fs::read_to_string(&generated_file).expect("Failed to read generated code");
    assert!(generated_content.contains("User"));
    assert!(generated_content.contains("email"));
}

#[test]
fn test_validate_command_detects_errors() {
    let temp_dir = setup_test_dir();

    // Create an invalid schema (model name not PascalCase)
    let invalid_schema = r#"user {
  id: +uuid
  Email: string
}
"#;
    fs::write(temp_dir.path().join("schema.forge"), invalid_schema)
        .expect("Failed to write schema");

    let output = forgedb_cmd(temp_dir.path())
        .arg("validate")
        .output()
        .expect("Failed to run forgedb validate");
    // Should fail due to validation errors in the parsing phase
    assert!(
        !output.status.success(),
        "Invalid schema should fail validation"
    );
}

#[test]
fn test_validate_command_passes_valid_schema() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    let output = forgedb_cmd(temp_dir.path())
        .args(["validate", "--schema-only"])
        .output()
        .expect("Failed to run forgedb validate");
    assert!(
        output.status.success(),
        "Valid schema should pass validation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// C1: a second consecutive `generate` must not fail with "File exists".
///
/// This test used to assert the OPPOSITE for the bare command, and the comment
/// it carried is the reason the bug survived: C1 fixed `build` by having it
/// pass `force: true` internally, and then pinned the unfixed `generate` here
/// as "baseline for why the bug existed". A defect asserted by a passing test
/// stops reading as a defect. `forgedb generate` — the command the scaffolded
/// README, `migrate create`'s printed next steps and `generate --check`'s own
/// remedy line all tell you to run — exited 1 on every run after the first,
/// with a green suite.
///
/// So the third leg is inverted rather than deleted: the run WITHOUT `--force`
/// is now the one that must succeed, and `--force` is checked only for still
/// being accepted. `tests/cli_loop_test.rs` carries the rest, including the
/// case this shape cannot see — a `generate` that skips silently instead of
/// refusing loudly also passes every assertion below.
#[test]
fn test_generate_all_is_idempotent() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    let run = |args: &[&str], what: &str| {
        let out = forgedb_cmd(temp_dir.path())
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("failed to run forgedb {args:?}: {e}"));
        assert!(
            out.status.success(),
            "{what} should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };

    run(&["generate", "all", "--output", "generated"], "first generate all");
    run(&["generate", "all", "--output", "generated"], "second generate all");
    run(
        &["generate", "all", "--output", "generated", "--force"],
        "generate all --force (still accepted)",
    );
}

#[test]
fn test_generate_check_mode() {
    let temp_dir = setup_test_dir();
    create_test_schema(temp_dir.path());

    // First generate code
    let output = forgedb_cmd(temp_dir.path())
        .args(["generate", "rust", "--output", "generated", "--force"])
        .output()
        .expect("Failed to run forgedb generate");
    assert!(
        output.status.success(),
        "Generate should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Now check mode should pass for up-to-date code
    let output = forgedb_cmd(temp_dir.path())
        .args(["generate", "rust", "--output", "generated", "--check"])
        .output()
        .expect("Failed to run forgedb generate --check");
    assert!(
        output.status.success(),
        "Check mode should pass for up-to-date code: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `migrate create` detects by default, records what it can prove, and REFUSES
/// what it cannot when there is nobody to ask (#374).
///
/// Three steps over one evolving app:
///
/// 1. a baseline records a snapshot with nothing to diff;
/// 2. an additive nullable field is fully provable and is recorded;
/// 3. a type change (`u32 -> string`) is NOT provable, and in a session with no
///    terminal it is a hard error naming the field — not a scaffold of TODOs
///    the operator discovers weeks later at `migrate build`.
///
/// Step 3 is the behaviour change #374 exists for, and the assertion that
/// **nothing is written** is the load-bearing half: a refusal that still left a
/// record behind would leave a lineage whose hop can never be built.
#[test]
fn test_migrate_create_records_the_provable_and_refuses_the_rest() {
    let temp_dir = setup_test_dir();
    let dir = temp_dir.path();

    let v1 = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";
    let v2_additive = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n}\n";
    // Not provable: `qty` changes u32 -> string and the differ cannot know the
    // re-encoding.
    let v3_authored =
        "Widget {\n  id: +uuid\n  sku: &string\n  qty: string\n  note: string?\n}\n";
    fs::write(dir.join("v1.forge"), v1).unwrap();
    fs::write(dir.join("v2.forge"), v2_additive).unwrap();
    fs::write(dir.join("v3.forge"), v3_authored).unwrap();

    // 1. Baseline records a snapshot with nothing to diff (succeeds, no migration).
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "baseline", "--schema", "v1.forge"])
        .output()
        .expect("run migrate baseline");
    assert!(out.status.success(), "baseline should succeed");
    assert!(dir.join("migrations/.schema-snapshot.forge").exists(), "snapshot recorded");
    assert!(dir.join("migrations/schemas/v1.forge").exists(), "baseline versioned schema recorded");

    // 2. Additive (new nullable field): provable, recorded, version bumped.
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "add_note", "--schema", "v2.forge"])
        .output()
        .expect("run migrate additive");
    assert!(out.status.success(), "additive migration should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("note"), "reports the added field: {stdout}");
    assert!(dir.join("migrations/schemas/v2.forge").exists(), "v2 versioned schema recorded");

    // 3. A type change with nobody to ask: refused, naming the field.
    let out = forgedb_cmd(dir)
        .args(["migrate", "create", "qty_to_string", "--schema", "v3.forge"])
        .output()
        .expect("run migrate authored");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "a change ForgeDB cannot prove must not be recorded unanswered: {combined}"
    );
    assert!(
        combined.contains("Widget.qty"),
        "the refusal must name the field: {combined}"
    );
    assert!(
        !dir.join("migrations/schemas/v3.forge").exists(),
        "a refused create must write NOTHING — a recorded v3 with no answer is a \
         lineage whose hop can never be built"
    );
}

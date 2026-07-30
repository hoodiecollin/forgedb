use crate::{error::CliError, ui, Result};
use forgedb_codegen::{
    ApiGenerator, FfiGenerator, GoGenerator, GoSdkGenerator, NapiGenerator, OpenApiGenerator,
    PyO3Generator, PythonSdkGenerator, RustGenerator, RustSdkGenerator, StubGenerator,
    TypeScriptGenerator, WasmGenerator,
};
use forgedb_parser::Parser;
use std::fs;
use std::path::{Path, PathBuf};

/// The `--sdk`/`--runtime`/`--replica` mode axis (#122). Orthogonal to the
/// runtime/language axis (`python`, `node`, `bun`, `browser`): a target names a
/// runtime, a mode names *how* to bind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateMode {
    /// Network REST client (`--sdk`).
    Sdk,
    /// In-process native FFI binding (`--runtime`).
    Runtime,
    /// In-process read-replica follower (`--replica`).
    Replica,
}

impl GenerateMode {
    fn flag(self) -> &'static str {
        match self {
            GenerateMode::Sdk => "--sdk",
            GenerateMode::Runtime => "--runtime",
            GenerateMode::Replica => "--replica",
        }
    }
}

pub struct GenerateOptions {
    pub target: String,
    /// The mode axis from `--sdk`/`--runtime`/`--replica` (#122). `None` for a
    /// standalone artifact target.
    pub mode: Option<GenerateMode>,
    pub check: bool,
    pub output: Option<String>,
    /// Explicit schema file path (from CLI `--schema` or config `[generate].schema`).
    /// When `None`, `find_schema_file()` searches for the default names.
    pub schema: Option<String>,
    /// Target list from `[generate].targets` in `forgedb.toml`.
    /// When present and `target` is `"all"`, only these targets are generated.
    /// Ignored for explicit single-target invocations.
    pub config_targets: Option<Vec<String>>,
    /// Generate-time runtime-behavior config (epic #126) resolved from the
    /// `[runtime]`/`[storage]` tables, baked into the emitted `database.rs`.
    pub gen_config: forgedb_codegen::GenConfig,
    pub force: bool,
    /// Origin format version for the `transform` target (#74 Phase 3).
    pub from: Option<u32>,
    /// Destination format version for the `transform` target (#74 Phase 3).
    pub to: Option<u32>,
}

pub fn run(options: GenerateOptions) -> Result<()> {
    ui::header("🔨", "Generating code from schema");

    // The `transform` target (#74 Phase 3) is special: it reads the committed
    // per-version schemas under `migrations/` for its `--from`/`--to` range, not
    // the single app schema — so it short-circuits before schema discovery.
    if options.target.to_lowercase() == "transform" {
        let from = options.from.ok_or_else(|| {
            CliError::Other("generate transform requires --from <version>".to_string())
        })?;
        let to = options.to.ok_or_else(|| {
            CliError::Other("generate transform requires --to <version>".to_string())
        })?;
        let output = PathBuf::from(
            options
                .output
                .as_deref()
                .unwrap_or("migrations/transform"),
        );
        return crate::commands::migrate::emit_transform(from, to, &output, options.force);
    }

    // Find schema file — explicit path takes priority over auto-discovery.
    let schema_path = match options.schema.as_deref() {
        Some(p) => p.to_string(),
        None => find_schema_file()?,
    };
    ui::info(&format!("Using schema: {}", schema_path));

    // Read and parse schema
    let schema_content = fs::read_to_string(&schema_path)
        .map_err(|e| CliError::SchemaNotFound(format!("{}: {}", schema_path, e)))?;

    let mut parser = Parser::new(&schema_content)
        .map_err(|e| CliError::SchemaValidation(format!("Lexer error: {}", e)))?;

    let schema = parser
        .parse()
        .map_err(|e| CliError::SchemaValidation(format!("Parser error: {}", e)))?;

    ui::success(&format!(
        "Parsed schema ({} models, {} total fields)",
        schema.models.len(),
        schema.models.iter().map(|m| m.fields.len()).sum::<usize>()
    ));

    // On-disk format version (#74 Phase 1/2): derive it from the committed
    // migration lineage under `migrations/` and bake it into the generated app's
    // `EXPECTED_FORMAT_VERSION` (red line #8 — lineage-sourced, never hand-edited).
    // Baseline `1` when there is no lineage (a project that has authored no
    // migrations yet).  The open guard compares this opaque integer and refuses a
    // stale data dir; it is threaded into every `database.rs` emission (the server
    // and the wasm replica share one lineage).
    let format_version = forgedb_migrations::current_format_version("migrations");

    // Determine the committed output directory.
    let output_dir = options.output.as_deref().unwrap_or("./generated");
    let committed_path = PathBuf::from(output_dir);

    // Check mode (CI staleness gate): instead of writing into the committed dir,
    // generate every artifact into a throwaway scratch dir, then compare it
    // byte-for-byte against what's committed — touching nothing on disk. In normal
    // mode `output_path` IS the committed dir and generation writes in place.
    let output_path = if options.check {
        std::env::temp_dir().join(format!("forgedb-check-{}", std::process::id()))
    } else {
        committed_path.clone()
    };
    // A stale scratch dir must never block a write, so check mode always forces.
    let force = options.force || options.check;

    // Create the output directory (a fresh scratch dir in check mode).
    if options.check {
        let _ = fs::remove_dir_all(&output_path);
    }
    fs::create_dir_all(&output_path)?;

    // Resolve the (runtime/language, mode) axes (#122) into a single canonical
    // internal target. Standalone artifacts (rust/api/…) pass through unchanged;
    // a runtime target (python/node/bun/browser) requires a mode and maps to the
    // matching generator.
    let target = resolve_target(&options.target, options.mode)?;
    ui::detail(&format!("output dir: {}", committed_path.display()));
    ui::detail(&format!("format version: {}", format_version));
    ui::detail(&format!("resolved target: {}", target));
    let mut generated_files = Vec::new();

    match target.as_str() {
        "all" => {
            // When config restricts which targets to emit, honour that list;
            // otherwise generate everything.
            let allowed = options.config_targets.as_deref();
            generated_files.extend(generate_all(
                &schema,
                &output_path,
                force,
                allowed,
                format_version,
                options.gen_config,
            )?);
        }
        "rust" => {
            let result =
                RustGenerator::generate_with_config(&schema, format_version, options.gen_config)
                    .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("database.rs");
            write_file(&path, &result.code, force)?;
            generated_files.push((path, result));
        }
        "typescript" => {
            let result = TypeScriptGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("types.ts");
            write_file(&path, &result.code, force)?;
            generated_files.push((path, result));
            write_ts_package_scaffold(&output_path)?;
        }
        "api" => {
            let result = ApiGenerator::generate_with_config(&schema, options.gen_config)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("api.rs");
            write_file(&path, &result.code, force)?;
            generated_files.push((path, result));
        }
        "openapi" => {
            let result = OpenApiGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("openapi.json");
            write_file(&path, &result.code, force)?;
            generated_files.push((path, result));
        }
        "stubs" => {
            let result = StubGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let stubs_dir = output_path.join("stubs");
            fs::create_dir_all(&stubs_dir)?;
            let path = stubs_dir.join("README.md");
            write_file(&path, &result.code, force)?;
            generated_files.push((path, result));
        }
        "wasm" => {
            generated_files.extend(generate_wasm_replica(
                &schema,
                &output_path,
                force,
                format_version,
                options.gen_config,
            )?);
        }
        "ffi" => {
            generated_files.extend(generate_ffi_engine(
                &schema,
                &output_path,
                force,
                format_version,
            )?);
        }
        // Per-runtime ergonomic wrappers (#51/#52/#117). Resolved here from
        // `python --runtime` / `node|bun --runtime`; the generators land in their
        // own phases.
        "pyo3" => {
            generated_files.extend(generate_pyo3_binding(
                &schema,
                &output_path,
                force,
                format_version,
            )?);
        }
        "napi" => {
            generated_files.extend(generate_napi_binding(
                &schema,
                &output_path,
                force,
                format_version,
            )?);
        }
        // Golang binding (RFC #203). Resolved from `go --runtime`;
        // rides the FFI cdylib over cgo — adds no new C symbol / substrate dep.
        "go" => {
            generated_files.extend(generate_go_binding(
                &schema,
                &output_path,
                force,
                format_version,
            )?);
        }
        // REST client SDKs (#118/#205/#206). Resolved from `python|go|rust --sdk`.
        // Transport clients over the generated REST API — no on-disk format, so
        // they take no `format_version`.
        "rust-sdk" => {
            generated_files.extend(generate_rust_sdk(&schema, &output_path, force)?);
        }
        "python-sdk" => {
            generated_files.extend(generate_python_sdk(&schema, &output_path, force)?);
        }
        "go-sdk" => {
            generated_files.extend(generate_go_sdk(&schema, &output_path, force)?);
        }
        _ => {
            return Err(CliError::Other(format!(
                "Unknown target: {}. Valid: standalone (all, rust, api, openapi, stubs, ffi, transform) \
                 or runtime × mode (python|node|bun|browser with --sdk/--runtime/--replica)",
                target
            )));
        }
    }

    // Check mode: compare each freshly generated artifact against what's
    // committed, then remove the scratch dir. Only generated artifacts are
    // compared — write-once user scaffolds (Cargo.toml, package.json, the static
    // worker bootstrap) are not in `generated_files`, so an edited scaffold never
    // trips the check.
    if options.check {
        let mut missing = Vec::new();
        let mut stale = Vec::new();
        for (scratch_file, code) in &generated_files {
            let rel = scratch_file
                .strip_prefix(&output_path)
                .unwrap_or(scratch_file);
            let committed = committed_path.join(rel);
            match fs::read_to_string(&committed) {
                Ok(existing) if existing == code.code => {}
                Ok(_) => stale.push(committed),
                Err(_) => missing.push(committed),
            }
        }
        let _ = fs::remove_dir_all(&output_path);

        if missing.is_empty() && stale.is_empty() {
            ui::success(&format!(
                "Generated code is up to date ({} artifact(s) checked).",
                generated_files.len()
            ));
            return Ok(());
        }

        println!();
        for path in &missing {
            ui::error(&format!("  missing: {}", path.display()));
        }
        for path in &stale {
            ui::error(&format!("  stale:   {}", path.display()));
        }
        println!();
        ui::error(&format!(
            "Generated code is out of date ({} missing, {} stale) — run `forgedb generate` to update.",
            missing.len(),
            stale.len()
        ));
        return Err(CliError::CodeGeneration(
            "generated code is out of date".to_string(),
        ));
    }

    // Report results
    ui::success(&format!("Generated {} files:", generated_files.len()));
    for (path, result) in &generated_files {
        ui::info(&format!(
            "  ✓ {} ({} lines) - {}",
            path.display(),
            result.line_count(),
            result.description
        ));
    }

    Ok(())
}

/// Generate all (or a filtered subset of) artifacts.
///
/// `target_filter` — when `Some`, only generates targets whose names appear in
/// the slice; `None` means generate everything.  Valid names: `rust`,
/// `typescript`, `api`, `openapi`, `stubs`.
fn generate_all(
    schema: &forgedb_parser::Schema,
    output_path: &PathBuf,
    force: bool,
    target_filter: Option<&[String]>,
    format_version: u32,
    gen_config: forgedb_codegen::GenConfig,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let enabled = |name: &str| -> bool {
        target_filter.map_or(true, |ts| ts.iter().any(|t| t.as_str() == name))
    };

    let mut files = Vec::new();

    // Generate Rust database code (with the #126 generate-time runtime config).
    if enabled("rust") {
        let rust_result = RustGenerator::generate_with_config(schema, format_version, gen_config)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let rust_path = output_path.join("database.rs");
        write_file(&rust_path, &rust_result.code, force)?;
        files.push((rust_path, rust_result));
    }

    // Generate TypeScript types
    if enabled("typescript") {
        let ts_result = TypeScriptGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let ts_path = output_path.join("types.ts");
        write_file(&ts_path, &ts_result.code, force)?;
        files.push((ts_path, ts_result));
        write_ts_package_scaffold(output_path)?;
    }

    // Generate API
    if enabled("api") {
        let api_result = ApiGenerator::generate_with_config(schema, gen_config)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let api_path = output_path.join("api.rs");
        write_file(&api_path, &api_result.code, force)?;
        files.push((api_path, api_result));
    }

    // Generate OpenAPI spec
    if enabled("openapi") {
        let openapi_result = OpenApiGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let openapi_path = output_path.join("openapi.json");
        write_file(&openapi_path, &openapi_result.code, force)?;
        files.push((openapi_path, openapi_result));
    }

    // Generate stubs
    if enabled("stubs") {
        let stub_result = StubGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let stubs_dir = output_path.join("stubs");
        fs::create_dir_all(&stubs_dir)?;
        let stub_path = stubs_dir.join("README.md");
        write_file(&stub_path, &stub_result.code, force)?;
        files.push((stub_path, stub_result));
    }

    // Generate the wasm browser read-replica crate.  This is an OPT-IN target:
    // the default `all` (no config filter) skips it, since it emits a whole
    // browser crate most projects don't need; a project enables it by listing
    // `wasm` in `[generate].targets`.
    if target_filter.is_some_and(|ts| ts.iter().any(|t| t.as_str() == "wasm")) {
        files.extend(generate_wasm_replica(
            schema,
            output_path,
            force,
            format_version,
            gen_config,
        )?);
    }

    // Generate the native FFI engine crate (the Layer-0 C-ABI spine every
    // language binding hangs off).  Also OPT-IN — the default `all` skips it; a
    // project enables it by listing `ffi` in `[generate].targets`.
    if target_filter.is_some_and(|ts| ts.iter().any(|t| t.as_str() == "ffi")) {
        files.extend(generate_ffi_engine(schema, output_path, force, format_version)?);
    }

    // REST client SDKs (#118/#205/#206) — also OPT-IN (the default `all` skips
    // them; a project enables one by listing `rust-sdk`/`python-sdk`/`go-sdk` in
    // `[generate].targets`).  Each emits a portable network client, no on-disk
    // format, so none take `format_version`.
    if target_filter.is_some_and(|ts| ts.iter().any(|t| t.as_str() == "rust-sdk")) {
        files.extend(generate_rust_sdk(schema, output_path, force)?);
    }
    if target_filter.is_some_and(|ts| ts.iter().any(|t| t.as_str() == "python-sdk")) {
        files.extend(generate_python_sdk(schema, output_path, force)?);
    }
    if target_filter.is_some_and(|ts| ts.iter().any(|t| t.as_str() == "go-sdk")) {
        files.extend(generate_go_sdk(schema, output_path, force)?);
    }

    Ok(files)
}

/// Generate the native FFI engine crate (language bindings #51/#52/#117): the
/// Layer-0 C-ABI spine (`ffi/src/ffi.rs`) over the SAME generated `database.rs`
/// (`ffi/src/database.rs`), plus a `Cargo.toml` scaffold written only when
/// absent.  Build it with `cargo build --release` to produce the `cdylib` a
/// Python/Node/Bun binding loads.
fn generate_ffi_engine(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
    format_version: u32,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let ffi_dir = output_path.join("ffi");
    let src_dir = ffi_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let mut files = Vec::new();

    // The Layer-0 C-ABI spine (lifecycle + error surface).  Emitted as the crate
    // root `lib.rs` so its `mod database;` resolves to `src/database.rs` (same
    // shape as the wasm replica's `lib.rs`).
    let ffi_result = FfiGenerator::generate(schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let lib_path = src_dir.join("lib.rs");
    write_file(&lib_path, &ffi_result.code, force)?;
    files.push((lib_path, ffi_result));

    // The generated database the spine wraps (same generator as the `rust`
    // target — the FFI engine is the same data logic, exposed over the C-ABI).
    let rust_result = RustGenerator::generate_with_format_version(schema, format_version)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let db_path = src_dir.join("database.rs");
    write_file(&db_path, &rust_result.code, force)?;
    files.push((db_path, rust_result));

    write_ffi_engine_scaffold(&ffi_dir)?;
    Ok(files)
}

/// Generate the PyO3 Python binding crate (#51): the `#[pyclass]` wrapper
/// (`pyo3/src/lib.rs`) over the SAME generated `database.rs`
/// (`pyo3/src/database.rs`), plus a `Cargo.toml` scaffold written only when
/// absent.  Build it with `maturin develop` / `maturin build` (or a plain
/// `cargo build` for the compile check) to produce the `forgedb` extension
/// module Python imports.
fn generate_pyo3_binding(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
    format_version: u32,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let pyo3_dir = output_path.join("pyo3");
    let src_dir = pyo3_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let mut files = Vec::new();

    // The PyO3 wrapper (crate root `lib.rs` so its `mod database;` resolves to
    // `src/database.rs`, same shape as the FFI engine / wasm replica).
    let py_result =
        PyO3Generator::generate(schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let lib_path = src_dir.join("lib.rs");
    write_file(&lib_path, &py_result.code, force)?;
    files.push((lib_path, py_result));

    // The generated database the wrapper binds (same generator as the `rust`
    // target — the binding is the same data logic, exposed to Python).
    let rust_result = RustGenerator::generate_with_format_version(schema, format_version)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let db_path = src_dir.join("database.rs");
    write_file(&db_path, &rust_result.code, force)?;
    files.push((db_path, rust_result));

    let cargo_path = pyo3_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        fs::write(
            &cargo_path,
            PyO3Generator::cargo_toml_scaffold("forgedb-python"),
        )?;
        ui::info(&format!(
            "  ✓ {} (PyO3 binding scaffold)",
            cargo_path.display()
        ));
    }

    // pyproject.toml so `maturin` can build a wheel out of the box. User-editable
    // — written only when absent.
    let pyproject_path = pyo3_dir.join("pyproject.toml");
    if !pyproject_path.exists() {
        fs::write(&pyproject_path, PYO3_PYPROJECT_SCAFFOLD)?;
        ui::info(&format!(
            "  ✓ {} (maturin pyproject)",
            pyproject_path.display()
        ));
    }

    Ok(files)
}

/// Generate the NAPI-RS Node/Bun binding crate (#52/#117): the `#[napi]` wrapper
/// (`napi/src/lib.rs`) over the SAME generated `database.rs`
/// (`napi/src/database.rs`), plus `Cargo.toml` / `build.rs` / `package.json`
/// scaffolds written only when absent.  Build it with `napi build` (or a plain
/// `cargo build` for the compile check) to produce the `forgedb` `.node` addon
/// both Node and Bun `require()` (Option A — one artifact for both runtimes).
fn generate_napi_binding(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
    format_version: u32,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let napi_dir = output_path.join("napi");
    let src_dir = napi_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let mut files = Vec::new();

    // The NAPI-RS wrapper (crate root `lib.rs` so its `mod database;` resolves to
    // `src/database.rs`, same shape as the FFI engine / PyO3 / wasm replica).
    let napi_result =
        NapiGenerator::generate(schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let lib_path = src_dir.join("lib.rs");
    write_file(&lib_path, &napi_result.code, force)?;
    files.push((lib_path, napi_result));

    // The generated database the wrapper binds (same generator as the `rust`
    // target — the binding is the same data logic, exposed to Node/Bun).
    let rust_result = RustGenerator::generate_with_format_version(schema, format_version)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let db_path = src_dir.join("database.rs");
    write_file(&db_path, &rust_result.code, force)?;
    files.push((db_path, rust_result));

    // User-editable config scaffolds — written only when absent (a regenerate,
    // even `--force` which overwrites the `.rs` files, never clobbers them).
    let cargo_path = napi_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        fs::write(&cargo_path, NapiGenerator::cargo_toml_scaffold("forgedb-node"))?;
        ui::info(&format!("  ✓ {} (NAPI-RS binding scaffold)", cargo_path.display()));
    }

    let build_path = napi_dir.join("build.rs");
    if !build_path.exists() {
        fs::write(&build_path, NapiGenerator::build_rs_scaffold())?;
        ui::info(&format!("  ✓ {} (napi-build script)", build_path.display()));
    }

    let package_path = napi_dir.join("package.json");
    if !package_path.exists() {
        fs::write(&package_path, NapiGenerator::package_json_scaffold())?;
        ui::info(&format!("  ✓ {} (@napi-rs/cli package)", package_path.display()));
    }

    Ok(files)
}

/// Generate the Golang binding (RFC #203): a per-schema Go cgo package
/// (`go/forgedb.go` + `go/forgedb.h`) that binds the SAME generated native FFI
/// C-ABI over cgo, alongside the FFI engine crate itself (`ffi/`, the `cdylib`
/// the Go package links). At NAPI-RS parity (CRUD, snapshot reads, relation
/// traversal, async CRUD, Arrow export). Rides the existing C-ABI unchanged —
/// adds no new C symbol and no new substrate dep. A `go.mod` scaffold is written
/// only when absent. Build order: `cargo build --release` in `ffi/`, then
/// `go build` in `go/`.
fn generate_go_binding(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
    format_version: u32,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    // The FFI engine crate (cdylib) the Go package links against — reused
    // verbatim, so Go requires no new C symbol.
    let mut files = generate_ffi_engine(schema, output_path, force, format_version)?;

    let go_dir = output_path.join("go");
    fs::create_dir_all(&go_dir)?;

    // The generated Go cgo package.
    let go_result =
        GoGenerator::generate(schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let go_path = go_dir.join("forgedb.go");
    write_file(&go_path, &go_result.code, force)?;
    files.push((go_path, go_result));

    // The async completion bridge (the `//export` callback — a separate file, per
    // cgo's rule that an `//export` file's preamble carries no C definitions).
    let async_result = GoGenerator::generate_async_bridge();
    let async_path = go_dir.join("forgedb_async.go");
    write_file(&async_path, &async_result.code, force)?;
    files.push((async_path, async_result));

    // Arrow columnar export (only when the schema has exportable columns) — the
    // ONE part of the Go binding that pulls an external module (arrow-go).
    let needs_arrow = GoGenerator::needs_arrow(schema);
    if let Some(arrow_result) = GoGenerator::generate_arrow(schema) {
        let arrow_path = go_dir.join("forgedb_arrow.go");
        write_file(&arrow_path, &arrow_result.code, force)?;
        files.push((arrow_path, arrow_result));
        ui::warning(
            "the Go Arrow export uses the external module `github.com/apache/arrow-go/v18` \
             (added to go.mod) — run `go mod tidy` in the `go/` dir before `go build`",
        );
    }

    // The C header cgo `#include`s (declares the `forgedb_*` prototypes).
    let header_result =
        GoGenerator::generate_header(schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let header_path = go_dir.join("forgedb.h");
    write_file(&header_path, &header_result.code, force)?;
    files.push((header_path, header_result));

    // User-editable `go.mod` — written only when absent (like every other
    // binding scaffold; a `--force` regenerate never clobbers it).
    let go_mod_path = go_dir.join("go.mod");
    if !go_mod_path.exists() {
        fs::write(
            &go_mod_path,
            GoGenerator::go_mod_scaffold("forgedb", needs_arrow),
        )?;
        ui::info(&format!("  ✓ {} (Go module scaffold)", go_mod_path.display()));
    }

    let readme_path = go_dir.join("README.md");
    if !readme_path.exists() {
        fs::write(&readme_path, GoGenerator::readme_scaffold())?;
        ui::info(&format!("  ✓ {} (Go binding README)", readme_path.display()));
    }

    Ok(files)
}

/// Generate the Rust REST client SDK crate (#206): a `reqwest`-based async client
/// (`rust-sdk/src/lib.rs`) over the generated REST API, plus a `Cargo.toml`
/// scaffold written only when absent. A transport client — links none of the
/// forgedb substrate crates, only `reqwest`/`serde`. Build with `cargo build` in
/// `rust-sdk/`.
fn generate_rust_sdk(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let sdk_dir = output_path.join("rust-sdk");
    let src_dir = sdk_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let mut files = Vec::new();
    let result =
        RustSdkGenerator::generate(schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let lib_path = src_dir.join("lib.rs");
    write_file(&lib_path, &result.code, force)?;
    files.push((lib_path, result));

    let cargo_path = sdk_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        fs::write(
            &cargo_path,
            RustSdkGenerator::cargo_toml_scaffold("forgedb-client"),
        )?;
        ui::info(&format!("  ✓ {} (Rust SDK scaffold)", cargo_path.display()));
    }
    Ok(files)
}

/// Generate the Python REST client SDK (#118): a stdlib-`urllib` client module
/// (`python-sdk/forgedb_client.py`) over the generated REST API, plus a
/// `pyproject.toml` scaffold written only when absent. Dependency-free. Install
/// with `pip install .` in `python-sdk/`.
fn generate_python_sdk(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let sdk_dir = output_path.join("python-sdk");
    fs::create_dir_all(&sdk_dir)?;

    let mut files = Vec::new();
    let result =
        PythonSdkGenerator::generate(schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let py_path = sdk_dir.join("forgedb_client.py");
    write_file(&py_path, &result.code, force)?;
    files.push((py_path, result));

    let pyproject_path = sdk_dir.join("pyproject.toml");
    if !pyproject_path.exists() {
        fs::write(&pyproject_path, PythonSdkGenerator::pyproject_scaffold())?;
        ui::info(&format!(
            "  ✓ {} (Python SDK pyproject)",
            pyproject_path.display()
        ));
    }
    Ok(files)
}

/// Generate the Go REST client SDK (#205): a pure-stdlib `net/http` client
/// package (`go-sdk/client.go`) over the generated REST API, plus `go.mod` /
/// `README.md` scaffolds written only when absent. No cgo, no external module.
/// Build with `go build` in `go-sdk/`.
fn generate_go_sdk(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let sdk_dir = output_path.join("go-sdk");
    fs::create_dir_all(&sdk_dir)?;

    let mut files = Vec::new();
    let result =
        GoSdkGenerator::generate(schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let go_path = sdk_dir.join("client.go");
    write_file(&go_path, &result.code, force)?;
    files.push((go_path, result));

    let mod_path = sdk_dir.join("go.mod");
    if !mod_path.exists() {
        fs::write(&mod_path, GoSdkGenerator::go_mod_scaffold("forgedb-client"))?;
        ui::info(&format!("  ✓ {} (Go SDK module scaffold)", mod_path.display()));
    }

    let readme_path = sdk_dir.join("README.md");
    if !readme_path.exists() {
        fs::write(&readme_path, GoSdkGenerator::readme_scaffold())?;
        ui::info(&format!("  ✓ {} (Go SDK README)", readme_path.display()));
    }
    Ok(files)
}

/// The `maturin` build config for the generated PyO3 binding.
const PYO3_PYPROJECT_SCAFFOLD: &str = r#"[build-system]
requires = ["maturin>=1.5,<2.0"]
build-backend = "maturin"

[project]
name = "forgedb"
version = "0.1.0"
requires-python = ">=3.8"
description = "Generated ForgeDB Python binding"

[tool.maturin]
features = ["pyo3/extension-module"]
"#;

/// Write the FFI engine crate's `Cargo.toml`.  User-editable config, so it is
/// written ONLY when absent — a regenerate (even `--force`, which overwrites the
/// `.rs` files) never clobbers it, mirroring the wasm replica scaffold.
fn write_ffi_engine_scaffold(ffi_dir: &Path) -> Result<()> {
    let path = ffi_dir.join("Cargo.toml");
    if !path.exists() {
        fs::write(&path, FfiGenerator::cargo_toml_scaffold("forgedb-ffi-engine"))?;
        ui::info(&format!("  ✓ {} (native FFI engine scaffold)", path.display()));
    }
    Ok(())
}

/// Generate the `wasm32` browser read-replica crate (#110 Milestone C): the
/// `#[wasm_bindgen]` transport (`replica/src/lib.rs`) over the SAME generated
/// `database.rs` (`replica/src/database.rs`), plus a `Cargo.toml` scaffold
/// written only when absent.  Build it with `wasm-pack build --target web`.
fn generate_wasm_replica(
    schema: &forgedb_parser::Schema,
    output_path: &Path,
    force: bool,
    format_version: u32,
    gen_config: forgedb_codegen::GenConfig,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let replica_dir = output_path.join("replica");
    let src_dir = replica_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let mut files = Vec::new();

    // The wasm-bindgen transport glue.
    let wasm_result = WasmGenerator::generate(schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let lib_path = src_dir.join("lib.rs");
    write_file(&lib_path, &wasm_result.code, force)?;
    files.push((lib_path, wasm_result));

    // The generated database the transport compiles against (same generator as
    // the `rust` target — the follower is the same data logic, recompiled).  It
    // shares the server's lineage version so the replica's `EXPECTED_FORMAT_VERSION`
    // matches the data it follows.
    let rust_result = RustGenerator::generate_with_format_version(schema, format_version)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let db_path = src_dir.join("database.rs");
    write_file(&db_path, &rust_result.code, force)?;
    files.push((db_path, rust_result));

    // The main-thread async client (#110 #2, WS3): a per-schema TS `ReplicaClient`
    // that RPCs into the Worker running the engine — mirrors the transport's read
    // surface exactly, invents nothing.
    let client_dir = replica_dir.join("client");
    fs::create_dir_all(&client_dir)?;
    let client_result = WasmGenerator::generate_client(schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let client_path = client_dir.join("replica-client.ts");
    write_file(&client_path, &client_result.code, force)?;
    files.push((client_path, client_result));

    // The STATIC, schema-agnostic Worker bootstrap (WS2). It runs the engine,
    // follows `/replicate`, and debounces auto-commit. Emitted verbatim (NOT from
    // a schema-aware path) so it cannot become schema-aware — the PM constraint.
    let worker_path = client_dir.join("replica-worker.js");
    write_file(
        &worker_path,
        &WasmGenerator::worker_bootstrap_with_config(gen_config),
        force,
    )?;
    ui::info(&format!("  ✓ {} (static worker bootstrap)", worker_path.display()));

    write_wasm_replica_scaffold(&replica_dir)?;
    Ok(files)
}

/// Write the wasm replica crate's `Cargo.toml`.  User-editable config, so it is
/// written ONLY when absent — a regenerate (even `--force`, which overwrites the
/// `.rs` files) never clobbers it, mirroring the TS SDK's `package.json`.
fn write_wasm_replica_scaffold(replica_dir: &Path) -> Result<()> {
    let path = replica_dir.join("Cargo.toml");
    if !path.exists() {
        fs::write(&path, WasmGenerator::cargo_toml_scaffold("forgedb-replica"))?;
        ui::info(&format!("  ✓ {} (wasm replica scaffold)", path.display()));
    }
    Ok(())
}

/// Write the npm packaging scaffold for the generated TypeScript SDK (Phase 5
/// WS5): `package.json` + `tsconfig.json` alongside `types.ts`.  These are
/// user-editable config, so they are written ONLY when absent — a regenerate
/// (even `--force`, which overwrites `types.ts`) never clobbers them.
fn write_ts_package_scaffold(output_path: &Path) -> Result<()> {
    let files = [
        ("package.json", TypeScriptGenerator::package_json_scaffold()),
        ("tsconfig.json", TypeScriptGenerator::tsconfig_scaffold()),
    ];
    for (name, content) in files {
        let path = output_path.join(name);
        if !path.exists() {
            fs::write(&path, content)?;
            ui::info(&format!("  ✓ {} (npm SDK scaffold)", path.display()));
        }
    }
    Ok(())
}

fn write_file(path: &PathBuf, content: &str, force: bool) -> Result<()> {
    // Check if file exists and we're not forcing
    if path.exists() && !force {
        return Err(CliError::Other(format!(
            "File exists: {}. Use --force to overwrite",
            path.display()
        )));
    }

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write file
    fs::write(path, content)?;

    Ok(())
}

/// Resolve the `generate` CLI's two axes (#122) into one canonical internal
/// target string the dispatcher understands.
///
/// - **Standalone artifacts** (`all`, `rust`, `api`, `openapi`, `stubs`, `ffi`,
///   `transform`) take **no** mode; passing one is an error.
/// - **Runtime targets** (`python`, `node`, `bun`, `browser`) **require** a mode
///   and map `(runtime, mode)` to the matching generator.
/// - The pre-#122 flat verbs `typescript`/`wasm` are a **clean break**: they
///   error with a pointer to the new form (this CLI is pre-1.0; see docs/SEMVER).
fn resolve_target(raw: &str, mode: Option<GenerateMode>) -> Result<String> {
    let target = raw.to_lowercase();

    // `rust` is dual-axis: the standalone database core (no mode) OR the REST
    // client SDK crate (`--sdk`, #206). It is the one runtime with no in-process
    // FFI binding to be a client *of* — it *is* the generated core — so `--sdk` is
    // its only mode.
    if target == "rust" {
        return match mode {
            None => Ok("rust".to_string()),
            Some(GenerateMode::Sdk) => Ok("rust-sdk".to_string()),
            Some(m) => Err(CliError::Other(format!(
                "`generate rust {}` is not supported — `generate rust` emits the database core, \
                 `generate rust --sdk` emits the REST client crate",
                m.flag()
            ))),
        };
    }

    // Standalone artifacts — the mode axis does not apply.
    const STANDALONE: &[&str] = &["all", "api", "openapi", "stubs", "ffi", "transform"];
    if STANDALONE.contains(&target.as_str()) {
        if let Some(m) = mode {
            return Err(CliError::Other(format!(
                "`generate {target}` is a standalone artifact and takes no mode flag ({} was given)",
                m.flag()
            )));
        }
        return Ok(target);
    }

    // Retired pre-#122 verbs — redirect to the runtime × mode form.
    match target.as_str() {
        "typescript" => {
            return Err(CliError::Other(
                "`generate typescript` was renamed — use `generate node --sdk` or `generate bun --sdk`"
                    .to_string(),
            ));
        }
        "wasm" => {
            return Err(CliError::Other(
                "`generate wasm` was renamed — use `generate browser --replica`".to_string(),
            ));
        }
        _ => {}
    }

    // Runtime targets — a mode is mandatory.
    let runtimes = ["python", "node", "bun", "browser", "go"];
    if !runtimes.contains(&target.as_str()) {
        return Err(CliError::Other(format!(
            "Unknown target: {target}. Valid: standalone (all, rust, api, openapi, stubs, ffi, transform) \
             or a runtime (python, node, bun, browser) with --sdk/--runtime/--replica"
        )));
    }
    let mode = mode.ok_or_else(|| {
        CliError::Other(format!(
            "`generate {target}` needs a mode — one of --sdk, --runtime, --replica"
        ))
    })?;

    // Map (runtime, mode) to the canonical internal target.
    match (target.as_str(), mode) {
        // Node/Bun REST SDK — the decomposed `generate typescript`.
        ("node", GenerateMode::Sdk) | ("bun", GenerateMode::Sdk) => Ok("typescript".to_string()),
        // Node/Bun native binding — one shared NAPI-RS `.node` (Option A).
        ("node", GenerateMode::Runtime) | ("bun", GenerateMode::Runtime) => Ok("napi".to_string()),
        // Python native binding — PyO3.
        ("python", GenerateMode::Runtime) => Ok("pyo3".to_string()),
        // Go native binding — cgo over the FFI cdylib (RFC #203).
        ("go", GenerateMode::Runtime) => Ok("go".to_string()),
        // REST client SDKs (#118/#205) — network clients over the generated API.
        ("python", GenerateMode::Sdk) => Ok("python-sdk".to_string()),
        ("go", GenerateMode::Sdk) => Ok("go-sdk".to_string()),
        // Browser read-replica follower — the decomposed `generate wasm`.
        ("browser", GenerateMode::Replica) => Ok("wasm".to_string()),

        // Recognised-but-not-in-this-milestone combinations.
        ("node", GenerateMode::Replica) | ("bun", GenerateMode::Replica) => Err(CliError::Other(
            "`generate node|bun --replica` (server-side WASM replica, #121) is not yet implemented"
                .to_string(),
        )),
        (rt, m) => Err(CliError::Other(format!(
            "`generate {rt} {}` is not a supported runtime × mode combination",
            m.flag()
        ))),
    }
}

fn find_schema_file() -> Result<String> {
    // Look for common schema file names
    let candidates = ["schema.forge", "schema.lang", "schema.forgedb"];

    for candidate in &candidates {
        if Path::new(candidate).exists() {
            return Ok(candidate.to_string());
        }
    }

    Err(CliError::SchemaNotFound(
        "No schema file found. Expected one of: schema.forge, schema.lang, schema.forgedb"
            .to_string(),
    ))
}

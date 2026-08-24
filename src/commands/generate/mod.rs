use crate::{error::CliError, ui, Result};
use forgedb_codegen::{
    ApiGenerator, FfiGenerator, GoGenerator, GoSdkGenerator, NapiGenerator, OpenApiGenerator,
    PyO3Generator, PythonSdkGenerator, RustGenerator, RustSdkGenerator, StubGenerator,
    TypeScriptGenerator, WasmGenerator,
};
use forgedb_parser::Parser;
use std::fs;
use std::path::{Path, PathBuf};

mod in_tree;

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
    /// Resolved schema file path (from CLI `--schema`, else found beside the
    /// caller). No config participates — `[generate].schema` was removed in #333.
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
    /// The app's container in the build cache, reserved by the caller BEFORE
    /// this runs (#335 §1/§3).
    ///
    /// `None` means "do not write cache packages" — the `check` mode and any
    /// caller that has not reserved one. The caller re-derives the workspace
    /// root AFTER this returns, because a root rendered before emission lists
    /// the previous run's packages.
    pub cache_container: Option<PathBuf>,
    /// Where the in-tree Rust package goes (#338), already resolved against the
    /// **schema's** directory by `Governing::rust_package`.
    ///
    /// `None` means `[placement].rust_package` is absent — which is the opt-out,
    /// and the only opt-out there is. Nothing is emitted and nothing changes.
    pub in_tree: Option<PathBuf>,
}

/// Every app-derived name one `generate` invocation builds under (#335 §2).
///
/// Computed ONCE in [`run`] and threaded, never re-derived. The FFI symbol
/// prefix baked into `ffi/src/lib.rs`, the prototypes declared in `forgedb.h`,
/// the `C.` calls in `forgedb.go` and the cache package names must all agree,
/// and every extra derivation is a way for them to stop agreeing *silently* —
/// a Go package that links against a symbol set nothing exports.
#[derive(Debug, Clone)]
struct AppNaming {
    /// The app's legible derived identity, e.g. `foo_services_blog`.
    app_name: String,
    /// `<app_name>_` — the per-app prefix on every exported C symbol.
    symbol_prefix: String,
}

impl AppNaming {
    /// The hash stand-in when no cache container was reserved.
    ///
    /// A `None` container means this invocation writes nothing into the cache,
    /// so nothing derived here can collide with another app — but the names
    /// still have to be *legal*, and an empty hash renders `blog--ffi`.
    /// [`crate::cache::member_hash`] is deliberately NOT recomputed here: it is
    /// keyed on the PROJECT-RELATIVE schema path, which this function does not
    /// have, so a second derivation would disagree with the cache's without
    /// saying so.
    const NO_CONTAINER: &'static str = "local";

    fn for_run(container: Option<&Path>, schema_path: &str) -> AppNaming {
        // Read, never re-derive. The name is a function of the project's whole
        // app set (`naming::app_name`), which this function cannot see — it has
        // one schema path and no project root. `cache::reserve` computed it with
        // all three inputs in hand and wrote it into the container.
        let app_name = container
            .and_then(crate::cache::member_app_name)
            .unwrap_or_else(|| {
                // No container means this invocation writes nothing into the
                // cache, so nothing derived here can collide with another app —
                // but the names still have to be legal. Fall back to the app's
                // own path segments with no project id and no siblings.
                let local = crate::naming::app_name(
                    Self::NO_CONTAINER,
                    Path::new(schema_path),
                    &[],
                    crate::naming::SymbolNaming::Minimal,
                );
                local
            });
        let symbol_prefix = crate::naming::symbol_prefix(&app_name);
        AppNaming {
            app_name,
            symbol_prefix,
        }
    }

    fn package(&self, kind: &crate::naming::PackageKind) -> String {
        crate::naming::package_name(&self.app_name, kind)
    }
}

/// The invocation-wide inputs every emitter arm reads.
///
/// Grouped into one value because the arms took eight positional parameters
/// otherwise, and eight positional parameters of which three are `bool`/`u32`
/// is a call site that can be reordered wrongly and still compile.
struct Emit<'a> {
    schema: &'a forgedb_parser::Schema,
    /// Where `output`-placed artifacts go. In `--check` mode this is a scratch
    /// directory, never the committed one.
    output: &'a Path,
    force: bool,
    schema_version: u32,
    gen_config: forgedb_codegen::GenConfig,
    naming: &'a AppNaming,
}

/// Everything one `generate` invocation will write into the app's build-cache
/// container (#335 §1/§6).
///
/// This replaces the previous lookup, which scanned the emitted-file list for
/// OUTPUT-relative paths (`ffi/src/lib.rs`, `napi/src/database.rs`, …). After
/// the placement flip those paths are never written, so there is no key left to
/// scan for — and the replacement is better than a renamed key would have been:
/// each field is set by exactly one emitter, so "one app, one `database.rs`"
/// stops being a property five call sites are trusted to honour and becomes a
/// single `Option` that is filled once.
#[derive(Default)]
struct CacheEmission {
    /// The exact bytes `core/src/lib.rs` receives: the generated database plus
    /// [`CORE_SUBSTRATE_REEXPORTS`]. The SAME `String` is written to
    /// `<output>/database.rs` as the mirror, which is what makes the two
    /// byte-identical by construction rather than by assertion.
    core_lib: Option<String>,
    /// `server/src/api.rs`, and the `<output>/api.rs` mirror.
    api: Option<String>,
    /// The `core` package as it will be written — manifest included.
    core: Option<PackagePlan>,
    /// The `server` package as it will be written.
    server: Option<PackagePlan>,
    /// Each wrapper package this invocation planned. An absent kind means this
    /// invocation did not emit it, and the cache writer skips it.
    wrappers: Vec<PackagePlan>,
    /// Memoized fingerprints, keyed by package directory. **One derivation**:
    /// the Go arm needs the `ffi` value mid-emission and the shim pass needs it
    /// again afterwards, and a second computation is a second thing that can
    /// disagree.
    fingerprints: std::collections::BTreeMap<String, String>,
    /// Was `ffi` DECLARED, or merely pulled in by the `go` arm?
    ///
    /// The two are different projects: a Go-only project has no `<output>/ffi/`
    /// and must not grow one, because a directory that exists is a directory
    /// delivery delivers into.
    ffi_declared: bool,
}

/// One cache package as it will be written: package-relative path -> contents.
///
/// **Rendered before anything is written**, which is the whole reason this type
/// exists. The fingerprint has to cover the exact bytes that land — manifests
/// included, since a substrate pin change alters the compiled artifact while
/// leaving every `.rs` byte identical — and the shims that carry the value are
/// written into `output` during the same invocation. Hashing a directory scan
/// afterwards is the ordering trap #335 hit once already (`place()` had to split
/// into `reserve` + `sync_root` because rendering from a scan described the
/// PREVIOUS run).
struct PackagePlan {
    kind: crate::naming::PackageKind,
    files: Vec<(String, String)>,
}

impl PackagePlan {
    fn new(kind: crate::naming::PackageKind, files: Vec<(String, String)>) -> PackagePlan {
        PackagePlan { kind, files }
    }

    /// This package's hash entries, namespaced by its directory.
    ///
    /// The prefix is what keeps `core/src/lib.rs` and `napi/src/lib.rs` distinct
    /// in an input that holds both.
    fn entries(&self) -> Vec<crate::fingerprint::Entry<'_>> {
        let dir = self.kind.dir();
        self.files
            .iter()
            .map(|(rel, body)| crate::fingerprint::Entry {
                path: format!("{dir}/{rel}"),
                bytes: body.as_str(),
            })
            .collect()
    }

    fn push(&mut self, rel: &str, body: String) {
        self.files.push((rel.to_string(), body));
    }
}

/// Why the FFI engine is being emitted.
///
/// A plain `bool` here would be a parameter a call site can compute a second,
/// divergent condition into — the shape #445 deleted from `CorePackage::cargo_toml`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FfiReason {
    /// `generate ffi`, or `ffi` in `[generate].targets`. The user asked for a C
    /// binding, so `<output>/ffi/` gets a header.
    Declared,
    /// The `go` arm needs the same engine package. Nothing lands in
    /// `<output>/ffi/`.
    ForGo,
}

impl CacheEmission {
    fn wrapper(&self, kind: &crate::naming::PackageKind) -> Option<&PackagePlan> {
        self.wrappers.iter().find(|p| &p.kind == kind)
    }

    fn wrapper_mut(&mut self, kind: &crate::naming::PackageKind) -> Option<&mut PackagePlan> {
        self.wrappers.iter_mut().find(|p| &p.kind == kind)
    }

    /// Plan one binding wrapper: manifest, `src/lib.rs`, and whatever build-time
    /// files that wrapper needs beside them.
    fn plan_wrapper(
        &mut self,
        kind: crate::naming::PackageKind,
        manifest: String,
        lib_rs: String,
        extra: &[(&str, String)],
    ) {
        let mut files = vec![
            ("Cargo.toml".to_string(), manifest),
            ("src/lib.rs".to_string(), lib_rs),
        ];
        for (name, body) in extra {
            files.push(((*name).to_string(), body.clone()));
        }
        self.wrappers.push(PackagePlan::new(kind, files));
    }

    /// The fingerprint of one wrapper package: the app's `core` plus that
    /// package's own directory, `src/fingerprint.rs` excluded by name.
    ///
    /// `None` when either half has not been planned — a `--sdk`-only run emits
    /// no `core`, and there is then nothing to fingerprint.
    fn fingerprint(&mut self, kind: &crate::naming::PackageKind) -> Option<String> {
        let dir = kind.dir();
        if let Some(value) = self.fingerprints.get(&dir) {
            return Some(value.clone());
        }
        let value = {
            let core = self.core.as_ref()?;
            let pkg = self.wrapper(kind)?;
            let mut entries = core.entries();
            entries.extend(pkg.entries());
            crate::fingerprint::compute(&entries)
        };
        self.fingerprints.insert(dir, value.clone());
        Some(value)
    }
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

    // Surface any non-fatal diagnostics the parse collected (#237). `generate` is
    // the most-run command, so it is where a deprecation most needs to be seen —
    // and before this it was the one path with nowhere to put a warning at all.
    //
    // Note this reads the warnings off the *fail-fast* `parse` above rather than
    // switching to `parse_recover`: recovering here would change generation's error
    // semantics from abort-on-first-error to recover-and-continue, which is a far
    // larger behavioral change than any deprecation needs.
    //
    // Warnings never gate generation. The error count is ignored here on purpose —
    // `parse` already returned `Err` for anything fatal — and only warnings can
    // reach this point.
    let _ = crate::diagnostics::report(&parser.take_warnings());

    ui::success(&format!(
        "Parsed schema ({} models, {} total fields)",
        schema.models.len(),
        schema.models.iter().map(|m| m.fields.len()).sum::<usize>()
    ));

    // On-disk format version (#74 Phase 1/2): derive it from the committed
    // migration lineage under `migrations/` and bake it into the generated app's
    // `EXPECTED_SCHEMA_VERSION` (red line #8 — lineage-sourced, never hand-edited).
    // Baseline `1` when there is no lineage (a project that has authored no
    // migrations yet).  The open guard compares this opaque integer and refuses a
    // stale data dir; it is threaded into every `database.rs` emission (the server
    // and the wasm replica share one lineage).
    // #437: resolved from the SCHEMA's directory, never the CWD. The bare relative
    // string this used to pass read whatever `migrations/` the current directory had —
    // so generating from a repo root baked baseline 1, and generating app B from app A's
    // directory baked A's lineage into B. Both compile, both emit a number, and the
    // interlock silently stops guarding.
    let lineage_dir = crate::project::migrations_dir(Path::new(&schema_path));
    let schema_version = forgedb_migrations::current_schema_version(&lineage_dir);

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

    // #338 C1/C8: a placement inside the build cache is refused BEFORE anything
    // is written — before the output directory is created, before the mirror.
    // A refusal that fires after the mirror lands has already done the damage it
    // exists to prevent, and "nothing was written" is the half of the scenario a
    // guard placed at the emitter would silently fail.
    if let Some(dir) = options.in_tree.as_deref() {
        in_tree::guard(dir)?;
    }

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
    ui::detail(&format!("schema version: {}", schema_version));
    ui::detail(&format!("resolved target: {}", target));

    // Every derived name this invocation builds under (#335 §2), computed once
    // and threaded from here. The symbol prefix in particular is load-bearing:
    // `ffi.rs`, `forgedb.h` and `forgedb.go` are three emitters of the SAME
    // symbol set, and they agree only because all three read this one value.
    let naming = AppNaming::for_run(options.cache_container.as_deref(), &schema_path);

    // The invocation-wide inputs every emitter arm reads (#335 §6). Grouped so
    // an arm takes three parameters instead of eight, and so adding an input
    // cannot silently reorder an existing call site's arguments.
    let ctx = Emit {
        schema: &schema,
        output: &output_path,
        force,
        schema_version,
        gen_config: options.gen_config,
        naming: &naming,
    };

    // Everything this invocation hands the cache. After the flip (#335 §6)
    // `output` never receives `ffi/`, `napi/`, `pyo3/` or `replica/`'s crate, so
    // there is no output-relative path left for the cache emitter to key on —
    // the wrapper bodies reach it through here instead.
    let mut cache = CacheEmission::default();
    let mut generated_files = Vec::new();

    match target.as_str() {
        "all" => {
            // When config restricts which targets to emit, honour that list;
            // otherwise generate everything.
            let allowed = options.config_targets.clone();
            generate_all(&ctx, allowed.as_deref(), &mut cache, &mut generated_files)?;
        }
        "rust" => {
            ensure_database(&ctx, &mut cache, &mut generated_files)?;
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
            emit_api(&ctx, &mut cache, &mut generated_files)?;
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
            generate_wasm_replica(&ctx, &mut cache, &mut generated_files)?;
        }
        "ffi" => {
            generate_ffi_engine(&ctx, &mut cache, &mut generated_files, FfiReason::Declared)?;
        }
        // Per-runtime ergonomic wrappers (#51/#52/#117). Resolved here from
        // `python --runtime` / `node|bun --runtime`; the generators land in their
        // own phases.
        "pyo3" => {
            generate_pyo3_binding(&ctx, &mut cache, &mut generated_files)?;
        }
        "napi" => {
            generate_napi_binding(&ctx, &mut cache, &mut generated_files)?;
        }
        // Golang binding (RFC #203). Resolved from `go --runtime`; rides the FFI
        // engine's `staticlib` over cgo — adds no new C symbol / substrate dep.
        "go" => {
            generate_go_binding(&ctx, &mut cache, &mut generated_files)?;
        }
        // REST client SDKs (#118/#205/#206). Resolved from `python|go|rust --sdk`.
        // Transport clients over the generated REST API — no on-disk format, so
        // they take no `schema_version`.
        "rust-sdk" => {
            generate_rust_sdk(&ctx, &mut generated_files)?;
        }
        "python-sdk" => {
            generate_python_sdk(&ctx, &mut generated_files)?;
        }
        "go-sdk" => {
            generate_go_sdk(&ctx, &mut generated_files)?;
        }
        _ => {
            return Err(CliError::Other(format!(
                "Unknown target: {}. Valid: standalone (all, rust, api, openapi, stubs, ffi, transform) \
                 or runtime × mode (python|node|bun|browser with --sdk/--runtime/--replica)",
                target
            )));
        }
    }

    // The consumer-facing half of every delivered target (#337), as a POST-PASS.
    //
    // It has to run after every arm, not inside one: a shim carries the
    // fingerprint of its package, the fingerprint covers the rendered manifest,
    // and a manifest is not rendered until the arm that plans the package has
    // run. Writing a shim from inside an arm would hash a package that is not
    // finished being planned.
    //
    // It runs BEFORE the check-mode comparison below, so `generate --check`
    // covers the shims — they are committed generated text like `types.ts`.
    emit_consumer_shims(&ctx, &mut cache, &mut generated_files)?;

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
        // The in-tree package is committed source too, so `--check` — CI's
        // staleness gate — has to cover it. Compared in memory against the live
        // location: it never gets a scratch path, because a placement may sit
        // outside the output directory and the scratch-relative join above would
        // then resolve back to the real one.
        if let (Some(dir), Some(core_lib)) = (options.in_tree.as_deref(), cache.core_lib.as_deref())
        {
            let (m, s) = in_tree::check(
                dir,
                &naming.package(&crate::naming::PackageKind::Core),
                &ctx.gen_config,
                core_lib,
            )?;
            missing.extend(m);
            stale.extend(s);
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

    // The supersession rule (#335 §6). It runs on every real generate, and it is
    // about the files this invocation deliberately did NOT write: `output` no
    // longer receives the `ffi`, `napi`, `pyo3` or `replica` crates, and a
    // frozen-but-compilable copy of one is a build that keeps going green
    // against a database that stopped tracking the schema.
    supersede_moved_packages(&committed_path)?;

    // The cache packages (#335 §1). `core/src/lib.rs` gets the SAME `String`
    // `<output>/database.rs` got — one value, two writes — never a second
    // generator invocation.
    if let Some(container) = &options.cache_container {
        emit_cache_packages(container, &cache)?;
    }

    // The in-tree placement (#338). A SECOND DESTINATION for the package the
    // cache emitter just wrote, never a second generator: both read
    // `CorePackage::files` over the same memoized `core_lib`, so the two copies
    // are byte-identical by construction.
    //
    // Keyed on `options.in_tree` alone, independent of whether a cache container
    // was reserved: the two placements answer different questions and a project
    // may want either, both, or neither.
    if let (Some(dir), Some(core_lib)) = (options.in_tree.as_deref(), cache.core_lib.as_deref()) {
        in_tree::emit(
            dir,
            &naming.package(&crate::naming::PackageKind::Core),
            &ctx.gen_config,
            core_lib,
        )?;
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
/// the slice; `None` means generate everything.
fn generate_all(
    ctx: &Emit<'_>,
    target_filter: Option<&[String]>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    // A default-on target: emitted unless config narrows the list.
    let enabled = |name: &str| -> bool {
        target_filter.is_none_or(|ts| ts.iter().any(|t| t.as_str() == name))
    };
    // An OPT-IN target: the default `all` (no config filter) skips it, because
    // each emits a whole extra package most projects do not want. A project
    // turns one on by naming it in `[generate].targets`.
    let opt_in = |name: &str| -> bool {
        target_filter.is_some_and(|ts| ts.iter().any(|t| t.as_str() == name))
    };

    // The app's database (with the #126 generate-time runtime config).
    if enabled("rust") {
        ensure_database(ctx, cache, files)?;
    }

    // Generate TypeScript types
    if enabled("typescript") {
        let ts_result = TypeScriptGenerator::generate(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let ts_path = ctx.output.join("types.ts");
        write_file(&ts_path, &ts_result.code, ctx.force)?;
        files.push((ts_path, ts_result));
        write_ts_package_scaffold(ctx.output)?;
    }

    // Generate API
    if enabled("api") {
        emit_api(ctx, cache, files)?;
    }

    // Generate OpenAPI spec
    if enabled("openapi") {
        let openapi_result = OpenApiGenerator::generate(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let openapi_path = ctx.output.join("openapi.json");
        write_file(&openapi_path, &openapi_result.code, ctx.force)?;
        files.push((openapi_path, openapi_result));
    }

    // Generate stubs
    if enabled("stubs") {
        let stub_result = StubGenerator::generate(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let stubs_dir = ctx.output.join("stubs");
        fs::create_dir_all(&stubs_dir)?;
        let stub_path = stubs_dir.join("README.md");
        write_file(&stub_path, &stub_result.code, ctx.force)?;
        files.push((stub_path, stub_result));
    }

    // The wasm browser read-replica.
    if opt_in("wasm") {
        generate_wasm_replica(ctx, cache, files)?;
    }

    // The native FFI engine (the Layer-0 C-ABI spine every language binding
    // hangs off, and the `staticlib` the Go binding links).
    if opt_in("ffi") {
        generate_ffi_engine(ctx, cache, files, FfiReason::Declared)?;
    }

    // REST client SDKs (#118/#205/#206). Each emits a portable network client,
    // no on-disk format, so none take `schema_version`.
    if opt_in("rust-sdk") {
        generate_rust_sdk(ctx, files)?;
    }
    if opt_in("python-sdk") {
        generate_python_sdk(ctx, files)?;
    }
    if opt_in("go-sdk") {
        generate_go_sdk(ctx, files)?;
    }

    // The three native runtime bindings.  These had NO arm here at all until
    // #335 §12: they were reachable only through a single-target CLI invocation
    // (`generate node --runtime`, `generate python --runtime`, `generate go
    // --runtime`), so `[generate].targets` could name them and `generate all`
    // would still emit nothing.  Decision 10 gives them config spellings, which
    // is only meaningful if `all` can actually reach them.
    if opt_in("napi") {
        generate_napi_binding(ctx, cache, files)?;
    }
    if opt_in("pyo3") {
        generate_pyo3_binding(ctx, cache, files)?;
    }
    if opt_in("go") {
        // `generate_go_binding` emits the FFI engine itself and is idempotent, so
        // `targets = ["ffi", "go"]` reaches it twice and still emits one engine.
        generate_go_binding(ctx, cache, files)?;
    }

    Ok(())
}

/// Generate the app's ONE database, write the `output/database.rs` mirror, and
/// hand the cache the exact bytes `core/src/lib.rs` receives — **at most once
/// per invocation**.
///
/// # Why this is memoized rather than called per arm
///
/// Five arms need a database: `rust` and the four binding wrappers. Until #335
/// each of the five called [`RustGenerator`] itself, and only the `rust` arm
/// threaded the app's [`forgedb_codegen::GenConfig`] — so a single `generate`
/// run wrote **two databases with different durability semantics** and nothing
/// said so. Routing all five through one memoized call makes "one app, one
/// database" a property of the code rather than of five call sites that are each
/// expected to pass the same arguments.
///
/// # One value, two writes
///
/// `core_lib` is built once. `<output>/database.rs` and `core/src/lib.rs`
/// receive that same `String`; nothing recomputes it, which is what makes the
/// mirror structurally incapable of drifting from the copy ForgeDB compiles.
fn ensure_database(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    if cache.core_lib.is_some() {
        return Ok(());
    }

    let result = RustGenerator::generate_with_config(ctx.schema, ctx.schema_version, ctx.gen_config)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let core_lib = format!("{}{}", result.code, CORE_SUBSTRATE_REEXPORTS);

    let path = ctx.output.join("database.rs");
    write_file(&path, &core_lib, ctx.force)?;
    files.push((
        path,
        forgedb_codegen::GeneratedCode {
            code: core_lib.clone(),
            description: result.description,
        },
    ));
    // The `core` package as it will be written, manifest included — rendered
    // HERE so the fingerprint sees the bytes that land. The manifest comes from
    // the SAME `GenConfig` that rendered `core_lib` (#445), so its `utoipa` pin
    // and the source's `use utoipa::ToSchema;` cannot disagree.
    cache.core = Some(PackagePlan::new(
        crate::naming::PackageKind::Core,
        forgedb_codegen::CorePackage::files(
            &ctx.naming.package(&crate::naming::PackageKind::Core),
            &ctx.gen_config,
            &core_lib,
        )
        .into_iter()
        .map(|(rel, body)| (rel.to_string(), body))
        .collect(),
    ));
    cache.core_lib = Some(core_lib);
    Ok(())
}

/// Emit the REST API layer: the `<output>/api.rs` mirror and the bytes
/// `server/src/api.rs` receives — one value, two writes (#335 §6).
fn emit_api(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    if cache.api.is_some() {
        return Ok(());
    }
    let result = ApiGenerator::generate_with_config(ctx.schema, ctx.gen_config)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let path = ctx.output.join("api.rs");
    write_file(&path, &result.code, ctx.force)?;
    let core_pkg = ctx.naming.package(&crate::naming::PackageKind::Core);
    let server_pkg = ctx.naming.package(&crate::naming::PackageKind::Server);
    cache.server = Some(PackagePlan::new(
        crate::naming::PackageKind::Server,
        vec![
            (
                "Cargo.toml".to_string(),
                forgedb_codegen::ServerPackage::cargo_toml(&server_pkg, &core_pkg),
            ),
            // `api.rs` needs no generator change: it opens with `use super::*;`,
            // so a `main.rs` that globs `forgedb_core` compiles it verbatim.
            ("src/api.rs".to_string(), result.code.clone()),
            (
                "src/main.rs".to_string(),
                forgedb_codegen::ServerPackage::main_rs(),
            ),
        ],
    ));
    cache.api = Some(result.code.clone());
    files.push((path, result));
    Ok(())
}

/// Generate the native FFI engine package (language bindings #51/#52/#117): the
/// Layer-0 C-ABI spine over the app's one `core`.
///
/// **It writes nothing into `output`** (#335 §6): the package is emitted wholly
/// into the build cache, where `forgedb build` compiles it. Idempotent, because
/// the `go` arm needs the same package.
fn generate_ffi_engine(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
    reason: FfiReason,
) -> Result<()> {
    // Set BEFORE the memo check: `targets = ["go", "ffi"]` reaches this twice,
    // and a flag set after an early return records whichever call happened to be
    // first.
    if reason == FfiReason::Declared {
        cache.ffi_declared = true;
    }
    if cache.wrapper(&crate::naming::PackageKind::Ffi).is_some() {
        return Ok(());
    }
    ensure_database(ctx, cache, files)?;
    let ffi_result = FfiGenerator::generate(ctx.schema, &ctx.naming.symbol_prefix)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    cache.plan_wrapper(
        crate::naming::PackageKind::Ffi,
        FfiGenerator::cargo_toml(
            &ctx.naming.package(&crate::naming::PackageKind::Ffi),
            &ctx.naming.package(&crate::naming::PackageKind::Core),
        ),
        ffi_result.code,
        &[],
    );
    Ok(())
}

/// Generate the PyO3 Python binding package (#51) into the build cache.
///
/// `#[pymodule] fn forgedb` is deliberately NOT renamed to the derived package
/// name: CPython resolves `PyInit_<stem>` from the **delivered filename**, so the
/// user's `import forgedb` depends on how the artifact is named on disk, not on
/// cargo's package name.
fn generate_pyo3_binding(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    if cache.wrapper(&crate::naming::PackageKind::Pyo3).is_some() {
        return Ok(());
    }
    ensure_database(ctx, cache, files)?;
    let py_result = PyO3Generator::generate(ctx.schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    cache.plan_wrapper(
        crate::naming::PackageKind::Pyo3,
        PyO3Generator::cargo_toml(
            &ctx.naming.package(&crate::naming::PackageKind::Pyo3),
            &ctx.naming.package(&crate::naming::PackageKind::Core),
        ),
        py_result.code,
        // The `build.rs` is not optional packaging: without
        // `pyo3_build_config::add_extension_module_link_args()` a plain
        // `cargo build` of an extension module fails at LINK time on macOS
        // (undefined `_PyExc_*`), which a `cargo check` never reaches.
        &[("build.rs", PyO3Generator::build_rs_scaffold().to_string())],
    );
    Ok(())
}

/// Generate the NAPI-RS Node/Bun binding package (#52/#117) into the build
/// cache. One `.node` addon serves both runtimes (Option A).
fn generate_napi_binding(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    if cache.wrapper(&crate::naming::PackageKind::Napi).is_some() {
        return Ok(());
    }
    ensure_database(ctx, cache, files)?;
    let napi_result = NapiGenerator::generate(ctx.schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    cache.plan_wrapper(
        crate::naming::PackageKind::Napi,
        NapiGenerator::cargo_toml(
            &ctx.naming.package(&crate::naming::PackageKind::Napi),
            &ctx.naming.package(&crate::naming::PackageKind::Core),
        ),
        napi_result.code,
        // The `package.json` no longer lives here: it is the CONSUMER's file and
        // moved to `<output>/napi/` with the entry module it names (#337).
        &[("build.rs", NapiGenerator::build_rs_scaffold().to_string())],
    );
    Ok(())
}

/// Generate the `wasm32` browser read-replica (#110 Milestone C).
///
/// The Rust crate moves into the cache like the other three wrappers. The
/// **browser-side assets do not**: `replica-client.ts` and `replica-worker.js`
/// are files the user's page imports and serves, and a content-hashed directory
/// under `~/.forgedb` is unservable — the same argument §6 makes for keeping
/// `go/` in `output`. So `replica/` survives in `output` holding `client/` only,
/// and its `src/` is superseded by [`supersede_moved_packages`].
fn generate_wasm_replica(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    if cache.wrapper(&crate::naming::PackageKind::Wasm).is_some() {
        return Ok(());
    }
    ensure_database(ctx, cache, files)?;

    // The wasm-bindgen transport glue — a cache package.
    let wasm_result =
        WasmGenerator::generate(ctx.schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    cache.plan_wrapper(
        crate::naming::PackageKind::Wasm,
        WasmGenerator::cargo_toml(
            &ctx.naming.package(&crate::naming::PackageKind::Wasm),
            &ctx.naming.package(&crate::naming::PackageKind::Core),
        ),
        wasm_result.code,
        &[],
    );

    // The main-thread async client (#110 #2): a per-schema TS `ReplicaClient`
    // that RPCs into the Worker running the engine — mirrors the transport's read
    // surface exactly, invents nothing.
    let client_dir = ctx.output.join("replica").join("client");
    fs::create_dir_all(&client_dir)?;
    let client_result = WasmGenerator::generate_client(ctx.schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let client_path = client_dir.join("replica-client.ts");
    write_file(&client_path, &client_result.code, ctx.force)?;
    files.push((client_path, client_result));

    // The STATIC, schema-agnostic Worker bootstrap. It runs the engine,
    // follows `/replicate`, and debounces auto-commit. Emitted verbatim (NOT from
    // a schema-aware path) so it cannot become schema-aware — the PM constraint.
    let worker_path = client_dir.join("replica-worker.js");
    write_file(
        &worker_path,
        &WasmGenerator::worker_bootstrap_with_config(ctx.gen_config),
        ctx.force,
    )?;
    ui::info(&format!(
        "  ✓ {} (static worker bootstrap)",
        worker_path.display()
    ));

    Ok(())
}

/// Generate the Golang binding (RFC #203): a per-schema Go cgo package
/// (`go/forgedb.go` + `go/forgedb.h`) that binds the generated native FFI C-ABI.
///
/// **`go/` stays in `output`** (#335 §6): it is Go source the user's program
/// imports, and a hashed cache directory is unimportable. The engine it links is
/// the cache's FFI package, delivered here as `libforgedb.a` by `forgedb build`
/// — see [`crate::commands::build::deliver`].
fn generate_go_binding(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    // The FFI engine package the Go binding links against — reused verbatim, so
    // Go requires no new C symbol. Idempotent, so `targets = ["ffi", "go"]`
    // emits one engine rather than racing two arms to write the same files.
    generate_ffi_engine(ctx, cache, files, FfiReason::ForGo)?;

    let go_dir = ctx.output.join("go");
    fs::create_dir_all(&go_dir)?;

    // **The `ffi` package's fingerprint, not this directory's.** `<output>/go/`
    // is not a cache package and is not hashed; the archive `forgedb build`
    // delivers here IS the `ffi` package's artifact, so the `ffi` value is the
    // only one that can agree with what the archive exports. A per-directory
    // reading of the same granularity rule breaks the comparison.
    let fingerprint = cache
        .fingerprint(&crate::naming::PackageKind::Ffi)
        .ok_or_else(|| {
            CliError::CodeGeneration(
                "the Go binding needs the FFI package's source fingerprint, and no FFI \
                 package was planned. This is a ForgeDB bug; please report it."
                    .to_string(),
            )
        })?;

    // The generated Go cgo package.
    let go_result = GoGenerator::generate(ctx.schema, &ctx.naming.symbol_prefix, &fingerprint)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let go_path = go_dir.join("forgedb.go");
    write_file(&go_path, &go_result.code, ctx.force)?;
    files.push((go_path, go_result));

    // The async completion bridge (the `//export` callback — a separate file, per
    // cgo's rule that an `//export` file's preamble carries no C definitions).
    let async_result = GoGenerator::generate_async_bridge(&ctx.naming.symbol_prefix);
    let async_path = go_dir.join("forgedb_async.go");
    write_file(&async_path, &async_result.code, ctx.force)?;
    files.push((async_path, async_result));

    // Arrow columnar export (only when the schema has exportable columns) — the
    // ONE part of the Go binding that pulls an external module (arrow-go).
    let needs_arrow = GoGenerator::needs_arrow(ctx.schema);
    if let Some(arrow_result) = GoGenerator::generate_arrow(ctx.schema, &ctx.naming.symbol_prefix) {
        let arrow_path = go_dir.join("forgedb_arrow.go");
        write_file(&arrow_path, &arrow_result.code, ctx.force)?;
        files.push((arrow_path, arrow_result));
        ui::warning(
            "the Go Arrow export uses the external module `github.com/apache/arrow-go/v18` \
             (added to go.mod) — run `go mod tidy` in the `go/` dir before `go build`",
        );
    }

    // The C header cgo `#include`s (declares the app's prefixed prototypes).
    // Emitted by `FfiGenerator` — the symbols it declares are DEFINED there.
    let header_result =
        FfiGenerator::generate_header(ctx.schema, &ctx.naming.symbol_prefix, &fingerprint)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let header_path = go_dir.join("forgedb.h");
    write_file(&header_path, &header_result.code, ctx.force)?;
    files.push((header_path, header_result));

    // User-editable `go.mod` — written only when absent. This only-if-absent rule
    // survives the flip because `go/` is a DELIVERED directory in the user's tree
    // (#335 §7 retires it only for cache members, which are ForgeDB-owned).
    let go_mod_path = go_dir.join("go.mod");
    if !go_mod_path.exists() {
        fs::write(
            &go_mod_path,
            GoGenerator::go_mod_scaffold("forgedb", needs_arrow),
        )?;
        ui::info(&format!(
            "  ✓ {} (Go module scaffold)",
            go_mod_path.display()
        ));
    }

    let readme_path = go_dir.join("README.md");
    if !readme_path.exists() {
        fs::write(&readme_path, GoGenerator::readme_scaffold())?;
        ui::info(&format!("  ✓ {} (Go binding README)", readme_path.display()));
    }

    Ok(())
}

/// Generate the Rust REST client SDK crate (#206): a `reqwest`-based async client
/// (`rust-sdk/src/lib.rs`) over the generated REST API, plus a `Cargo.toml`
/// scaffold written only when absent. A transport client — links none of the
/// forgedb substrate crates, only `reqwest`/`serde`.
///
/// **Adopt it; do not build it in place.** `cargo build` inside `rust-sdk/` works
/// only when nothing above the output directory is a cargo workspace. In a
/// consumer whose tree *is* one — the common case — cargo refuses with *"current
/// package believes it's in a workspace when it's not"*, because the crate sits
/// under a workspace root without being a member. Add a path dep on it from
/// whichever crate should call the API and build from the workspace root; cargo
/// makes a path dependency residing inside the workspace directory a member on
/// its own.
///
/// Emitting a `[workspace]` table here would NOT fix that: a nested package
/// carrying one that any member path-depends on makes cargo fail the entire
/// workspace (#430, closed as not-a-defect — the same reason #338's in-tree
/// package emits none). `parent-workspace` in `.github/workflows/substrate-reclose.yml`
/// is what keeps this paragraph honest.
fn generate_rust_sdk(
    ctx: &Emit<'_>,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    let sdk_dir = ctx.output.join("rust-sdk");
    let src_dir = sdk_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let result = RustSdkGenerator::generate(ctx.schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let lib_path = src_dir.join("lib.rs");
    write_file(&lib_path, &result.code, ctx.force)?;
    files.push((lib_path, result));

    let cargo_path = sdk_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        fs::write(
            &cargo_path,
            RustSdkGenerator::cargo_toml_scaffold("forgedb-client"),
        )?;
        ui::info(&format!("  ✓ {} (Rust SDK scaffold)", cargo_path.display()));
    }
    Ok(())
}

/// Generate the Python REST client SDK (#118): a stdlib-`urllib` client module
/// (`python-sdk/forgedb_client.py`) over the generated REST API, plus a
/// `pyproject.toml` scaffold written only when absent. Dependency-free. Install
/// with `pip install .` in `python-sdk/`.
fn generate_python_sdk(
    ctx: &Emit<'_>,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    let sdk_dir = ctx.output.join("python-sdk");
    fs::create_dir_all(&sdk_dir)?;

    let result = PythonSdkGenerator::generate(ctx.schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let py_path = sdk_dir.join("forgedb_client.py");
    write_file(&py_path, &result.code, ctx.force)?;
    files.push((py_path, result));

    let pyproject_path = sdk_dir.join("pyproject.toml");
    if !pyproject_path.exists() {
        fs::write(&pyproject_path, PythonSdkGenerator::pyproject_scaffold())?;
        ui::info(&format!(
            "  ✓ {} (Python SDK pyproject)",
            pyproject_path.display()
        ));
    }
    Ok(())
}

/// Generate the Go REST client SDK (#205): a pure-stdlib `net/http` client
/// package (`go-sdk/client.go`) over the generated REST API, plus `go.mod` /
/// `README.md` scaffolds written only when absent. No cgo, no external module.
/// Build with `go build` in `go-sdk/`.
fn generate_go_sdk(
    ctx: &Emit<'_>,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    let sdk_dir = ctx.output.join("go-sdk");
    fs::create_dir_all(&sdk_dir)?;

    let result =
        GoSdkGenerator::generate(ctx.schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let go_path = sdk_dir.join("client.go");
    write_file(&go_path, &result.code, ctx.force)?;
    files.push((go_path, result));

    let mod_path = sdk_dir.join("go.mod");
    if !mod_path.exists() {
        fs::write(&mod_path, GoSdkGenerator::go_mod_scaffold("forgedb-client"))?;
        ui::info(&format!(
            "  ✓ {} (Go SDK module scaffold)",
            mod_path.display()
        ));
    }

    let readme_path = sdk_dir.join("README.md");
    if !readme_path.exists() {
        fs::write(&readme_path, GoSdkGenerator::readme_scaffold())?;
        ui::info(&format!("  ✓ {} (Go SDK README)", readme_path.display()));
    }
    Ok(())
}

/// Write the npm packaging scaffold for the generated TypeScript SDK (Phase 5):
/// `package.json` + `tsconfig.json` alongside `types.ts`.  These are
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
/// Test-only door onto [`resolve_target`], so `crate::targets`'s anti-drift
/// guard can assert that every config spelling means what its documented
/// command line means.  Exposing the resolver rather than duplicating its table
/// is the point: a copy is a second definition, which is the defect decision 10
/// removes.
#[cfg(test)]
pub fn resolve_target_for_test(raw: &str, mode: Option<GenerateMode>) -> Result<String> {
    resolve_target(raw, mode)
}

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
    // One list of candidate names, in `project` (#333) — this used to be one of
    // three open-coded copies, so adding a name meant finding all three.
    Ok(crate::project::find_schema(None)?.display().to_string())
}

/// Substrate `core` re-exports so its dependents can reach it without pinning it.
///
/// **This is what makes substrate type identity structural rather than lucky.**
/// The generated `api.rs` and the four binding wrappers name substrate crates
/// ABSOLUTELY — `forgedb_storage::Snapshot`, `forgedb_types::*` — so each of
/// them would otherwise have to pin those crates itself, and their types would
/// unify with `core`'s only because one lockfile happened to resolve several
/// independently-authored pin lists identically. Routing every dependent
/// through `core` makes agreement a property of the code.
///
/// Only the crates a dependent names but does not pin belong here. `auth` and
/// `query-params` stay pinned by `server` directly: those are API-layer
/// substrate that `core` itself does not link.
///
/// `changefeed` IS here, and it is the one entry that is not about `api.rs`:
/// the wasm replica names `forgedb_changefeed::durable::PersistedEvent` to
/// decode the frames it follows. `core` already pins `forgedb-changefeed`
/// unconditionally, so re-exporting it costs the replica nothing and buys the
/// same type-identity guarantee as the other two — a replica that pinned it
/// itself would decode a `PersistedEvent` that is only coincidentally the same
/// type as the one `core` was compiled against.
///
/// Reached through the crate root's `use forgedb_core::*;`, which is why these
/// must be `pub use` at the root rather than inside a module.
pub const CORE_SUBSTRATE_REEXPORTS: &str = "\n\
// ---------------------------------------------------------------------------\n\
// Appended by ForgeDB (#335 §1). Not part of the generated database.\n\
//\n\
// Dependents of this crate name these substrate crates by absolute path. They\n\
// are re-exported here so those dependents pin ZERO substrate of their own and\n\
// their types UNIFY with this crate's, rather than merely resolving to the same\n\
// version by lockfile coincidence.\n\
// ---------------------------------------------------------------------------\n\
pub use forgedb_changefeed;\n\
pub use forgedb_storage;\n\
pub use forgedb_types;\n";

/// Write the app's cache packages from the values this invocation produced
/// (#335 §1/§6).
///
/// # One value, two writes
///
/// `core/src/lib.rs` receives **the same `String` `<output>/database.rs`
/// received** — [`ensure_database`] builds it once and both sinks read it. The
/// shipped defect was the opposite: five arms each calling `RustGenerator`, only
/// one of them threading the app's `GenConfig`, so one `generate` run produced
/// two databases with different durability semantics.
///
/// The old "are all five copies equal?" check is gone, and its absence is the
/// point: there is now one copy, so there is nothing left to compare. A check
/// that can never fire is a check that stops being read.
///
/// # The manifests are rewritten, not preserved
///
/// Every scaffolder in the output directory writes its `Cargo.toml` only when
/// absent, and says so: those files are the user's. **Nothing in the cache is
/// user-editable.** Carried forward unchanged, a CLI upgrade that bumps a
/// substrate pin would never reach an existing member, and the stale pin would
/// sit in a directory the user never opens where the publish-gap check cannot
/// see it.
/// Write one rendered `core` package to `dir`, and return the paths written.
///
/// **Both destinations go through here** — the cache member and #338's in-tree
/// placement. One renderer (`CorePackage::files`) and one writer is what makes
/// "the same package, two destinations" structural rather than two emitters that
/// happen to agree.
///
/// `fs::write`, never `write_file`: `write_file` refuses an existing file
/// without `--force`, and a `core` package — in the cache or in the user's tree
/// (#338) — is **ForgeDB's file**, rewritten in full on every generate. That is
/// what makes a CLI upgrade's substrate pin reach an existing project instead of
/// freezing at whatever the first run wrote (#290's floor problem).
fn write_core_package<P: AsRef<Path>>(dir: &Path, files: &[(P, String)]) -> Result<Vec<PathBuf>> {
    let mut written = Vec::with_capacity(files.len());
    for (rel, body) in files {
        let path = dir.join(rel.as_ref());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, body)?;
        written.push(path);
    }
    Ok(written)
}

/// Write every planned cache package.
///
/// **It writes plans, and renders nothing.** Every byte here was rendered during
/// the emitting arms, which is what lets the fingerprint cover the exact bytes
/// that land and lets the shims carrying that value be written in the same
/// invocation. Rendering here again would be a second derivation of one artifact.
///
/// Everything is rewritten in full on every generate. There is no
/// write-only-when-absent branch **on purpose**: the output directory's
/// scaffolds are the user's and are preserved, but a stale manifest in a
/// directory the user never opens is how a CLI upgrade that bumps a substrate
/// pin fails to reach an existing project.
fn emit_cache_packages(container: &Path, cache: &CacheEmission) -> Result<()> {
    let Some(core) = cache.core.as_ref() else {
        // No Rust database was emitted in this invocation (a `--sdk`-only or
        // stubs-only run), so there is no `core` to write and nothing that
        // depends on one.
        return Ok(());
    };

    // `write_core_package` is the ONE writer, shared with #338's in-tree
    // placement. Two destinations, one writer; a second copy of "create the
    // parent, write the bytes" is where the two would start disagreeing about,
    // say, whether a stale file is removed first.
    write_plan(container, core)?;

    if let Some(server) = cache.server.as_ref() {
        write_plan(container, server)?;
    }

    // --- The binding wrappers (#335 §1, steps 5b + 7) ------------------------
    //
    // Each manifest pins ZERO substrate, reaching all of it through `core`. That
    // is what makes their substrate types UNIFY with `core`'s: before this,
    // every wrapper carried its own pin list beside its own copy of
    // `database.rs`, and the copies agreed only because one lockfile resolved
    // several independently-authored lists the same way.
    //
    // A wrapper arm cannot fire without `core`: every emitter that plans one
    // calls `ensure_database` first, and this function returns early when that
    // produced nothing.
    for wrapper in &cache.wrappers {
        write_plan(container, wrapper)?;
    }

    Ok(())
}

/// Write one planned package into `container`.
fn write_plan(container: &Path, plan: &PackagePlan) -> Result<()> {
    let dir = container.join(plan.kind.dir());
    write_core_package(&dir, &plan.files)?;
    ui::detail(&format!("  ✓ {} (cache package)", dir.display()));
    Ok(())
}

// ===========================================================================
// The consumer-facing half (#337)
// ===========================================================================

/// What `<output>/.gitignore` says.
///
/// **Extensions only.** Never a directory, never `*.rs`, never `Cargo.toml`:
/// #338 writes a ForgeDB-owned cargo package into the consumer's tree and that
/// package is committed source, so a pattern here that swallowed a directory or
/// a Rust file would silently un-commit it.
///
/// The re-includes are not optional. `forgedb init`'s root `.gitignore` ignores
/// `*.js` and `*.d.ts` project-wide (they are build output for a TypeScript
/// project), and a deeper `.gitignore` is the only thing that can override that
/// for this subtree — otherwise the shims this whole mechanism exists to commit
/// are uncommittable. A free consequence: `replica/client/replica-worker.js`,
/// generated text that has never been committable, becomes so.
pub const OUTPUT_GITIGNORE: &str = "\
# Generated by ForgeDB. Rewritten on every generate.
#
# Everything ForgeDB generates here is TEXT you commit: database.rs, api.rs,
# types.ts, openapi.json, the client SDKs, the Go package, the shims. The only
# things ignored are the COMPILED artifacts `forgedb build` delivers beside
# them — each machine builds its own, from your schema, with your toolchain.
#
# Patterns are extensions only, deliberately: a directory pattern here would
# also swallow generated source, and ForgeDB owns this directory but does not
# own your judgement about what belongs in it.
*.a
*.lib
*.node
*.so
*.dylib

# ForgeDB's own shims. The project root .gitignore ignores these two extensions
# project-wide (they are build output for a TypeScript project); this subtree is
# the exception, and a deeper .gitignore is the only thing that can say so.
!*.js
!*.d.ts
";

/// Write the consumer-facing half of every delivered target: the shims that
/// carry the fingerprint, and the `.gitignore` that keeps the compiled halves
/// out of the repository.
///
/// Each shim is generated text like `types.ts` — committed, reviewable, and
/// covered by `generate --check`.
fn emit_consumer_shims(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    use crate::naming::PackageKind;

    // `fs::write`, not `write_file`: this is ForgeDB's statement about what
    // ForgeDB delivers, so it is rewritten on every generate rather than frozen
    // at whatever the project's first run wrote. A project that predates a newly
    // delivered name would otherwise commit a binary silently.
    let gitignore = ctx.output.join(".gitignore");
    fs::write(&gitignore, OUTPUT_GITIGNORE)?;
    files.push((
        gitignore,
        forgedb_codegen::GeneratedCode {
            code: OUTPUT_GITIGNORE.to_string(),
            description: "ignore rules for the delivered artifacts".to_string(),
        },
    ));

    // --- Node / Bun ---------------------------------------------------------
    if let Some(fp) = plan_fingerprint(cache, &PackageKind::Napi) {
        let dir = ctx.output.join(PackageKind::Napi.dir());
        fs::create_dir_all(&dir)?;

        let entry = NapiGenerator::entry_module(&fp);
        let entry_path = dir.join("index.js");
        write_file(&entry_path, &entry.code, ctx.force)?;
        files.push((entry_path, entry));

        let dts = NapiGenerator::type_declarations(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let dts_path = dir.join("index.d.ts");
        write_file(&dts_path, &dts.code, ctx.force)?;
        files.push((dts_path, dts));

        reconcile_napi_package_json(&dir)?;
    }

    // --- Python -------------------------------------------------------------
    if let Some(fp) = plan_fingerprint(cache, &PackageKind::Pyo3) {
        let dir = ctx.output.join(PackageKind::Pyo3.dir());
        fs::create_dir_all(&dir)?;

        let module = PyO3Generator::python_module(ctx.schema, &fp)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let module_path = dir.join("forgedb.py");
        write_file(&module_path, &module.code, ctx.force)?;
        files.push((module_path, module));

        let stub = PyO3Generator::type_stub(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let stub_path = dir.join("forgedb.pyi");
        write_file(&stub_path, &stub.code, ctx.force)?;
        files.push((stub_path, stub));
    }

    // --- C ------------------------------------------------------------------
    //
    // Keyed on `ffi_declared`, not on the package's existence: the `go` arm
    // plans the same package, and a Go-only project must not grow an
    // `<output>/ffi/` directory — a directory that exists is a directory
    // delivery delivers into.
    let ffi_fp = plan_fingerprint(cache, &PackageKind::Ffi);
    if cache.ffi_declared && let Some(fp) = ffi_fp {
        let dir = ctx.output.join(PackageKind::Ffi.dir());
        fs::create_dir_all(&dir)?;
        let header = FfiGenerator::generate_header(ctx.schema, &ctx.naming.symbol_prefix, &fp)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let header_path = dir.join("forgedb.h");
        write_file(&header_path, &header.code, ctx.force)?;
        files.push((header_path, header));
    }

    Ok(())
}

/// Compute a wrapper's fingerprint and put the emitted constant file into its
/// plan, so the cache package carries the value the shim was written with.
///
/// The two acts are one function because they must not be separable: a shim
/// written from a value the package never receives is a load check guaranteed to
/// fail, and a package carrying a value no shim compares is one guaranteed never
/// to run.
fn plan_fingerprint(
    cache: &mut CacheEmission,
    kind: &crate::naming::PackageKind,
) -> Option<String> {
    let value = cache.fingerprint(kind)?;
    let plan = cache.wrapper_mut(kind)?;
    // Idempotent: the `ffi` plan is reached twice when a project declares both
    // `ffi` and `go`.
    if !plan.files.iter().any(|(rel, _)| rel == crate::fingerprint::FINGERPRINT_FILE) {
        plan.push(
            crate::fingerprint::FINGERPRINT_FILE,
            crate::fingerprint::fingerprint_rs(&value),
        );
    }
    Some(value)
}

/// Write `<output>/napi/package.json`, or repoint a pre-#337 one.
///
/// It is the consumer's file — written only when absent, like every other
/// scaffold in the output directory. The reconciliation is the exception, and it
/// is narrow: `main`/`types` are rewritten ONLY when they still carry the
/// pre-#337 values, and every other key is preserved.
///
/// Without it a pre-#337 project's `main` still names `forgedb.node`, `require`
/// resolves the addon directly, and `index.js` **never executes** — a load check
/// that is present, correct, and never run. That is worse than an absent one,
/// because it reads as coverage.
fn reconcile_napi_package_json(dir: &Path) -> Result<()> {
    let path = dir.join("package.json");
    let Ok(existing) = fs::read_to_string(&path) else {
        fs::write(&path, NapiGenerator::package_json_scaffold())?;
        ui::info(&format!("  ✓ {} (npm binding scaffold)", path.display()));
        return Ok(());
    };

    let Ok(mut doc) = serde_json::from_str::<serde_json::Value>(&existing) else {
        // Not ours to repair. A `package.json` we cannot parse is one a human
        // has to look at, and rewriting it would destroy what they wrote.
        ui::warning(&format!(
            "{} is not valid JSON — leaving it alone. If `main` still names \
             `forgedb.node`, the generated `index.js` load check never runs.",
            path.display()
        ));
        return Ok(());
    };

    let mut changed = Vec::new();
    if doc.get("main").and_then(|v| v.as_str()) == Some(NapiGenerator::LEGACY_MAIN) {
        doc["main"] = serde_json::Value::String("index.js".to_string());
        changed.push("main");
    }
    if doc.get("types").is_none() {
        doc["types"] = serde_json::Value::String("index.d.ts".to_string());
        changed.push("types");
    }
    if changed.is_empty() {
        return Ok(());
    }
    let rendered = serde_json::to_string_pretty(&doc)
        .map_err(|e| CliError::Other(format!("could not render {}: {e}", path.display())))?;
    fs::write(&path, format!("{rendered}\n"))?;
    ui::warning(&format!(
        "repointed {} ({}) — `main` named the addon directly, so the generated \
         `index.js` load check would never have run",
        path.display(),
        changed.join(", ")
    ));
    Ok(())
}

// ===========================================================================
// The supersession rule (#335 §6)
// ===========================================================================

/// The package directories `output` has stopped receiving, and the Rust files
/// ForgeDB used to write under each.
///
/// `replica` is here for `src/` only — its `client/` assets are still emitted
/// (see [`generate_wasm_replica`]).
const MOVED_PACKAGES: [&str; 4] = ["ffi", "napi", "pyo3", "replica"];

/// The generated Rust files each moved package used to hold. Naming them
/// explicitly rather than walking `src/` is deliberate: the rule is "replace
/// what ForgeDB generated", and a walk would also rewrite a file the user put
/// there.
const MOVED_PACKAGE_FILES: [&str; 2] = ["lib.rs", "database.rs"];

/// Replace a superseded generated file's contents with a `compile_error!`.
///
/// Pure and deterministic — the idempotence in [`supersede_moved_packages`] is a
/// content compare against this exact string, so it must not carry a timestamp,
/// a path that varies, or anything else that changes between runs.
fn supersession_text(package: &str) -> String {
    format!(
        "// Superseded by ForgeDB. This file is NO LONGER GENERATED here.\n\
         //\n\
         // ForgeDB now owns the build: the `{package}` package is emitted into, and\n\
         // compiled from, the ForgeDB build cache instead of this directory. Nothing\n\
         // regenerates this copy, so leaving it compilable would let a build keep\n\
         // succeeding against a database that no longer tracks your schema.\n\
         //\n\
         //   forgedb build                     # compile the current packages\n\
         //   forgedb build --report -          # where every artifact landed\n\
         //\n\
         // Delete this directory once nothing reads it. ForgeDB will not delete it\n\
         // for you, and it has left every file it did not generate untouched.\n\
         compile_error!(\"ForgeDB no longer generates the `{package}` package here — it moved into the ForgeDB build cache. Run `forgedb build`.\");\n"
    )
}

/// Replace every Rust file ForgeDB generated under a moved package with a
/// `compile_error!` naming what happened, reporting each path it rewrites.
///
/// # Why this is not optional cleanup
///
/// Removing `ffi/`, `napi/`, `pyo3/` and `replica/`'s crate from `output` leaves
/// four directories frozen, never regenerated, **and still compilable** — in the
/// exact workflow ForgeDB's own Go README and its own reclose tell users to run.
/// Their build keeps going green against a `database.rs` that no longer tracks
/// the schema. This converts every one of those silent-stale-success cases into
/// a build failure carrying a message.
///
/// Idempotent by content compare, and it touches nothing else: the user-editable
/// scaffolds beside these files (`pyproject.toml`, `package.json`, `go.mod`,
/// `Cargo.toml`) are left exactly as they are.
fn supersede_moved_packages(output_dir: &Path) -> Result<()> {
    for package in MOVED_PACKAGES {
        let want = supersession_text(package);
        for file in MOVED_PACKAGE_FILES {
            let path = output_dir.join(package).join("src").join(file);
            // Never *create* one: only a file ForgeDB previously wrote here is
            // superseded. An absent file is a project that never enabled this
            // target, and planting a `compile_error!` in it would invent a
            // failure rather than describe one.
            let Ok(existing) = fs::read_to_string(&path) else {
                continue;
            };
            if existing == want {
                continue;
            }
            fs::write(&path, &want)?;
            ui::warning(&format!(
                "superseded {} — the `{}` package moved into the ForgeDB build cache",
                path.display(),
                package
            ));
        }
    }
    Ok(())
}

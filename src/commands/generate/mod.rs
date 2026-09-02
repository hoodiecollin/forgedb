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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateMode {
    Sdk,
    Runtime,
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
    pub mode: Option<GenerateMode>,
    pub check: bool,
    pub output: Option<String>,
    pub schema: Option<String>,
    pub config_targets: Option<Vec<String>>,
    pub gen_config: forgedb_codegen::GenConfig,
    pub force: bool,
    pub from: Option<u32>,
    pub to: Option<u32>,
    pub cache_container: Option<PathBuf>,
    pub in_tree: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct AppNaming {
    app_name: String,
    symbol_prefix: String,
}

impl AppNaming {
    const NO_CONTAINER: &'static str = "local";

    fn for_run(container: Option<&Path>, schema_path: &str) -> AppNaming {
        let app_name = container
            .and_then(crate::cache::member_app_name)
            .unwrap_or_else(|| {
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

struct Emit<'a> {
    schema: &'a forgedb_parser::Schema,
    output: &'a Path,
    schema_version: u32,
    gen_config: forgedb_codegen::GenConfig,
    naming: &'a AppNaming,
}

#[derive(Default)]
struct CacheEmission {
    core_lib: Option<String>,
    api: Option<String>,
    core: Option<PackagePlan>,
    server: Option<PackagePlan>,
    wrappers: Vec<PackagePlan>,
    fingerprints: std::collections::BTreeMap<String, String>,
    ffi_declared: bool,
}

struct PackagePlan {
    kind: crate::naming::PackageKind,
    files: Vec<(String, String)>,
}

impl PackagePlan {
    fn new(kind: crate::naming::PackageKind, files: Vec<(String, String)>) -> PackagePlan {
        PackagePlan { kind, files }
    }

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum FfiReason {
    Declared,
    ForGo,
}

impl CacheEmission {
    fn wrapper(&self, kind: &crate::naming::PackageKind) -> Option<&PackagePlan> {
        self.wrappers.iter().find(|p| &p.kind == kind)
    }

    fn wrapper_mut(&mut self, kind: &crate::naming::PackageKind) -> Option<&mut PackagePlan> {
        self.wrappers.iter_mut().find(|p| &p.kind == kind)
    }

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

    let schema_path = match options.schema.as_deref() {
        Some(p) => p.to_string(),
        None => find_schema_file()?,
    };
    ui::info(&format!("Using schema: {}", schema_path));

    let schema_content = fs::read_to_string(&schema_path)
        .map_err(|e| CliError::SchemaNotFound(format!("{}: {}", schema_path, e)))?;

    let mut parser = Parser::new(&schema_content)
        .map_err(|e| CliError::SchemaValidation(format!("Lexer error: {}", e)))?;

    let schema = parser
        .parse()
        .map_err(|e| CliError::SchemaValidation(format!("Parser error: {}", e)))?;

    let _ = crate::diagnostics::report(&parser.take_warnings());

    ui::success(&format!(
        "Parsed schema ({} models, {} total fields)",
        schema.models.len(),
        schema.models.iter().map(|m| m.fields.len()).sum::<usize>()
    ));

    let lineage_dir = crate::project::migrations_dir(Path::new(&schema_path));
    let schema_version = forgedb_migrations::current_schema_version(&lineage_dir);

    let output_dir = options.output.as_deref().unwrap_or("./generated");
    let committed_path = PathBuf::from(output_dir);

    let output_path = if options.check {
        std::env::temp_dir().join(format!("forgedb-check-{}", std::process::id()))
    } else {
        committed_path.clone()
    };
    if let Some(dir) = options.in_tree.as_deref() {
        in_tree::guard(dir)?;
    }

    if options.check {
        let _ = fs::remove_dir_all(&output_path);
    }
    fs::create_dir_all(&output_path)?;

    let target = resolve_target(&options.target, options.mode)?;
    ui::detail(&format!("output dir: {}", committed_path.display()));
    ui::detail(&format!("schema version: {}", schema_version));
    ui::detail(&format!("resolved target: {}", target));

    let naming = AppNaming::for_run(options.cache_container.as_deref(), &schema_path);

    let ctx = Emit {
        schema: &schema,
        output: &output_path,
        schema_version,
        gen_config: options.gen_config,
        naming: &naming,
    };

    let mut cache = CacheEmission::default();
    let mut generated_files = Vec::new();

    match target.as_str() {
        "all" => {
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
            write_file(&path, &result.code)?;
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
            write_file(&path, &result.code)?;
            generated_files.push((path, result));
        }
        "stubs" => {
            let result = StubGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let stubs_dir = output_path.join("stubs");
            fs::create_dir_all(&stubs_dir)?;
            let path = stubs_dir.join("README.md");
            write_file(&path, &result.code)?;
            generated_files.push((path, result));
        }
        "wasm" => {
            generate_wasm_replica(&ctx, &mut cache, &mut generated_files)?;
        }
        "ffi" => {
            generate_ffi_engine(&ctx, &mut cache, &mut generated_files, FfiReason::Declared)?;
        }
        "pyo3" => {
            generate_pyo3_binding(&ctx, &mut cache, &mut generated_files)?;
        }
        "napi" => {
            generate_napi_binding(&ctx, &mut cache, &mut generated_files)?;
        }
        "go" => {
            generate_go_binding(&ctx, &mut cache, &mut generated_files)?;
        }
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

    emit_consumer_shims(&ctx, &mut cache, &mut generated_files)?;

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

        ui::blank();
        for path in &missing {
            ui::error(&format!("  missing: {}", path.display()));
        }
        for path in &stale {
            ui::error(&format!("  stale:   {}", path.display()));
        }
        ui::blank();
        ui::error(&format!(
            "Generated code is out of date ({} missing, {} stale) — run `forgedb generate` to update.",
            missing.len(),
            stale.len()
        ));
        return Err(CliError::CodeGeneration(
            "generated code is out of date".to_string(),
        ));
    }

    supersede_moved_packages(&committed_path)?;

    if let Some(container) = &options.cache_container {
        emit_cache_packages(container, &cache)?;
    }

    if let (Some(dir), Some(core_lib)) = (options.in_tree.as_deref(), cache.core_lib.as_deref()) {
        in_tree::emit(
            dir,
            &naming.package(&crate::naming::PackageKind::Core),
            &ctx.gen_config,
            core_lib,
        )?;
    }

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

fn generate_all(
    ctx: &Emit<'_>,
    target_filter: Option<&[String]>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    let enabled = |name: &str| -> bool {
        target_filter.is_none_or(|ts| ts.iter().any(|t| t.as_str() == name))
    };
    let opt_in = |name: &str| -> bool {
        target_filter.is_some_and(|ts| ts.iter().any(|t| t.as_str() == name))
    };

    if enabled("rust") {
        ensure_database(ctx, cache, files)?;
    }

    if enabled("typescript") {
        let ts_result = TypeScriptGenerator::generate(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let ts_path = ctx.output.join("types.ts");
        write_file(&ts_path, &ts_result.code)?;
        files.push((ts_path, ts_result));
        write_ts_package_scaffold(ctx.output)?;
    }

    if enabled("api") {
        emit_api(ctx, cache, files)?;
    }

    if enabled("openapi") {
        let openapi_result = OpenApiGenerator::generate(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let openapi_path = ctx.output.join("openapi.json");
        write_file(&openapi_path, &openapi_result.code)?;
        files.push((openapi_path, openapi_result));
    }

    if enabled("stubs") {
        let stub_result = StubGenerator::generate(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let stubs_dir = ctx.output.join("stubs");
        fs::create_dir_all(&stubs_dir)?;
        let stub_path = stubs_dir.join("README.md");
        write_file(&stub_path, &stub_result.code)?;
        files.push((stub_path, stub_result));
    }

    if opt_in("wasm") {
        generate_wasm_replica(ctx, cache, files)?;
    }

    if opt_in("ffi") {
        generate_ffi_engine(ctx, cache, files, FfiReason::Declared)?;
    }

    if opt_in("rust-sdk") {
        generate_rust_sdk(ctx, files)?;
    }
    if opt_in("python-sdk") {
        generate_python_sdk(ctx, files)?;
    }
    if opt_in("go-sdk") {
        generate_go_sdk(ctx, files)?;
    }

    if opt_in("napi") {
        generate_napi_binding(ctx, cache, files)?;
    }
    if opt_in("pyo3") {
        generate_pyo3_binding(ctx, cache, files)?;
    }
    if opt_in("go") {
        generate_go_binding(ctx, cache, files)?;
    }

    Ok(())
}

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
    write_file(&path, &core_lib)?;
    files.push((
        path,
        forgedb_codegen::GeneratedCode {
            code: core_lib.clone(),
            description: result.description,
        },
    ));
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
    write_file(&path, &result.code)?;
    let core_pkg = ctx.naming.package(&crate::naming::PackageKind::Core);
    let server_pkg = ctx.naming.package(&crate::naming::PackageKind::Server);
    cache.server = Some(PackagePlan::new(
        crate::naming::PackageKind::Server,
        vec![
            (
                "Cargo.toml".to_string(),
                forgedb_codegen::ServerPackage::cargo_toml(&server_pkg, &core_pkg),
            ),
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

fn generate_ffi_engine(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
    reason: FfiReason,
) -> Result<()> {
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
        &[("build.rs", PyO3Generator::build_rs_scaffold().to_string())],
    );
    Ok(())
}

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
        &[("build.rs", NapiGenerator::build_rs_scaffold().to_string())],
    );
    Ok(())
}

fn generate_wasm_replica(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    if cache.wrapper(&crate::naming::PackageKind::Wasm).is_some() {
        return Ok(());
    }
    ensure_database(ctx, cache, files)?;

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

    let client_dir = ctx.output.join("replica").join("client");
    fs::create_dir_all(&client_dir)?;
    let client_result = WasmGenerator::generate_client(ctx.schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let client_path = client_dir.join("replica-client.ts");
    write_file(&client_path, &client_result.code)?;
    files.push((client_path, client_result));

    let worker_path = client_dir.join("replica-worker.js");
    write_file(
        &worker_path,
        &WasmGenerator::worker_bootstrap_with_config(ctx.gen_config),
    )?;
    ui::info(&format!(
        "  ✓ {} (static worker bootstrap)",
        worker_path.display()
    ));

    Ok(())
}

fn generate_go_binding(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    generate_ffi_engine(ctx, cache, files, FfiReason::ForGo)?;

    let go_dir = ctx.output.join("go");
    fs::create_dir_all(&go_dir)?;

    let fingerprint = cache
        .fingerprint(&crate::naming::PackageKind::Ffi)
        .ok_or_else(|| {
            CliError::CodeGeneration(
                "the Go binding needs the FFI package's source fingerprint, and no FFI \
                 package was planned. This is a ForgeDB bug; please report it."
                    .to_string(),
            )
        })?;

    let go_result = GoGenerator::generate(ctx.schema, &ctx.naming.symbol_prefix, &fingerprint)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let go_path = go_dir.join("forgedb.go");
    write_file(&go_path, &go_result.code)?;
    files.push((go_path, go_result));

    let async_result = GoGenerator::generate_async_bridge(&ctx.naming.symbol_prefix);
    let async_path = go_dir.join("forgedb_async.go");
    write_file(&async_path, &async_result.code)?;
    files.push((async_path, async_result));

    let needs_arrow = GoGenerator::needs_arrow(ctx.schema);
    if let Some(arrow_result) = GoGenerator::generate_arrow(ctx.schema, &ctx.naming.symbol_prefix) {
        let arrow_path = go_dir.join("forgedb_arrow.go");
        write_file(&arrow_path, &arrow_result.code)?;
        files.push((arrow_path, arrow_result));
        ui::warning(
            "the Go Arrow export uses the external module `github.com/apache/arrow-go/v18` \
             (added to go.mod) — run `go mod tidy` in the `go/` dir before `go build`",
        );
    }

    let header_result =
        FfiGenerator::generate_header(ctx.schema, &ctx.naming.symbol_prefix, &fingerprint)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let header_path = go_dir.join("forgedb.h");
    write_file(&header_path, &header_result.code)?;
    files.push((header_path, header_result));

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
    write_file(&lib_path, &result.code)?;
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

fn generate_python_sdk(
    ctx: &Emit<'_>,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    let sdk_dir = ctx.output.join("python-sdk");
    fs::create_dir_all(&sdk_dir)?;

    let result = PythonSdkGenerator::generate(ctx.schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let py_path = sdk_dir.join("forgedb_client.py");
    write_file(&py_path, &result.code)?;
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

fn generate_go_sdk(
    ctx: &Emit<'_>,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    let sdk_dir = ctx.output.join("go-sdk");
    fs::create_dir_all(&sdk_dir)?;

    let result =
        GoSdkGenerator::generate(ctx.schema).map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let go_path = sdk_dir.join("client.go");
    write_file(&go_path, &result.code)?;
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

fn write_file(path: &PathBuf, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, content)?;

    Ok(())
}

#[cfg(test)]
pub fn resolve_target_for_test(raw: &str, mode: Option<GenerateMode>) -> Result<String> {
    resolve_target(raw, mode)
}

fn resolve_target(raw: &str, mode: Option<GenerateMode>) -> Result<String> {
    let target = raw.to_lowercase();

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

    match (target.as_str(), mode) {
        ("node", GenerateMode::Sdk) | ("bun", GenerateMode::Sdk) => Ok("typescript".to_string()),
        ("node", GenerateMode::Runtime) | ("bun", GenerateMode::Runtime) => Ok("napi".to_string()),
        ("python", GenerateMode::Runtime) => Ok("pyo3".to_string()),
        ("go", GenerateMode::Runtime) => Ok("go".to_string()),
        ("python", GenerateMode::Sdk) => Ok("python-sdk".to_string()),
        ("go", GenerateMode::Sdk) => Ok("go-sdk".to_string()),
        ("browser", GenerateMode::Replica) => Ok("wasm".to_string()),

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
    Ok(crate::project::find_schema(None)?.display().to_string())
}

pub const CORE_SUBSTRATE_REEXPORTS: &str = "\n\
pub use forgedb_changefeed;\n\
pub use forgedb_storage;\n\
pub use forgedb_types;\n";

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

fn emit_cache_packages(container: &Path, cache: &CacheEmission) -> Result<()> {
    let Some(core) = cache.core.as_ref() else {
        return Ok(());
    };

    write_plan(container, core)?;

    if let Some(server) = cache.server.as_ref() {
        write_plan(container, server)?;
    }

    for wrapper in &cache.wrappers {
        write_plan(container, wrapper)?;
    }

    Ok(())
}

fn write_plan(container: &Path, plan: &PackagePlan) -> Result<()> {
    let dir = container.join(plan.kind.dir());
    write_core_package(&dir, &plan.files)?;
    ui::detail(&format!("  ✓ {} (cache package)", dir.display()));
    Ok(())
}

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

fn emit_consumer_shims(
    ctx: &Emit<'_>,
    cache: &mut CacheEmission,
    files: &mut Vec<(PathBuf, forgedb_codegen::GeneratedCode)>,
) -> Result<()> {
    use crate::naming::PackageKind;

    let gitignore = ctx.output.join(".gitignore");
    fs::write(&gitignore, OUTPUT_GITIGNORE)?;
    files.push((
        gitignore,
        forgedb_codegen::GeneratedCode {
            code: OUTPUT_GITIGNORE.to_string(),
            description: "ignore rules for the delivered artifacts".to_string(),
        },
    ));

    if let Some(fp) = plan_fingerprint(cache, &PackageKind::Napi) {
        let dir = ctx.output.join(PackageKind::Napi.dir());
        fs::create_dir_all(&dir)?;

        let entry = NapiGenerator::entry_module(&fp);
        let entry_path = dir.join("index.js");
        write_file(&entry_path, &entry.code)?;
        files.push((entry_path, entry));

        let dts = NapiGenerator::type_declarations(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let dts_path = dir.join("index.d.ts");
        write_file(&dts_path, &dts.code)?;
        files.push((dts_path, dts));

        reconcile_napi_package_json(&dir)?;
    }

    if let Some(fp) = plan_fingerprint(cache, &PackageKind::Pyo3) {
        let dir = ctx.output.join(PackageKind::Pyo3.dir());
        fs::create_dir_all(&dir)?;

        let module = PyO3Generator::python_module(ctx.schema, &fp)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let module_path = dir.join("forgedb.py");
        write_file(&module_path, &module.code)?;
        files.push((module_path, module));

        let stub = PyO3Generator::type_stub(ctx.schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let stub_path = dir.join("forgedb.pyi");
        write_file(&stub_path, &stub.code)?;
        files.push((stub_path, stub));
    }

    let ffi_fp = plan_fingerprint(cache, &PackageKind::Ffi);
    if cache.ffi_declared && let Some(fp) = ffi_fp {
        let dir = ctx.output.join(PackageKind::Ffi.dir());
        fs::create_dir_all(&dir)?;
        let header = FfiGenerator::generate_header(ctx.schema, &ctx.naming.symbol_prefix, &fp)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let header_path = dir.join("forgedb.h");
        write_file(&header_path, &header.code)?;
        files.push((header_path, header));
    }

    Ok(())
}

fn plan_fingerprint(
    cache: &mut CacheEmission,
    kind: &crate::naming::PackageKind,
) -> Option<String> {
    let value = cache.fingerprint(kind)?;
    let plan = cache.wrapper_mut(kind)?;
    if !plan.files.iter().any(|(rel, _)| rel == crate::fingerprint::FINGERPRINT_FILE) {
        plan.push(
            crate::fingerprint::FINGERPRINT_FILE,
            crate::fingerprint::fingerprint_rs(&value),
        );
    }
    Some(value)
}

fn reconcile_napi_package_json(dir: &Path) -> Result<()> {
    let path = dir.join("package.json");
    let Ok(existing) = fs::read_to_string(&path) else {
        fs::write(&path, NapiGenerator::package_json_scaffold())?;
        ui::info(&format!("  ✓ {} (npm binding scaffold)", path.display()));
        return Ok(());
    };

    let Ok(mut doc) = serde_json::from_str::<serde_json::Value>(&existing) else {
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

const MOVED_PACKAGES: [&str; 4] = ["ffi", "napi", "pyo3", "replica"];

const MOVED_PACKAGE_FILES: [&str; 2] = ["lib.rs", "database.rs"];

fn supersession_text(package: &str) -> String {
    format!(
        "// Superseded by ForgeDB. This file is NO LONGER GENERATED here.\n\
         compile_error!(\"ForgeDB no longer generates the `{package}` package here — it moved into the ForgeDB build cache. Run `forgedb build`.\");\n"
    )
}

fn supersede_moved_packages(output_dir: &Path) -> Result<()> {
    for package in MOVED_PACKAGES {
        let want = supersession_text(package);
        for file in MOVED_PACKAGE_FILES {
            let path = output_dir.join(package).join("src").join(file);
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

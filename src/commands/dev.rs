//! `forgedb dev` — watch the schema and regenerate through `commands::generate`.
//!
//! # Why this file no longer has a generator in it (#364, #335 §16)
//!
//! `dev` used to hand a schema path and an output directory straight to
//! `forgedb_watcher::auto_watch`, whose `SchemaRegenerator` ran four generators
//! of its own.  Those generators read no `forgedb.toml`: `RustGenerator::generate`
//! hardcodes `schema_version = 1` and `GenConfig::DEFAULT`.  So on any project
//! with a `[storage]`/`[runtime]` knob set, or with a migration lineage, a single
//! save in `dev` overwrote a correct `database.rs` with a **different database** —
//! different durability, different cascade depth, and an open guard that refuses
//! the data dir the app is running against.
//!
//! `dev` now resolves exactly what `Commands::Generate` resolves, from the same
//! upward walk, and every regeneration is a call to [`generate::run`].  There is
//! no second emission path left to drift.
//!
//! The watcher keeps the half only it can do — debounce, the watch loop, and the
//! parse check that keeps a broken save from reaching the generator.

use crate::commands::generate;
use crate::{error::CliError, ui, Result};
use forgedb_watcher::{auto_watch, RegenerateResult};
use std::path::{Path, PathBuf};

/// Everything one `dev` regeneration needs, resolved **once** by the caller.
///
/// Cloned into the watcher callback, which is why it is separate from
/// [`DevOptions`]: the sync hook is a `Box<dyn Fn>` and cannot be `Clone`.
#[derive(Clone)]
pub struct DevGenerate {
    /// The resolved schema path — the one `project::find_schema` found, not
    /// clap's `"schema.forge"` default.
    pub schema: String,
    /// The resolved output directory — `--output` > `[generate].output` >
    /// the schema-relative built-in default. Never clap's raw `"generated"`.
    pub output: String,
    /// Canonical `[generate].targets`, already validated and revocabularied.
    pub config_targets: Vec<String>,
    /// The `[runtime]`/`[storage]` knobs baked into `database.rs` — the value
    /// whose absence was #364.
    pub gen_config: forgedb_codegen::GenConfig,
    /// The app's container in the build cache, reserved by the caller.
    pub cache_container: Option<PathBuf>,
}

/// Re-derive the cache workspace root after an emission.
///
/// `dev` never returns — it blocks in the watch loop until Ctrl+C — so the
/// caller cannot run its own `sync_after_emission` "after `dev::run`".  The step
/// is handed in instead of reimplemented here, so the de-list/prune/render order
/// keeps exactly one definition (#335 §3).
pub type SyncHook = Box<dyn Fn() -> Result<()> + Send>;

pub struct DevOptions {
    pub generate: DevGenerate,
    pub debounce: u64,
    pub clear: bool,
    pub sync: SyncHook,
}

/// Run one regeneration — the same call `forgedb generate` makes.
///
/// `target: "all"` with the app's `config_targets` is what makes this the
/// *whole* declared artifact set rather than the watcher's old hardcoded four.
/// `force: true` because a watch loop by definition rewrites files that already
/// exist; without it the second save fails with "use --force to overwrite".
fn regenerate(opts: &DevGenerate) -> Result<()> {
    generate::run(generate::GenerateOptions {
        target: "all".to_string(),
        mode: None,
        check: false,
        output: Some(opts.output.clone()),
        schema: Some(opts.schema.clone()),
        config_targets: Some(opts.config_targets.clone()),
        gen_config: opts.gen_config,
        force: true,
        from: None,
        to: None,
        cache_container: opts.cache_container.clone(),
    })
}

pub fn run(options: DevOptions) -> Result<()> {
    // Resolved by the caller, so this is a real existence check on a real path
    // rather than on clap's default (#333: the schema names itself).
    let schema_path = Path::new(&options.generate.schema);
    if !schema_path.exists() {
        return Err(CliError::SchemaNotFound(format!(
            "Schema file not found: {}",
            options.generate.schema
        )));
    }

    // Print initial header
    ui::header("👁️", "ForgeDB Dev Mode");
    println!();
    ui::info(&format!("Watching: {}", options.generate.schema));
    ui::info(&format!("Output:   {}", options.generate.output));
    ui::info(&format!("Debounce: {}ms", options.debounce));
    println!();
    ui::info("Press Ctrl+C to stop watching");
    println!();
    println!("{}", "─".repeat(60));
    println!();

    let clear_terminal = options.clear;
    let emission = options.generate.clone();
    let sync = options.sync;
    // The watcher reports whether the changed schema PARSES; the generating is
    // this callback's, which is the whole of #364's fix. A schema that did not
    // parse must not reach the generator — regenerating from the last good parse
    // would silently ship code for a file the user is mid-edit on.
    let callback = Box::new(move |result: &RegenerateResult| {
        if clear_terminal {
            clear_screen();
        }

        if !result.success {
            ui::error(&result.message);
        } else {
            match regenerate(&emission).and_then(|()| sync()) {
                Ok(()) => ui::success("Regenerated"),
                Err(e) => ui::error(&format!("Generation failed: {}", e)),
            }
        }

        println!();
        ui::info("Waiting for changes...");
        println!();
    });

    // Run watcher (blocks until Ctrl+C). `output` still rides along because the
    // published `auto_watch` signature takes it; nothing in the watcher writes
    // there any more.
    auto_watch(
        &options.generate.schema,
        &options.generate.output,
        options.debounce,
        Some(callback),
    )
    .map_err(|e| CliError::Other(format!("Watcher error: {}", e)))?;

    Ok(())
}

/// Clear the terminal screen
fn clear_screen() {
    // ANSI escape codes for clearing screen
    print!("\x1B[2J\x1B[1;1H");
}

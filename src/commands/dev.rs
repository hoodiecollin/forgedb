use crate::commands::generate;
use crate::{error::CliError, ui, Result};
use forgedb_watcher::{auto_watch, RegenerateResult};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct DevGenerate {
    pub schema: String,
    pub output: String,
    pub config_targets: Vec<String>,
    pub gen_config: forgedb_codegen::GenConfig,
    pub cache_container: Option<PathBuf>,
    pub in_tree: Option<PathBuf>,
}

pub type SyncHook = Box<dyn Fn() -> Result<()> + Send>;

pub struct DevOptions {
    pub generate: DevGenerate,
    pub debounce: u64,
    pub clear: bool,
    pub sync: SyncHook,
}

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
        in_tree: opts.in_tree.clone(),
    })
}

pub fn run(options: DevOptions) -> Result<()> {
    let schema_path = Path::new(&options.generate.schema);
    if !schema_path.exists() {
        return Err(CliError::SchemaNotFound(format!(
            "Schema file not found: {}",
            options.generate.schema
        )));
    }

    ui::header("👁️", "ForgeDB Dev Mode");
    ui::blank();
    ui::info(&format!("Watching: {}", options.generate.schema));
    ui::info(&format!("Output:   {}", options.generate.output));
    ui::info(&format!("Debounce: {}ms", options.debounce));
    ui::blank();
    ui::info("Press Ctrl+C to stop watching");
    ui::blank();
    ui::line(&"─".repeat(60));
    ui::blank();

    let clear_terminal = options.clear;
    let emission = options.generate.clone();
    let sync = options.sync;
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

        ui::blank();
        ui::info("Waiting for changes...");
        ui::blank();
    });

    crate::ask::forbid();

    auto_watch(
        &options.generate.schema,
        &options.generate.output,
        options.debounce,
        Some(callback),
    )
    .map_err(|e| CliError::Other(format!("Watcher error: {}", e)))?;

    Ok(())
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

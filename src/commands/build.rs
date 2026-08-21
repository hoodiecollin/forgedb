use crate::{error::CliError, ui, Result};
use std::process::Command;

pub struct BuildOptions {
    pub release: bool,
    pub target: String,
    pub output: Option<String>,
    pub schema: Option<String>,
    pub no_api: bool,
    /// Generate-time runtime-behavior config (epic #126), resolved by the caller
    /// from the **one** config the CLI loaded.
    ///
    /// This is passed in rather than loaded here (#361): `build` used to call
    /// the config itself, so a single invocation was served by two
    /// different files — `--config` for the output/schema paths and whatever sat
    /// in the working directory for every `[runtime]`/`[storage]` knob. Taking it
    /// as an argument is what makes that unrepresentable; two loaders that merely
    /// agree today is the same bug waiting.
    pub gen_config: forgedb_codegen::GenConfig,
    /// The project's declared target set, as canonical internal names (#335 §12).
    ///
    /// This used to be hardcoded `None` at both call sites below, which made
    /// every opt-in arm of `generate_all` — `ffi`, the browser replica, the
    /// three REST SDKs, and (once they existed) the three native bindings —
    /// unreachable from `forgedb build`. `build` could not reach a target the
    /// project had explicitly declared.
    pub config_targets: Vec<String>,
    /// The app's container in the build cache, reserved by the caller.
    pub cache_container: Option<std::path::PathBuf>,
}

pub fn run(options: BuildOptions) -> Result<()> {
    ui::header("🔨", "Building production artifacts");

    // First, validate the schema
    ui::info("Validating schema...");
    crate::commands::validate::run(crate::commands::validate::ValidateOptions {
        strict: false,
        schema_only: true,
        implementations: false,
        components: false,
        schema: options.schema.clone(),
    })?;

    // Generate code — respect the --no-api flag by running only the targets we want.
    ui::info("Generating code...");
    if options.no_api {
        ui::info("Skipping API generation (--no-api)");
        // (target, mode) pairs in the #122 taxonomy — the TS SDK is now
        // `node --sdk`, the rest are standalone artifacts.
        let targets: &[(&str, Option<crate::commands::generate::GenerateMode>)] = &[
            ("rust", None),
            ("node", Some(crate::commands::generate::GenerateMode::Sdk)),
            ("stubs", None),
        ];
        for (target, mode) in targets {
            crate::commands::generate::run(crate::commands::generate::GenerateOptions {
                target: target.to_string(),
                mode: *mode,
                check: false,
                output: options.output.clone(),
                schema: options.schema.clone(),
                config_targets: Some(options.config_targets.clone()),
                cache_container: options.cache_container.clone(),
                gen_config: options.gen_config,
                force: true,
                from: None,
                to: None,
            })?;
        }
    } else {
        // Build always regenerates derived artifacts — pass force: true so a
        // second consecutive `build` does not fail on "File exists".
        crate::commands::generate::run(crate::commands::generate::GenerateOptions {
            target: "all".to_string(),
            mode: None,
            check: false,
            output: options.output.clone(),
            schema: options.schema.clone(),
            config_targets: Some(options.config_targets.clone()),
            cache_container: options.cache_container.clone(),
            gen_config: options.gen_config,
            force: true,
            from: None,
            to: None,
        })?;
    }

    // Build based on target
    match options.target.as_str() {
        "native" => build_native(&options)?,
        "wasm" => build_wasm(&options)?,
        "both" => {
            build_native(&options)?;
            build_wasm(&options)?;
        }
        target => {
            return Err(CliError::Build(format!("Unknown target: {}", target)));
        }
    }

    ui::success("Build complete!");
    println!();
    println!("Artifacts:");
    if let Some(output) = &options.output {
        println!("  Output directory: {}/", output);
    } else {
        println!(
            "  Output directory: target/{}/",
            if options.release { "release" } else { "debug" }
        );
    }

    Ok(())
}

fn build_native(options: &BuildOptions) -> Result<()> {
    ui::info("Building Rust native binary...");

    let mut cmd = Command::new("cargo");
    cmd.arg("build");

    if options.release {
        cmd.arg("--release");
    }

    let output = cmd
        .output()
        .map_err(|e| CliError::Build(format!("Failed to run cargo build: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Build(format!("Compilation failed:\n{}", stderr)));
    }

    ui::success("Compiled database (native)");
    Ok(())
}

fn build_wasm(options: &BuildOptions) -> Result<()> {
    ui::info("Building WebAssembly target...");

    // Check if wasm32-unknown-unknown target is installed
    let check_target = Command::new("rustup")
        .args(&["target", "list", "--installed"])
        .output()
        .map_err(|e| CliError::Build(format!("Failed to check Rust targets: {}", e)))?;

    let installed_targets = String::from_utf8_lossy(&check_target.stdout);
    if !installed_targets.contains("wasm32-unknown-unknown") {
        ui::warning("wasm32-unknown-unknown target not installed");
        ui::info("Installing wasm32-unknown-unknown target...");

        let install = Command::new("rustup")
            .args(&["target", "add", "wasm32-unknown-unknown"])
            .status()
            .map_err(|e| CliError::Build(format!("Failed to install WASM target: {}", e)))?;

        if !install.success() {
            return Err(CliError::Build(
                "Failed to install wasm32-unknown-unknown target".to_string(),
            ));
        }
    }

    // Build for WASM
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    cmd.arg("--target").arg("wasm32-unknown-unknown");

    if options.release {
        cmd.arg("--release");
    }

    let output = cmd
        .output()
        .map_err(|e| CliError::Build(format!("Failed to run cargo build for WASM: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Build(format!(
            "WASM compilation failed:\n{}",
            stderr
        )));
    }

    ui::success("Compiled database (wasm)");
    Ok(())
}

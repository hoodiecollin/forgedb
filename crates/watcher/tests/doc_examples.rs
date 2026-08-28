use forgedb_watcher::{SchemaRegenerator, SchemaWatcher, auto_watch};
use std::path::Path;

#[allow(dead_code)]
fn watch_then_regenerate_compiles() -> Result<(), Box<dyn std::error::Error>> {
    let mut watcher = SchemaWatcher::new(200)?;
    watcher.watch(Path::new("./schema.forge"))?;

    let regenerator = SchemaRegenerator::new(Path::new("./schema.forge"), Path::new("./generated"));
    let result = regenerator.regenerate();
    let _ = (result.success, result.message);
    Ok(())
}

#[allow(dead_code)]
fn regenerate_result_fields_compile() {
    let regenerator = SchemaRegenerator::new(Path::new("./schema.forge"), Path::new("./generated"));

    let result = regenerator.regenerate();
    if result.success {
        let _ = result.output_path;
    } else {
        let _ = result.message;
    }
}

#[allow(dead_code)]
fn auto_watch_with_callback_compiles() {
    auto_watch(
        "schema.forge",
        "generated",
        200,
        Some(Box::new(|result| {
            let _ = (result.success, &result.message, &result.output_path);
        })),
    )
    .expect("Failed to start watcher");
}

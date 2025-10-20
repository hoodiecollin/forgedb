use forgedb_watcher::*;
use std::path::Path;

#[test]
fn test_regenerator_creation() {
    let regen = SchemaRegenerator::new("schema.forge", "generated");
    assert_eq!(regen.schema_path(), Path::new("schema.forge"));
    assert_eq!(regen.output_dir(), Path::new("generated"));
}

#[test]
fn test_regenerate_missing_file() {
    let regen = SchemaRegenerator::new("/nonexistent/schema.forge", "generated");
    let result = regen.regenerate();
    assert!(!result.success);
    assert!(result.message.contains("not found"));
}

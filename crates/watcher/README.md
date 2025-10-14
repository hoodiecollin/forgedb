# SinkDB Watcher

File watching and auto-regeneration for SinkDB schemas.

## Overview

The `sinkdb-watcher` crate provides file system watching capabilities for SinkDB schema files. When a schema file changes, the watcher automatically triggers code regeneration, providing a seamless development experience.

## Features

- **File watching** - Monitor schema files for changes using the `notify` crate
- **Debouncing** - Configurable debounce period to avoid redundant regenerations
- **Auto-regeneration** - Automatically parse and generate code when schemas change
- **Error handling** - Clear, actionable error messages for common issues
- **Cross-platform** - Works on macOS, Linux, and Windows

## Usage

### Basic Example

```rust
use sinkdb_watcher::auto_watch;

fn main() {
    // Watch schema.sink and regenerate to generated/ directory
    // with 200ms debounce
    auto_watch(
        "schema.sink",
        "generated",
        200,
        Some(Box::new(|result| {
            if result.success {
                println!("✓ {}", result.message);
            } else {
                eprintln!("✗ {}", result.message);
            }
        }))
    ).expect("Failed to start watcher");
}
```

### Advanced Usage

```rust
use sinkdb_watcher::{SchemaWatcher, SchemaRegenerator};

fn main() {
    let mut watcher = SchemaWatcher::new(200).unwrap();
    let regenerator = SchemaRegenerator::new("schema.sink", "generated");

    watcher.watch("schema.sink").unwrap();

    loop {
        match watcher.next_event() {
            Ok(event) => {
                println!("Change detected: {:?}", event.kind);
                let result = regenerator.regenerate();
                println!("{}", result.message);
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
```

## API

### SchemaWatcher

Core file watching functionality.

- `new(debounce_ms: u64)` - Create a new watcher with debounce period
- `watch(path)` - Start watching a file
- `unwatch(path)` - Stop watching a file
- `next_event()` - Block until next change (with debouncing)
- `try_next_event()` - Non-blocking event check

### SchemaRegenerator

Handles code regeneration from schema changes.

- `new(schema_path, output_dir)` - Create a regenerator
- `regenerate()` - Parse schema and generate code
- `schema_path()` - Get the watched schema path
- `output_dir()` - Get the output directory

### auto_watch

Convenience function combining watching and regeneration.

```rust
pub fn auto_watch<P, Q>(
    schema_path: P,
    output_dir: Q,
    debounce_ms: u64,
    callback: Option<Box<dyn Fn(&RegenerateResult) + Send>>,
) -> Result<(), WatchError>
```

## Testing

```bash
# Run all tests
cargo test -p sinkdb-watcher

# Run with output
cargo test -p sinkdb-watcher -- --nocapture
```

## Dependencies

- `notify` - Cross-platform file system notifications
- `crossbeam-channel` - High-performance channels for event communication
- `sinkdb` - Parser and code generation

## Documentation

See [SPRINT5_WATCHER.md](../../SPRINT5_WATCHER.md) for complete documentation.

## License

Part of the SinkDB project.

//! ForgeDB Watcher
//!
//! File system watching and automatic schema regeneration for ForgeDB development workflow.
//!
//! # Overview
//!
//! This crate provides file system watching capabilities for ForgeDB, enabling automatic
//! code regeneration when schema files change. It is primarily used by the ForgeDB CLI's
//! watch mode to provide a seamless development experience.
//!
//! # Architecture
//!
//! The watcher consists of two main components:
//!
//! - **File Watcher** - Monitors file system for changes using the `notify` crate
//! - **Schema Regenerator** - Triggers code regeneration when schema files change
//!
//! ## Watch Flow
//!
//! 1. **Monitor Files** - Watch schema directory for changes
//! 2. **Detect Changes** - Identify created, modified, or deleted files
//! 3. **Debounce Events** - Group rapid changes to avoid duplicate regenerations
//! 4. **Trigger Regeneration** - Parse and regenerate code from updated schemas
//! 5. **Report Results** - Provide feedback on success or errors
//!
//! # Examples
//!
//! ## Basic File Watching
//!
//! ```rust,no_run
//! use forgedb_watcher::{SchemaRegenerator, FileChangeEvent, ChangeKind};
//! use std::path::Path;
//!
//! // Create a regenerator
//! let regenerator = SchemaRegenerator::new(
//!     Path::new("./schemas"),
//!     Path::new("./generated")
//! );
//!
//! // Watch for schema changes
//! regenerator.watch(|event| {
//!     match event.kind {
//!         ChangeKind::Modified => {
//!             println!("Schema modified: {:?}", event.path);
//!             // Regeneration happens automatically
//!         }
//!         ChangeKind::Created => {
//!             println!("Schema created: {:?}", event.path);
//!         }
//!         ChangeKind::Removed => {
//!             println!("Schema removed: {:?}", event.path);
//!         }
//!     }
//! })?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Manual Regeneration
//!
//! ```rust,no_run
//! use forgedb_watcher::SchemaRegenerator;
//! use std::path::Path;
//!
//! let regenerator = SchemaRegenerator::new(
//!     Path::new("./schemas"),
//!     Path::new("./generated")
//! );
//!
//! // Trigger regeneration manually
//! match regenerator.regenerate() {
//!     Ok(_) => println!("Regeneration successful"),
//!     Err(e) => eprintln!("Regeneration failed: {}", e),
//! }
//! ```
//!
//! ## With Custom Callback
//!
//! ```rust,no_run
//! use forgedb_watcher::{SchemaRegenerator, FileChangeEvent};
//! use std::path::Path;
//!
//! let regenerator = SchemaRegenerator::new(
//!     Path::new("./schemas"),
//!     Path::new("./generated")
//! );
//!
//! // Watch with custom callback
//! regenerator.watch_with_callback(|event: &FileChangeEvent| {
//!     println!("File changed: {:?} at {:?}",
//!         event.path,
//!         event.timestamp
//!     );
//!
//!     // Custom handling logic here
//!     Ok(())
//! })?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`SchemaRegenerator`] - Manages schema watching and regeneration
//! - [`FileChangeEvent`] - Represents a file system change event
//! - [`ChangeKind`] - Type of file change (Modified, Created, Removed)
//!
//! ## Error Types
//!
//! - [`WatchError`] - Errors during file watching
//! - [`RegenerateError`] - Errors during schema regeneration
//! - [`RegenerateResult`] - Result type for regeneration operations
//!
//! ## Key Methods
//!
//! - `SchemaRegenerator::new()` - Create a new regenerator
//! - `SchemaRegenerator::watch()` - Start watching for changes
//! - `SchemaRegenerator::regenerate()` - Manually trigger regeneration
//! - `SchemaRegenerator::stop()` - Stop watching
//!
//! # Debouncing
//!
//! The watcher includes built-in debouncing to handle rapid file changes:
//!
//! - Multiple changes within 500ms are grouped together
//! - Prevents duplicate regenerations from text editor auto-save
//! - Reduces CPU usage during active editing
//!
//! # Use Cases
//!
//! ## Development Workflow
//!
//! ```bash
//! # CLI usage
//! forgedb watch
//! ```
//!
//! This enables:
//! - Edit schema files in your editor
//! - Changes automatically detected
//! - Code regenerated instantly
//! - Continue development without manual steps
//!
//! ## CI/CD Integration
//!
//! While primarily for development, the watcher can also be used in CI:
//! - Validate schemas on commit
//! - Ensure generated code is up-to-date
//! - Detect schema changes in PRs
//!
//! # Performance
//!
//! - **Low overhead**: File watching uses efficient OS-level APIs
//! - **Fast regeneration**: Only affected files are processed
//! - **Debouncing**: Prevents excessive regeneration during editing
//! - **Non-blocking**: Watch runs in background, doesn't block main thread
//!
//! # Related Crates
//!
//! - [`forgedb-parser`](../forgedb_parser) - Parses schema files
//! - [`forgedb-validation`](../forgedb_validation) - Validates schemas before regeneration
//! - [`forgedb`](../../) - Main CLI that uses this watcher
//!
//! # Dependencies
//!
//! - [`notify`](https://docs.rs/notify) - Cross-platform file system watching
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation
//! - [SPRINT3_WATCH_MODE.md](../../archive/sprint-summaries/SPRINT3_WATCH_MODE.md) - Watch mode implementation

pub mod regenerator;

use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher,
};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub use regenerator::{RegenerateError, RegenerateResult, SchemaRegenerator};

/// Error types for the watcher
#[derive(Debug)]
pub enum WatchError {
    NotifyError(notify::Error),
    InvalidPath(String),
}

impl From<notify::Error> for WatchError {
    fn from(err: notify::Error) -> Self {
        WatchError::NotifyError(err)
    }
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatchError::NotifyError(e) => write!(f, "File watcher error: {}", e),
            WatchError::InvalidPath(p) => write!(f, "Invalid path: {}", p),
        }
    }
}

impl std::error::Error for WatchError {}

/// Represents a file change event
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: std::path::PathBuf,
    pub kind: ChangeKind,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    Modified,
    Created,
    Removed,
}

/// File watcher with debouncing support
pub struct SchemaWatcher {
    watcher: RecommendedWatcher,
    receiver: Receiver<Result<Event, notify::Error>>,
    debounce_duration: Duration,
}

impl SchemaWatcher {
    /// Create a new schema file watcher
    pub fn new(debounce_ms: u64) -> Result<Self, WatchError> {
        let (tx, rx) = channel();

        let watcher = RecommendedWatcher::new(
            move |res| {
                if let Err(_) = tx.send(res) {
                    // Receiver dropped, watcher should stop
                }
            },
            Config::default(),
        )?;

        Ok(SchemaWatcher {
            watcher,
            receiver: rx,
            debounce_duration: Duration::from_millis(debounce_ms),
        })
    }

    /// Watch a specific file or directory
    pub fn watch<P: AsRef<Path>>(&mut self, path: P) -> Result<(), WatchError> {
        let path_ref = path.as_ref();

        if !path_ref.exists() {
            return Err(WatchError::InvalidPath(format!(
                "Path does not exist: {}",
                path_ref.display()
            )));
        }

        self.watcher.watch(path_ref, RecursiveMode::NonRecursive)?;
        Ok(())
    }

    /// Stop watching a specific path
    pub fn unwatch<P: AsRef<Path>>(&mut self, path: P) -> Result<(), WatchError> {
        self.watcher.unwatch(path.as_ref())?;
        Ok(())
    }

    /// Wait for the next file change event (with debouncing)
    ///
    /// This method will block until a file change is detected, then wait for
    /// the debounce period to elapse before returning the event. This ensures
    /// that rapid consecutive changes are coalesced into a single event.
    pub fn next_event(&self) -> Result<FileChangeEvent, WatchError> {
        loop {
            // Wait for first event
            let event = self
                .receiver
                .recv()
                .map_err(|_| WatchError::NotifyError(notify::Error::generic("Channel closed")))?
                .map_err(WatchError::NotifyError)?;

            let (path, kind) = match self.extract_event_info(&event) {
                Some(info) => info,
                None => continue, // Ignore events we don't care about
            };

            // Debounce: wait for debounce period and drain any additional events
            let deadline = Instant::now() + self.debounce_duration;
            let mut last_event_time = Instant::now();
            let mut latest_kind = kind;

            loop {
                let now = Instant::now();
                if now >= deadline && now.duration_since(last_event_time) >= self.debounce_duration
                {
                    // Debounce period elapsed with no new events
                    break;
                }

                let timeout = deadline.saturating_duration_since(now);

                match self.receiver.recv_timeout(timeout) {
                    Ok(Ok(evt)) => {
                        // Got another event, update timestamp and possibly kind
                        if let Some((evt_path, evt_kind)) = self.extract_event_info(&evt) {
                            if evt_path == path {
                                last_event_time = Instant::now();
                                latest_kind = evt_kind;
                            }
                        }
                    }
                    Ok(Err(_)) => continue,
                    Err(_) => break, // Timeout or channel error, we're done debouncing
                }
            }

            return Ok(FileChangeEvent {
                path,
                kind: latest_kind,
                timestamp: Instant::now(),
            });
        }
    }

    /// Try to get the next event without blocking
    pub fn try_next_event(&self) -> Result<Option<FileChangeEvent>, WatchError> {
        match self.receiver.try_recv() {
            Ok(Ok(event)) => {
                if let Some((path, kind)) = self.extract_event_info(&event) {
                    Ok(Some(FileChangeEvent {
                        path,
                        kind,
                        timestamp: Instant::now(),
                    }))
                } else {
                    Ok(None)
                }
            }
            Ok(Err(e)) => Err(WatchError::NotifyError(e)),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(WatchError::NotifyError(
                notify::Error::generic("Channel closed"),
            )),
        }
    }

    /// Extract path and change kind from notify event
    fn extract_event_info(&self, event: &Event) -> Option<(std::path::PathBuf, ChangeKind)> {
        let path = event.paths.first()?.clone();

        let kind = match &event.kind {
            EventKind::Create(_) => ChangeKind::Created,
            EventKind::Modify(_) => ChangeKind::Modified,
            EventKind::Remove(_) => ChangeKind::Removed,
            _ => return None,
        };

        Some((path, kind))
    }
}

/// Auto-watch a schema file and regenerate on changes
///
/// This is a convenience function that sets up watching and automatic regeneration.
/// It will block indefinitely, regenerating code every time the schema changes.
///
/// # Arguments
/// * `schema_path` - Path to the schema file to watch
/// * `output_dir` - Directory where generated code should be written
/// * `debounce_ms` - Debounce period in milliseconds (e.g., 200)
/// * `callback` - Optional callback to be notified of regeneration results
///
/// # Example
/// ```no_run
/// use forgedb_watcher::auto_watch;
///
/// auto_watch(
///     "schema.forge",
///     "generated",
///     200,
///     Some(Box::new(|result| {
///         if result.success {
///             println!("✓ {}", result.message);
///         } else {
///             eprintln!("✗ {}", result.message);
///         }
///     }))
/// ).expect("Failed to start watcher");
/// ```
pub fn auto_watch<P: AsRef<Path>, Q: AsRef<Path>>(
    schema_path: P,
    output_dir: Q,
    debounce_ms: u64,
    callback: Option<Box<dyn Fn(&RegenerateResult) + Send>>,
) -> Result<(), WatchError> {
    let schema_path = schema_path.as_ref();
    let regenerator = SchemaRegenerator::new(schema_path, output_dir.as_ref());

    let mut watcher = SchemaWatcher::new(debounce_ms)?;
    watcher.watch(schema_path)?;

    println!("👁  Watching {} for changes...", schema_path.display());
    println!("   Press Ctrl+C to stop\n");

    // Do initial generation
    let result = regenerator.regenerate();
    if let Some(ref cb) = callback {
        cb(&result);
    }

    // Watch for changes
    loop {
        match watcher.next_event() {
            Ok(event) => {
                // Only regenerate if the event is for our schema file
                if event.path == schema_path {
                    println!("\n📝 Schema changed, regenerating...");
                    let result = regenerator.regenerate();

                    if let Some(ref cb) = callback {
                        cb(&result);
                    }
                }
            }
            Err(e) => {
                eprintln!("Watcher error: {}", e);
                return Err(e);
            }
        }
    }
}

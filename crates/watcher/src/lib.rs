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
//! use forgedb_watcher::{SchemaRegenerator, SchemaWatcher};
//! use std::path::Path;
//!
//! // Create a watcher and point it at the schema file.
//! let mut watcher = SchemaWatcher::new(200)?;
//! watcher.watch(Path::new("./schema.forge"))?;
//!
//! // Run the initial generation before entering the watch loop.
//! let regenerator = SchemaRegenerator::new(
//!     Path::new("./schema.forge"),
//!     Path::new("./generated"),
//! );
//! let result = regenerator.regenerate();
//! println!("Initial generation: success={}, msg={}", result.success, result.message);
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
//!     Path::new("./schema.forge"),
//!     Path::new("./generated"),
//! );
//!
//! // regenerate() returns RegenerateResult (not a Result — check the struct fields).
//! let result = regenerator.regenerate();
//! if result.success {
//!     println!("Regeneration successful: {:?}", result.output_path);
//! } else {
//!     eprintln!("Regeneration failed: {}", result.message);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Watching with a Callback
//!
//! ```rust,no_run
//! use forgedb_watcher::auto_watch;
//!
//! // auto_watch blocks indefinitely; interrupt with Ctrl-C.
//! auto_watch(
//!     "schema.forge",
//!     "generated",
//!     200,
//!     Some(Box::new(|result| {
//!         if result.success {
//!             println!("Regenerated: {:?}", result.output_path);
//!         } else {
//!             eprintln!("Error: {}", result.message);
//!         }
//!     })),
//! ).expect("Failed to start watcher");
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
//! - [`RegenerateResult`] - Result of a regeneration attempt
//!
//! ## Key Functions / Methods
//!
//! - [`auto_watch`] - High-level convenience: watch + regenerate loop
//! - [`SchemaWatcher::new`] - Create a low-level watcher
//! - [`SchemaRegenerator::new`] - Create a regenerator
//! - [`SchemaRegenerator::regenerate`] - Manually trigger regeneration
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
//! - [`forgedb`](../../) - Main CLI that uses this watcher
//!
//! # Dependencies
//!
//! - [`notify`](https://docs.rs/notify) - Cross-platform file system watching

pub mod regenerator;

use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher,
};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub use regenerator::{RegenerateCallback, RegenerateError, RegenerateResult, SchemaRegenerator};

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
                if tx.send(res).is_err() {
                    // Receiver was dropped — the watcher should stop soon.
                    tracing::debug!("Watcher event channel closed; stopping event forwarding.");
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
                        if let Some((evt_path, evt_kind)) = self.extract_event_info(&evt)
                            && evt_path == path
                        {
                            last_event_time = Instant::now();
                            latest_kind = evt_kind;
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

/// Auto-watch a schema file and regenerate on changes.
///
/// This is a convenience function that sets up watching and automatic
/// regeneration.  It blocks indefinitely, regenerating code every time the
/// schema file changes.
///
/// ## Path matching
///
/// On macOS, `notify` emits canonicalized absolute paths while the caller
/// often passes a relative path.  This function canonicalizes both sides
/// before comparing, and additionally watches the parent directory (rather than
/// the file directly) to handle atomic-save-rename patterns used by most
/// text editors.
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
    callback: Option<RegenerateCallback>,
) -> Result<(), WatchError> {
    let schema_path = schema_path.as_ref();
    let regenerator = SchemaRegenerator::new(schema_path, output_dir.as_ref());

    // Canonicalize the schema path once for reliable comparison.
    // On macOS, notify emits absolute canonicalized paths; without this the
    // simple equality check `event.path == schema_path` never matches when
    // schema_path is relative (W2 fix).
    let schema_canon = schema_path
        .canonicalize()
        .unwrap_or_else(|_| schema_path.to_path_buf());

    // Watch the parent directory rather than the file directly.  This catches
    // atomic-save-rename events that most editors emit: they write to a temp
    // file and rename it into place, which produces a Created event on the
    // target rather than a Modified event.
    let watch_dir = schema_canon
        .parent()
        .unwrap_or(schema_canon.as_path());
    let schema_file_name = schema_canon.file_name();

    let mut watcher = SchemaWatcher::new(debounce_ms)?;

    if watch_dir.exists() {
        watcher.watch(watch_dir)?;
    } else {
        watcher.watch(schema_path)?;
    }

    println!("Watching {} for changes...", schema_path.display());
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
                // Match by canonicalized path AND by file name (handles
                // atomic-save-rename where the event path differs).
                let event_canon = event
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| event.path.clone());

                let matches = event_canon == schema_canon
                    || schema_file_name
                        .map(|name| event.path.file_name() == Some(name))
                        .unwrap_or(false);

                if matches {
                    println!("\nSchema changed, regenerating...");
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

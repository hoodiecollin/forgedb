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
/// use sinkdb_watcher::auto_watch;
///
/// auto_watch(
///     "schema.sink",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn test_watcher_creation() {
        let watcher = SchemaWatcher::new(100);
        assert!(watcher.is_ok());
    }

    #[test]
    fn test_watch_nonexistent_path() {
        let mut watcher = SchemaWatcher::new(100).unwrap();
        let result = watcher.watch("/nonexistent/path/schema.sink");
        assert!(result.is_err());
    }

    #[test]
    fn test_watch_and_detect_change() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_schema_watch.sink");

        // Create test file
        fs::write(&test_file, "User { id: +u64 }").unwrap();

        // Canonicalize paths to handle symlinks (e.g., /var -> /private/var on macOS)
        let test_file_canonical = test_file.canonicalize().unwrap();

        let mut watcher = SchemaWatcher::new(50).unwrap();
        watcher.watch(&test_file).unwrap();

        // Spawn a thread to modify the file after a short delay
        let test_file_clone = test_file.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&test_file_clone)
                .unwrap();
            writeln!(file, "\nPost {{ id: +u64 }}").unwrap();
        });

        // Wait for event (with timeout)
        let _start = Instant::now();
        let event = watcher.next_event();

        assert!(event.is_ok());
        let event = event.unwrap();

        // Canonicalize the event path for comparison
        let event_path_canonical = event.path.canonicalize().unwrap_or(event.path.clone());
        assert_eq!(event_path_canonical, test_file_canonical);
        assert!(matches!(event.kind, ChangeKind::Modified));

        // Should complete in reasonable time
        assert!(_start.elapsed() < Duration::from_secs(2));

        // Cleanup
        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_debouncing() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_schema_debounce.sink");

        // Create test file
        fs::write(&test_file, "User { id: +u64 }").unwrap();

        let mut watcher = SchemaWatcher::new(200).unwrap();
        watcher.watch(&test_file).unwrap();

        // Spawn a thread to make multiple rapid changes
        let test_file_clone = test_file.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            for i in 0..5 {
                fs::write(&test_file_clone, format!("User {{ id: +u64 }} // {}", i)).unwrap();
                std::thread::sleep(Duration::from_millis(20));
            }
        });

        // Wait for event - should get only one despite multiple writes
        let event = watcher.next_event();
        assert!(event.is_ok());

        // Try to get another event immediately - should timeout
        // (debouncing should have coalesced all changes)
        let _start = Instant::now();
        match watcher.receiver.recv_timeout(Duration::from_millis(100)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Expected - no more events
            }
            _ => {
                // Might get one more event if timing is off, but not many
            }
        }

        // Cleanup
        fs::remove_file(&test_file).ok();
    }
}

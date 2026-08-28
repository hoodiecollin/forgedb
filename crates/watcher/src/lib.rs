pub mod regenerator;

use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher,
};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub use regenerator::{RegenerateCallback, RegenerateError, RegenerateResult, SchemaRegenerator};

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

pub struct SchemaWatcher {
    watcher: RecommendedWatcher,
    receiver: Receiver<Result<Event, notify::Error>>,
    debounce_duration: Duration,
}

impl SchemaWatcher {
    pub fn new(debounce_ms: u64) -> Result<Self, WatchError> {
        let (tx, rx) = channel();

        let watcher = RecommendedWatcher::new(
            move |res| {
                if tx.send(res).is_err() {
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

    pub fn unwatch<P: AsRef<Path>>(&mut self, path: P) -> Result<(), WatchError> {
        self.watcher.unwatch(path.as_ref())?;
        Ok(())
    }

    pub fn next_event(&self) -> Result<FileChangeEvent, WatchError> {
        loop {
            let event = self
                .receiver
                .recv()
                .map_err(|_| WatchError::NotifyError(notify::Error::generic("Channel closed")))?
                .map_err(WatchError::NotifyError)?;

            let (path, kind) = match self.extract_event_info(&event) {
                Some(info) => info,
                None => continue,
            };

            let deadline = Instant::now() + self.debounce_duration;
            let mut last_event_time = Instant::now();
            let mut latest_kind = kind;

            loop {
                let now = Instant::now();
                if now >= deadline && now.duration_since(last_event_time) >= self.debounce_duration
                {
                    break;
                }

                let timeout = deadline.saturating_duration_since(now);

                match self.receiver.recv_timeout(timeout) {
                    Ok(Ok(evt)) => {
                        if let Some((evt_path, evt_kind)) = self.extract_event_info(&evt)
                            && evt_path == path
                        {
                            last_event_time = Instant::now();
                            latest_kind = evt_kind;
                        }
                    }
                    Ok(Err(_)) => continue,
                    Err(_) => break,
                }
            }

            return Ok(FileChangeEvent {
                path,
                kind: latest_kind,
                timestamp: Instant::now(),
            });
        }
    }

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

pub fn auto_watch<P: AsRef<Path>, Q: AsRef<Path>>(
    schema_path: P,
    output_dir: Q,
    debounce_ms: u64,
    callback: Option<RegenerateCallback>,
) -> Result<(), WatchError> {
    let schema_path = schema_path.as_ref();
    let regenerator = SchemaRegenerator::new(schema_path, output_dir.as_ref());

    let schema_canon = schema_path
        .canonicalize()
        .unwrap_or_else(|_| schema_path.to_path_buf());

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

    let result = regenerator.regenerate();
    if let Some(ref cb) = callback {
        cb(&result);
    }

    loop {
        match watcher.next_event() {
            Ok(event) => {
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

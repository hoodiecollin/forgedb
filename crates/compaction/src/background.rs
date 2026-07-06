use crate::compactor::Compactor;
use crate::types::*;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

/// Background compaction manager
pub struct BackgroundCompactor {
    compactor: Arc<Mutex<Compactor>>,
    config: CompactionConfig,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<CompactionStatus>>,
    last_results: Arc<Mutex<Vec<CompactionResult>>>,
    /// C5: store the background thread handle so `Drop` can join it instead of
    /// sleeping and hoping.
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl BackgroundCompactor {
    pub fn new<P: AsRef<Path>>(data_dir: P, config: CompactionConfig) -> Self {
        let compactor = Compactor::new(data_dir, config.clone());

        Self {
            compactor: Arc::new(Mutex::new(compactor)),
            config,
            running: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(CompactionStatus::Idle)),
            last_results: Arc::new(Mutex::new(Vec::new())),
            handle: Mutex::new(None),
        }
    }

    /// Start the background compaction thread.
    ///
    /// No-op if already running.
    pub fn start(&self) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        self.running.store(true, Ordering::SeqCst);

        let compactor = Arc::clone(&self.compactor);
        let config = self.config.clone();
        let running = Arc::clone(&self.running);
        let status = Arc::clone(&self.status);
        let last_results = Arc::clone(&self.last_results);

        let handle = thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(config.check_interval_secs));

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                {
                    let mut s = status.lock().unwrap();
                    *s = CompactionStatus::Running;
                }

                let results = {
                    let compactor = compactor.lock().unwrap();
                    compactor.compact_needed()
                };

                match results {
                    Ok(results) => {
                        // C4: use log facade instead of println!/eprintln!
                        if !results.is_empty() {
                            log::info!(
                                "Background compaction completed for {} model(s)",
                                results.len()
                            );
                            for result in &results {
                                if result.success {
                                    log::info!(
                                        "  {} — reclaimed {} bytes ({:.1}%) in {}ms",
                                        result.model_name,
                                        result.bytes_reclaimed,
                                        result.reclaim_percentage(),
                                        result.duration_ms
                                    );
                                } else {
                                    log::error!(
                                        "  {} — compaction failed: {}",
                                        result.model_name,
                                        result.error.as_deref().unwrap_or("unknown error")
                                    );
                                }
                            }
                        }

                        {
                            let mut lr = last_results.lock().unwrap();
                            *lr = results;
                        }
                        {
                            let mut s = status.lock().unwrap();
                            *s = CompactionStatus::Completed;
                        }
                    }
                    Err(e) => {
                        // C4: use log facade
                        log::error!("Background compaction error: {}", e);
                        {
                            let mut s = status.lock().unwrap();
                            *s = CompactionStatus::Failed;
                        }
                    }
                }
            }

            let mut s = status.lock().unwrap();
            *s = CompactionStatus::Idle;
        });

        // C5: store handle so Drop can join it
        *self.handle.lock().unwrap() = Some(handle);
    }

    /// Signal the background thread to stop.
    ///
    /// Does not block; use `Drop` (or explicitly drop this struct) to wait for
    /// the thread to fully exit.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check whether the background thread is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get current compaction status.
    pub fn status(&self) -> CompactionStatus {
        let status = self.status.lock().unwrap();
        status.clone()
    }

    /// Get results from the last compaction run.
    pub fn last_results(&self) -> Vec<CompactionResult> {
        let results = self.last_results.lock().unwrap();
        results.clone()
    }

    /// Trigger a one-shot manual compaction in a new thread (non-blocking).
    ///
    /// # C6 fix
    ///
    /// The previous implementation checked status then spawned in separate steps,
    /// creating a TOCTOU race where two concurrent callers could both observe
    /// `!Running` and both spawn compaction threads.  Now the check and the
    /// `Running` transition happen inside a single mutex critical section.
    pub fn trigger_manual(&self) -> Result<(), String> {
        // Hold the lock while checking AND transitioning to Running so that
        // two concurrent callers cannot both pass the guard.
        {
            let mut s = self.status.lock().unwrap();
            if *s == CompactionStatus::Running {
                return Err("Compaction already running".to_string());
            }
            *s = CompactionStatus::Running;
        }

        let compactor = Arc::clone(&self.compactor);
        let status = Arc::clone(&self.status);
        let last_results = Arc::clone(&self.last_results);

        thread::spawn(move || {
            // Status is already set to Running by the caller; perform compaction.
            let results = {
                let compactor = compactor.lock().unwrap();
                compactor.compact_needed()
            };

            match results {
                Ok(results) => {
                    // C4: use log facade
                    log::info!(
                        "Manual compaction completed for {} model(s)",
                        results.len()
                    );
                    for result in &results {
                        if result.success {
                            log::info!(
                                "  {} — reclaimed {} bytes ({:.1}%) in {}ms",
                                result.model_name,
                                result.bytes_reclaimed,
                                result.reclaim_percentage(),
                                result.duration_ms
                            );
                        }
                    }

                    {
                        let mut lr = last_results.lock().unwrap();
                        *lr = results;
                    }
                    {
                        let mut s = status.lock().unwrap();
                        *s = CompactionStatus::Completed;
                    }
                }
                Err(e) => {
                    // C4: use log facade
                    log::error!("Manual compaction error: {}", e);
                    {
                        let mut s = status.lock().unwrap();
                        *s = CompactionStatus::Failed;
                    }
                }
            }
        });

        Ok(())
    }
}

impl Drop for BackgroundCompactor {
    fn drop(&mut self) {
        // Signal the background thread to stop
        self.running.store(false, Ordering::SeqCst);

        // C5: join the background thread so we don't race on shutdown.
        // The previous implementation did `thread::sleep(100ms)` which was
        // racy.  Joining blocks until the thread actually exits.
        if let Some(handle) = self.handle.lock().unwrap().take() {
            // Ignore join errors (the thread may have panicked)
            let _ = handle.join();
        }
    }
}

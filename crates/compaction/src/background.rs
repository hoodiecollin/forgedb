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
        }
    }

    /// Start background compaction thread
    pub fn start(&self) {
        if self.running.load(Ordering::SeqCst) {
            return; // Already running
        }

        self.running.store(true, Ordering::SeqCst);

        let compactor = Arc::clone(&self.compactor);
        let config = self.config.clone();
        let running = Arc::clone(&self.running);
        let status = Arc::clone(&self.status);
        let last_results = Arc::clone(&self.last_results);

        thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                // Wait for check interval
                thread::sleep(Duration::from_secs(config.check_interval_secs));

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                // Update status
                {
                    let mut s = status.lock().unwrap();
                    *s = CompactionStatus::Running;
                }

                // Perform compaction
                let results = {
                    let compactor = compactor.lock().unwrap();
                    compactor.compact_needed()
                };

                match results {
                    Ok(results) => {
                        if !results.is_empty() {
                            println!("Background compaction completed:");
                            for result in &results {
                                if result.success {
                                    println!(
                                        "  {} - Reclaimed {} bytes ({:.1}%) in {}ms",
                                        result.model_name,
                                        result.bytes_reclaimed,
                                        result.reclaim_percentage(),
                                        result.duration_ms
                                    );
                                } else {
                                    eprintln!(
                                        "  {} - Failed: {}",
                                        result.model_name,
                                        result
                                            .error
                                            .as_ref()
                                            .unwrap_or(&"Unknown error".to_string())
                                    );
                                }
                            }
                        }

                        // Store results
                        {
                            let mut lr = last_results.lock().unwrap();
                            *lr = results;
                        }

                        // Update status
                        {
                            let mut s = status.lock().unwrap();
                            *s = CompactionStatus::Completed;
                        }
                    }
                    Err(e) => {
                        eprintln!("Background compaction error: {}", e);

                        // Update status
                        {
                            let mut s = status.lock().unwrap();
                            *s = CompactionStatus::Failed;
                        }
                    }
                }
            }

            // Set status to idle when thread exits
            let mut s = status.lock().unwrap();
            *s = CompactionStatus::Idle;
        });
    }

    /// Stop background compaction thread
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if background compaction is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get current compaction status
    pub fn status(&self) -> CompactionStatus {
        let status = self.status.lock().unwrap();
        status.clone()
    }

    /// Get results from last compaction run
    pub fn last_results(&self) -> Vec<CompactionResult> {
        let results = self.last_results.lock().unwrap();
        results.clone()
    }

    /// Trigger manual compaction (non-blocking)
    pub fn trigger_manual(&self) -> Result<(), String> {
        if self.status() == CompactionStatus::Running {
            return Err("Compaction already running".to_string());
        }

        let compactor = Arc::clone(&self.compactor);
        let status = Arc::clone(&self.status);
        let last_results = Arc::clone(&self.last_results);

        thread::spawn(move || {
            // Update status
            {
                let mut s = status.lock().unwrap();
                *s = CompactionStatus::Running;
            }

            // Perform compaction
            let results = {
                let compactor = compactor.lock().unwrap();
                compactor.compact_needed()
            };

            match results {
                Ok(results) => {
                    println!("Manual compaction completed:");
                    for result in &results {
                        if result.success {
                            println!(
                                "  {} - Reclaimed {} bytes ({:.1}%) in {}ms",
                                result.model_name,
                                result.bytes_reclaimed,
                                result.reclaim_percentage(),
                                result.duration_ms
                            );
                        }
                    }

                    // Store results
                    {
                        let mut lr = last_results.lock().unwrap();
                        *lr = results;
                    }

                    // Update status
                    {
                        let mut s = status.lock().unwrap();
                        *s = CompactionStatus::Completed;
                    }
                }
                Err(e) => {
                    eprintln!("Manual compaction error: {}", e);

                    // Update status
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
        self.stop();
        // Give thread time to exit gracefully
        thread::sleep(Duration::from_millis(100));
    }
}


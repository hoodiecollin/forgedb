use crate::compactor::Compactor;
use crate::types::*;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

pub struct BackgroundCompactor {
    compactor: Arc<Mutex<Compactor>>,
    config: CompactionConfig,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<CompactionStatus>>,
    last_results: Arc<Mutex<Vec<CompactionResult>>>,
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

        *self.handle.lock().unwrap() = Some(handle);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> CompactionStatus {
        let status = self.status.lock().unwrap();
        status.clone()
    }

    pub fn last_results(&self) -> Vec<CompactionResult> {
        let results = self.last_results.lock().unwrap();
        results.clone()
    }

    pub fn trigger_manual(&self) -> Result<(), String> {
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
            let results = {
                let compactor = compactor.lock().unwrap();
                compactor.compact_needed()
            };

            match results {
                Ok(results) => {
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
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
}

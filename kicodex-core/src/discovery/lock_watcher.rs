use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;

/// Events emitted by the lock file watcher.
#[derive(Debug, Clone)]
pub enum LockEvent {
    /// A `.lck` file was created — a project was opened.
    ProjectOpened(PathBuf),
    /// A `.lck` file was removed — a project was closed.
    ProjectClosed(PathBuf),
}

/// Watches known project directories for `.lck` file creation/deletion.
/// Directories can be added both before and after `start()` is called.
pub struct LockWatcher {
    watched_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    event_tx: mpsc::UnboundedSender<LockEvent>,
    /// Channel to send new directories to the running watcher thread.
    add_dir_tx: std::sync::Mutex<Option<std::sync::mpsc::SyncSender<PathBuf>>>,
}

impl LockWatcher {
    /// Create a new lock watcher with the given event channel sender.
    pub fn new(event_tx: mpsc::UnboundedSender<LockEvent>) -> Self {
        Self {
            watched_dirs: Arc::new(std::sync::Mutex::new(Vec::new())),
            event_tx,
            add_dir_tx: std::sync::Mutex::new(None),
        }
    }

    /// Add a directory to watch for `.lck` files.
    /// Safe to call before or after `start()`.
    pub fn add_directory(&self, dir: PathBuf) {
        {
            let mut dirs = self.watched_dirs.lock().unwrap();
            if dirs.contains(&dir) {
                return;
            }
            dirs.push(dir.clone());
        }
        // If the watcher thread is already running, notify it.
        if let Some(tx) = self.add_dir_tx.lock().unwrap().as_ref() {
            let _ = tx.try_send(dir);
        }
    }

    /// Start watching all registered directories. Spawns a blocking thread.
    /// Returns an error if the watcher cannot be created.
    pub fn start(&self) -> Result<(), notify::Error> {
        let dirs = self.watched_dirs.lock().unwrap().clone();

        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_secs(1), tx)?;

        for dir in &dirs {
            if dir.exists() {
                if let Err(e) =
                    debouncer.watcher().watch(dir, notify::RecursiveMode::NonRecursive)
                {
                    tracing::warn!("Lock watcher: failed to watch {}: {}", dir.display(), e);
                } else {
                    tracing::debug!("Lock watcher: watching {}", dir.display());
                }
            }
        }

        // Channel for dynamically adding directories after start.
        let (add_tx, add_rx) = std::sync::mpsc::sync_channel::<PathBuf>(32);
        *self.add_dir_tx.lock().unwrap() = Some(add_tx);

        let event_tx = self.event_tx.clone();

        std::thread::spawn(move || {
            let mut debouncer = debouncer;

            loop {
                // Drain any pending add-directory requests first (non-blocking).
                while let Ok(dir) = add_rx.try_recv() {
                    if dir.exists() {
                        if let Err(e) = debouncer
                            .watcher()
                            .watch(&dir, notify::RecursiveMode::NonRecursive)
                        {
                            tracing::warn!(
                                "Lock watcher: failed to watch new dir {}: {}",
                                dir.display(),
                                e
                            );
                        } else {
                            tracing::debug!(
                                "Lock watcher: now watching new dir {}",
                                dir.display()
                            );
                        }
                    }
                }

                // Wait up to 100 ms for a file event, then loop to check add_rx again.
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(events)) => {
                        for event in &events {
                            if event.kind != DebouncedEventKind::Any {
                                continue;
                            }
                            let path = &event.path;
                            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

                            if ext != "lck" {
                                continue;
                            }

                            if let Some(parent) = path.parent() {
                                let dir = parent.to_path_buf();
                                if path.exists() {
                                    tracing::info!(
                                        "Lock file created: {} — project opened",
                                        path.display()
                                    );
                                    let _ = event_tx.send(LockEvent::ProjectOpened(dir));
                                } else {
                                    // Check if any other .lck files remain in the directory
                                    let has_remaining_locks = std::fs::read_dir(&dir)
                                        .map(|entries| {
                                            entries.filter_map(|e| e.ok()).any(|e| {
                                                e.path().extension().and_then(|ext| ext.to_str())
                                                    == Some("lck")
                                            })
                                        })
                                        .unwrap_or(false);

                                    if has_remaining_locks {
                                        tracing::debug!(
                                            "Lock file removed: {} — other locks remain, project still open",
                                            path.display()
                                        );
                                    } else {
                                        tracing::info!(
                                            "Lock file removed: {} — no locks remain, project closed",
                                            path.display()
                                        );
                                        let _ = event_tx.send(LockEvent::ProjectClosed(dir));
                                    }
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Lock watcher error: {}", e);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Normal — just loop to check add_rx.
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        tracing::info!("Lock watcher channel closed, stopping");
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

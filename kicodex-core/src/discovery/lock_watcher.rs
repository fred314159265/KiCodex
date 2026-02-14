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
pub struct LockWatcher {
    watched_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    event_tx: mpsc::UnboundedSender<LockEvent>,
}

impl LockWatcher {
    /// Create a new lock watcher with the given event channel sender.
    pub fn new(event_tx: mpsc::UnboundedSender<LockEvent>) -> Self {
        Self {
            watched_dirs: Arc::new(std::sync::Mutex::new(Vec::new())),
            event_tx,
        }
    }

    /// Add a directory to watch for `.lck` files.
    pub fn add_directory(&self, dir: PathBuf) {
        let mut dirs = self.watched_dirs.lock().unwrap();
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }

    /// Start watching all registered directories. Spawns a blocking thread.
    /// Returns an error if the watcher cannot be created.
    pub fn start(&self) -> Result<(), notify::Error> {
        let dirs = self.watched_dirs.lock().unwrap().clone();
        if dirs.is_empty() {
            return Ok(());
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_secs(1), tx)?;

        for dir in &dirs {
            if dir.exists() {
                debouncer
                    .watcher()
                    .watch(dir, notify::RecursiveMode::NonRecursive)?;
                tracing::debug!("Lock watcher: watching {}", dir.display());
            }
        }

        let event_tx = self.event_tx.clone();

        std::thread::spawn(move || {
            let _debouncer = debouncer;

            loop {
                match rx.recv() {
                    Ok(Ok(events)) => {
                        for event in &events {
                            if event.kind != DebouncedEventKind::Any {
                                continue;
                            }
                            let path = &event.path;
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("");

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
                                                e.path()
                                                    .extension()
                                                    .and_then(|ext| ext.to_str())
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
                    Err(_) => {
                        tracing::info!("Lock watcher channel closed, stopping");
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

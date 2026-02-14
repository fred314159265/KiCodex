use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use crate::registry::{PersistedRegistry, ProjectRegistry};

/// Start watching all registered library paths for changes.
/// When a CSV or YAML file changes, the corresponding library is reloaded
/// in the runtime registry.
///
/// This spawns a background tokio task and returns immediately.
pub fn start_watching(
    persisted: &PersistedRegistry,
    registry: Arc<ProjectRegistry>,
) -> Result<(), notify::Error> {
    // Build a map of watched directory -> (token, library_path) for reload lookup
    let mut watch_entries: Vec<(PathBuf, String, PathBuf)> = Vec::new();
    for entry in &persisted.projects {
        let library_path = PathBuf::from(&entry.library_path);
        if library_path.exists() {
            watch_entries.push((library_path.clone(), entry.token.clone(), library_path));
        }
    }

    if watch_entries.is_empty() {
        return Ok(());
    }

    let (tx, rx) = std::sync::mpsc::channel();

    let mut debouncer = new_debouncer(Duration::from_secs(1), tx)?;

    for (watch_path, _, _) in &watch_entries {
        debouncer
            .watcher()
            .watch(watch_path, notify::RecursiveMode::Recursive)?;
        tracing::info!("Watching {} for changes", watch_path.display());
    }

    let registry_clone = registry;
    let entries = watch_entries;

    // Spawn a background thread to process file system events
    std::thread::spawn(move || {
        // Keep the debouncer alive for the lifetime of this thread
        let _debouncer = debouncer;

        loop {
            match rx.recv() {
                Ok(Ok(events)) => {
                    for event in &events {
                        if event.kind != DebouncedEventKind::Any {
                            continue;
                        }
                        let path = &event.path;
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

                        if !matches!(ext, "csv" | "yaml" | "yml") {
                            continue;
                        }

                        // Find which library this file belongs to
                        for (watch_path, token, library_path) in &entries {
                            if path.starts_with(watch_path) {
                                tracing::info!(
                                    "Change detected in {}, reloading library...",
                                    path.display()
                                );
                                match registry_clone.reload(token, library_path) {
                                    Ok(()) => {
                                        tracing::info!("Library reloaded successfully");
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to reload library: {}", e);
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("File watch error: {}", e);
                }
                Err(_) => {
                    tracing::info!("File watcher channel closed, stopping");
                    break;
                }
            }
        }
    });

    Ok(())
}

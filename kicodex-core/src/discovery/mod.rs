pub mod auto_register;
pub mod lock_watcher;
pub mod process_scanner;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::registry::{PersistedRegistry, ProjectRegistry};

use self::lock_watcher::{LockEvent, LockWatcher};

/// Callback invoked when a new project is discovered and registered.
/// Receives the updated `PersistedRegistry` so the caller can sync state.
pub type OnDiscoveryCallback = Box<dyn Fn(&PersistedRegistry) + Send + Sync>;

/// Callback invoked when the set of active (open) projects changes.
/// Receives the list of currently active project directories.
pub type OnActiveChangedCallback = Box<dyn Fn(&[std::path::PathBuf]) + Send + Sync>;

/// Orchestrates automatic discovery of KiCad projects via process scanning
/// and lock file watching.
pub struct DiscoveryEngine {
    persisted: PersistedRegistry,
    registry: Arc<ProjectRegistry>,
    port: u16,
    on_discovery: Option<OnDiscoveryCallback>,
    on_active_changed: Option<OnActiveChangedCallback>,
}

impl DiscoveryEngine {
    pub fn new(persisted: PersistedRegistry, registry: Arc<ProjectRegistry>, port: u16) -> Self {
        Self {
            persisted,
            registry,
            port,
            on_discovery: None,
            on_active_changed: None,
        }
    }

    /// Set a callback invoked when new projects are discovered and registered.
    /// The callback receives the updated persisted registry.
    pub fn on_discovery(mut self, cb: impl Fn(&PersistedRegistry) + Send + Sync + 'static) -> Self {
        self.on_discovery = Some(Box::new(cb));
        self
    }

    /// Set a callback invoked whenever the set of active (open in KiCad) projects changes.
    pub fn on_active_changed(
        mut self,
        cb: impl Fn(&[std::path::PathBuf]) + Send + Sync + 'static,
    ) -> Self {
        self.on_active_changed = Some(Box::new(cb));
        self
    }

    /// Start the discovery engine. Returns a future that runs the discovery loop.
    ///
    /// The caller is responsible for spawning this on an async runtime, e.g.:
    /// - `tokio::spawn(engine.start())` from a tokio context
    /// - `tauri::async_runtime::spawn(engine.start())` from a Tauri app
    pub async fn start(self) {
        self.run().await;
    }

    async fn run(mut self) {
        tracing::info!("Discovery engine started");

        // Track currently active (open in KiCad) project directories
        let mut active_dirs: Vec<std::path::PathBuf> = Vec::new();

        // Initial process scan
        let initial_dirs = process_scanner::scan_kicad_processes();
        if !initial_dirs.is_empty() {
            tracing::info!("Initial scan found {} KiCad project(s)", initial_dirs.len());
        }

        for dir in &initial_dirs {
            self.try_register(dir);
        }

        // Update active set
        if initial_dirs != active_dirs {
            active_dirs = initial_dirs.clone();
            if let Some(ref cb) = self.on_active_changed {
                cb(&active_dirs);
            }
        }

        // Set up lock file watcher
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let lock_watcher = LockWatcher::new(event_tx);

        // Watch directories from persisted registry
        for entry in &self.persisted.projects {
            lock_watcher.add_directory(std::path::PathBuf::from(&entry.project_path));
        }
        // Watch discovered directories
        for dir in &initial_dirs {
            lock_watcher.add_directory(dir.clone());
        }

        if let Err(e) = lock_watcher.start() {
            tracing::warn!("Failed to start lock file watcher: {}", e);
        }

        // Periodic process scan loop
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.tick().await; // consume initial tick

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let dirs = process_scanner::scan_kicad_processes();
                    for dir in &dirs {
                        self.try_register(dir);
                    }
                    // Check if active set changed
                    if dirs != active_dirs {
                        tracing::info!(
                            "Active projects changed: {} -> {}",
                            active_dirs.len(),
                            dirs.len()
                        );
                        active_dirs = dirs;
                        if let Some(ref cb) = self.on_active_changed {
                            cb(&active_dirs);
                        }
                    }
                }
                Some(event) = event_rx.recv() => {
                    match event {
                        LockEvent::ProjectOpened(dir) => {
                            tracing::info!("Project opened (lock file): {}", dir.display());
                            self.try_register(&dir);
                            if !active_dirs.contains(&dir) {
                                active_dirs.push(dir);
                                if let Some(ref cb) = self.on_active_changed {
                                    cb(&active_dirs);
                                }
                            }
                        }
                        LockEvent::ProjectClosed(dir) => {
                            tracing::info!("Project closed (lock file): {}", dir.display());
                            if active_dirs.contains(&dir) {
                                active_dirs.retain(|d| d != &dir);
                                if let Some(ref cb) = self.on_active_changed {
                                    cb(&active_dirs);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn try_register(&mut self, dir: &std::path::Path) {
        match auto_register::try_auto_register(dir, &mut self.persisted, &self.registry, self.port)
        {
            Ok(count) if count > 0 => {
                tracing::info!(
                    "Auto-registered {} library/libraries from {}",
                    count,
                    dir.display()
                );
                if let Some(ref cb) = self.on_discovery {
                    cb(&self.persisted);
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::debug!("Could not auto-register {}: {}", dir.display(), e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_discovery_engine_starts_without_crash() {
        let persisted = PersistedRegistry::default();
        let registry = Arc::new(ProjectRegistry::new());
        let engine = DiscoveryEngine::new(persisted, registry, 18734);
        let handle = tokio::spawn(engine.start());

        // Let it run briefly then abort — just testing it doesn't panic on startup
        tokio::time::sleep(Duration::from_millis(500)).await;
        handle.abort();
        let _ = handle.await;
    }
}

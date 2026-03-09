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

        // Initial process scan, supplemented by lock-file check
        let mut initial_dirs = process_scanner::scan_kicad_processes();
        let lock_dirs = self.scan_registered_dirs_for_locks();
        for d in lock_dirs {
            if !initial_dirs.contains(&d) {
                initial_dirs.push(d);
            }
        }
        initial_dirs.sort();
        if !initial_dirs.is_empty() {
            tracing::info!("Initial scan found {} KiCad project(s)", initial_dirs.len());
        }

        // Set up lock file watcher before initial registration, so newly discovered
        // project dirs are added to the watcher immediately.
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let lock_watcher = LockWatcher::new(event_tx);

        // Watch directories already in persisted registry (project-attached entries).
        for entry in &self.persisted.projects {
            if let Some(ref pp) = entry.project_path {
                lock_watcher.add_directory(std::path::PathBuf::from(pp));
            }
        }

        if let Err(e) = lock_watcher.start() {
            tracing::warn!("Failed to start lock file watcher: {}", e);
        }

        for dir in &initial_dirs {
            self.try_register(dir, &lock_watcher);
        }

        // Update active set
        if initial_dirs != active_dirs {
            active_dirs = initial_dirs.clone();
            if let Some(ref cb) = self.on_active_changed {
                cb(&active_dirs);
            }
        }

        // Periodic process scan loop
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        // Slower interval for checking stale registrations
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(10));
        interval.tick().await; // consume initial tick
        cleanup_interval.tick().await;

        loop {
            tokio::select! {
                _ = cleanup_interval.tick() => {
                    self.cleanup_stale_registrations();
                }
                _ = interval.tick() => {
                    let mut dirs = process_scanner::scan_kicad_processes();

                    // Supplement with lock-file scan of all registered project
                    // directories.  This is far more reliable on Windows where
                    // sysinfo often cannot read process command-line args.
                    let lock_dirs = self.scan_registered_dirs_for_locks();
                    for d in lock_dirs {
                        if !dirs.contains(&d) {
                            dirs.push(d);
                        }
                    }

                    dirs.sort();

                    for dir in &dirs {
                        self.try_register(dir, &lock_watcher);
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
                            self.try_register(&dir, &lock_watcher);
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

    /// Strip the Windows extended-length path prefix (`\\?\`) so that
    /// normal `Path` operations (joins, exists, etc.) work reliably.
    fn normalize_path(p: &str) -> &str {
        p.strip_prefix(r"\\?\").unwrap_or(p)
    }

    /// Remove registered projects whose directories have been deleted entirely.
    ///
    /// We intentionally check only whether the *directory* still exists, not
    /// individual files like `library.yaml` or `kicodex.yaml`.  Missing files
    /// inside an existing directory are a configuration problem (and
    /// auto-register will skip them), not a stale registration.
    fn cleanup_stale_registrations(&mut self) {
        let stale_tokens: Vec<String> = self
            .persisted
            .projects
            .iter()
            .filter(|entry| {
                let lib_dir = std::path::Path::new(Self::normalize_path(&entry.library_path));
                if let Some(ref pp) = entry.project_path {
                    let project_dir = std::path::Path::new(Self::normalize_path(pp));
                    // Stale only if the project directory itself is gone
                    !project_dir.is_dir() || !lib_dir.is_dir()
                } else {
                    // Standalone: stale only if the library directory is gone
                    !lib_dir.is_dir()
                }
            })
            .map(|entry| entry.token.clone())
            .collect();

        if stale_tokens.is_empty() {
            return;
        }

        for token in &stale_tokens {
            // Log which entry is being removed so the user can debug
            if let Some(entry) = self.persisted.projects.iter().find(|e| e.token == *token) {
                tracing::info!(
                    "Deregistering stale entry '{}' (directory removed): project={}, library={}",
                    entry.name,
                    entry.project_path.as_deref().unwrap_or("<standalone>"),
                    entry.library_path,
                );
            }
            self.registry.remove(token);
        }

        // Remove all stale entries from persisted registry
        let stale_token_set: std::collections::HashSet<&str> =
            stale_tokens.iter().map(|t| t.as_str()).collect();
        self.persisted
            .projects
            .retain(|p| !stale_token_set.contains(p.token.as_str()));

        // Save and notify
        if let Some(registry_path) = crate::registry::PersistedRegistry::default_path() {
            if let Err(e) = self.persisted.save(&registry_path) {
                tracing::warn!("Failed to save registry after cleanup: {}", e);
            }
        }

        if let Some(ref cb) = self.on_discovery {
            cb(&self.persisted);
        }
    }

    /// Scan all registered project directories for `.lck` files.
    /// Returns directories that contain at least one lock file, indicating
    /// KiCad has a sub-editor (schematic/PCB) open in that project.
    fn scan_registered_dirs_for_locks(&self) -> Vec<std::path::PathBuf> {
        let mut dirs = Vec::new();
        for entry in &self.persisted.projects {
            let project_path = match entry.project_path {
                Some(ref pp) => std::path::PathBuf::from(Self::normalize_path(pp)),
                None => continue,
            };
            if !project_path.is_dir() {
                continue;
            }
            let has_lock = std::fs::read_dir(&project_path)
                .map(|entries| {
                    entries.filter_map(|e| e.ok()).any(|e| {
                        e.path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            == Some("lck")
                    })
                })
                .unwrap_or(false);
            if has_lock && !dirs.contains(&project_path) {
                dirs.push(project_path);
            }
        }
        dirs
    }

    fn try_register(&mut self, dir: &std::path::Path, lock_watcher: &LockWatcher) {
        match auto_register::try_auto_register(dir, &mut self.persisted, &self.registry, self.port)
        {
            Ok(count) if count > 0 => {
                tracing::info!(
                    "Auto-registered {} library/libraries from {}",
                    count,
                    dir.display()
                );
                // Save persisted registry to disk
                if let Some(registry_path) = crate::registry::PersistedRegistry::default_path() {
                    if let Err(e) = self.persisted.save(&registry_path) {
                        tracing::warn!("Failed to save registry: {}", e);
                    }
                }
                // Ensure the newly registered project directory is watched for lock files.
                lock_watcher.add_directory(dir.to_path_buf());
                if let Some(ref cb) = self.on_discovery {
                    cb(&self.persisted);
                }
            }
            Ok(_) => {
                // Already registered — still ensure we're watching the dir.
                lock_watcher.add_directory(dir.to_path_buf());
            }
            Err(e) => {
                tracing::debug!("Could not auto-register {}: {}", dir.display(), e);
            }
        }

        // Ensure httplib files for all persisted entries matching this directory,
        // even if there's no kicodex.yaml (e.g. registered via the UI).
        let dir_str = dir.to_string_lossy();
        for entry in &self.persisted.projects {
            if entry.project_path.as_deref() == Some(&*dir_str) {
                if let Err(e) = auto_register::ensure_httplib_file(
                    dir,
                    &entry.name,
                    entry.description.as_deref(),
                    &entry.token,
                    self.port,
                ) {
                    tracing::warn!("Failed to ensure httplib for {}: {}", entry.name, e);
                }
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

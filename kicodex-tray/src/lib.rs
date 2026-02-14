mod tray;

use std::path::PathBuf;
use std::sync::Arc;

use kicodex_core::discovery::DiscoveryEngine;
use kicodex_core::registry::{PersistedRegistry, ProjectRegistry};
use tauri::Manager;

/// Shared application state accessible via `app.state()`.
pub struct AppState {
    pub persisted: std::sync::Mutex<PersistedRegistry>,
    pub active_projects: std::sync::Mutex<Vec<ActiveProject>>,
    pub registry: Arc<ProjectRegistry>,
    pub port: u16,
}

/// An active project shown in the tray menu.
#[derive(Clone)]
pub struct ActiveProject {
    pub name: String,
    pub project_path: PathBuf,
}

/// Build menu labels from active projects.
fn menu_labels_from_active(active: &[ActiveProject]) -> Vec<String> {
    active
        .iter()
        .map(|p| {
            let dir_name = p
                .project_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            format!("{} — {}", p.name, dir_name)
        })
        .collect()
}

/// Resolve active project dirs against the persisted registry to get display info.
fn resolve_active_projects(
    active_dirs: &[PathBuf],
    persisted: &PersistedRegistry,
) -> Vec<ActiveProject> {
    active_dirs
        .iter()
        .map(|dir| {
            let dir_str = dir.to_string_lossy().to_string();
            // Find matching entry in persisted registry
            let name = persisted
                .projects
                .iter()
                .find(|p| p.project_path == dir_str)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| {
                    dir.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".to_string())
                });
            ActiveProject {
                name,
                project_path: dir.clone(),
            }
        })
        .collect()
}

pub fn run() {
    let port: u16 = 18734;

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
            // Init tracing
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();

            tracing::info!("KiCodex tray app starting");

            // Load persisted registry
            let registry_path =
                PersistedRegistry::default_path().expect("Could not determine config directory");
            let persisted = PersistedRegistry::load(&registry_path).unwrap_or_else(|e| {
                tracing::warn!("Failed to load registry: {}, starting fresh", e);
                PersistedRegistry::default()
            });

            // Build runtime registry from persisted
            let registry = ProjectRegistry::from_persisted(&persisted).unwrap_or_else(|e| {
                tracing::warn!("Failed to load some libraries: {}", e);
                ProjectRegistry::new()
            });
            let registry = Arc::new(registry);

            // Start file watcher for hot-reload
            if let Err(e) = kicodex_core::watcher::start_watching(&persisted, registry.clone()) {
                tracing::warn!("Failed to start file watcher: {}", e);
            }

            // Spawn HTTP server
            let server_registry = registry.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    kicodex_core::server::run_server_with_registry(server_registry, port).await
                {
                    tracing::error!("HTTP server error: {}", e);
                }
            });

            // Build initial tray menu (no active projects yet until first scan)
            let tray = app.tray_by_id("main");

            if let Some(tray) = &tray {
                let menu = tray::build_menu(app.handle(), &[]).expect("Failed to build tray menu");
                tray.set_menu(Some(menu)).expect("Failed to set tray menu");
                let _ = tray.set_tooltip(Some("KiCodex — scanning..."));

                // Set up menu event handler
                let app_handle = app.handle().clone();
                tray.on_menu_event(move |_tray, event| {
                    tray::handle_menu_event(&app_handle, event.id().as_ref());
                });
            }

            // Store shared state
            let state = AppState {
                persisted: std::sync::Mutex::new(persisted.clone()),
                active_projects: std::sync::Mutex::new(Vec::new()),
                registry: registry.clone(),
                port,
            };
            app.manage(state);

            // Start discovery engine
            let discovery_registry = registry.clone();
            let discovery_persisted = persisted;

            let app_handle_discovery = app.handle().clone();
            let app_handle_active = app.handle().clone();

            let engine = DiscoveryEngine::new(discovery_persisted, discovery_registry, port)
                .on_discovery(move |updated_persisted| {
                    tracing::info!(
                        "Discovery: now {} project(s) registered",
                        updated_persisted.projects.len()
                    );
                    // Sync persisted state
                    if let Some(state) = app_handle_discovery.try_state::<AppState>() {
                        *state.persisted.lock().unwrap() = updated_persisted.clone();
                    }
                })
                .on_active_changed(move |active_dirs| {
                    if let Some(state) = app_handle_active.try_state::<AppState>() {
                        let persisted = state.persisted.lock().unwrap();
                        let active = resolve_active_projects(active_dirs, &persisted);
                        let labels = menu_labels_from_active(&active);
                        *state.active_projects.lock().unwrap() = active;

                        if let Some(tray) = app_handle_active.tray_by_id("main") {
                            tray::update_menu(&app_handle_active, &tray, &labels);
                        }
                    }
                });

            tauri::async_runtime::spawn(engine.start());

            tracing::info!("KiCodex tray app initialized, server on port {}", port);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running KiCodex tray application");
}

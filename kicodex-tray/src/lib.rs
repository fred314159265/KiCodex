mod command_types;
mod commands;
mod tray;

use std::path::PathBuf;
use std::sync::Arc;

use kicodex_core::data::kicad_libs::KicadLibraries;
use kicodex_core::discovery::DiscoveryEngine;
use kicodex_core::registry::{PersistedRegistry, ProjectRegistry};
use tauri::{Emitter, Manager};

/// Shared application state accessible via `app.state()`.
pub struct AppState {
    pub persisted: std::sync::Mutex<PersistedRegistry>,
    pub active_projects: std::sync::Mutex<Vec<ActiveProject>>,
    pub registry: Arc<ProjectRegistry>,
    pub port: u16,
    pub kicad_libs: std::sync::Mutex<Option<KicadLibraries>>,
    pub validation_summary: std::sync::Mutex<Option<(usize, usize)>>,
}

/// An active project shown in the tray menu.
#[derive(Clone)]
pub struct ActiveProject {
    pub name: String,
    pub project_path: PathBuf,
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

/// Run validation across all registered projects and return (total_errors, total_warnings).
fn run_validation_summary(persisted: &PersistedRegistry) -> (usize, usize) {
    let mut total_errors = 0;
    let mut total_warnings = 0;

    // Collect unique library paths to avoid validating the same library twice
    let mut seen_libs = std::collections::HashSet::new();

    for entry in &persisted.projects {
        if !seen_libs.insert(entry.library_path.clone()) {
            continue;
        }

        let lib_path = entry.library_path
            .strip_prefix(r"\\?\")
            .unwrap_or(&entry.library_path)
            .to_string();

        match commands::validate_library(lib_path, Some(entry.project_path.clone())) {
            Ok(result) => {
                total_errors += result.error_count;
                total_warnings += result.warning_count;
            }
            Err(e) => {
                tracing::warn!("Validation failed for {}: {}", entry.name, e);
            }
        }
    }

    (total_errors, total_warnings)
}

pub fn run() {
    let port: u16 = 18734;

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::remove_project,
            commands::remove_library,
            commands::get_projects,
            commands::get_project_libraries,
            commands::scan_for_libraries,
            commands::apply_scan,
            commands::validate_library,
            commands::init_project,
            commands::create_library,
            commands::add_part_table,
            commands::delete_part_table,
            commands::get_part_table_data,
            commands::add_component,
            commands::update_component,
            commands::delete_component,
            commands::get_template,
            commands::save_template,
            commands::list_templates,
            commands::list_kicad_libraries,
            commands::list_kicad_entries,
            commands::open_in_explorer,
            commands::get_discovered_projects,
            commands::scan_project,
            commands::add_project,
            commands::add_git_library,
        ])
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
                    kicodex_core::server::run_server_with_registry(server_registry, port, "127.0.0.1").await
                {
                    tracing::error!("HTTP server error: {}", e);
                }
            });

            // Try to load KiCad libraries (non-fatal)
            let kicad_libs = match KicadLibraries::load(None) {
                Ok(libs) => {
                    tracing::info!("Loaded KiCad library tables");
                    Some(libs)
                }
                Err(e) => {
                    tracing::warn!("Could not load KiCad library tables: {}", e);
                    None
                }
            };

            // Build initial tray menu (no active projects yet until first scan)
            let tray = app.tray_by_id("main");

            if let Some(tray) = &tray {
                let menu =
                    tray::build_menu(app.handle(), &[], None).expect("Failed to build tray menu");
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
                kicad_libs: std::sync::Mutex::new(kicad_libs),
                validation_summary: std::sync::Mutex::new(None),
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
                    let _ = app_handle_discovery.emit("projects-changed", ());
                })
                .on_active_changed(move |active_dirs| {
                    if let Some(state) = app_handle_active.try_state::<AppState>() {
                        let persisted = state.persisted.lock().unwrap();
                        let active = resolve_active_projects(active_dirs, &persisted);
                        *state.active_projects.lock().unwrap() = active.clone();
                        drop(persisted);

                        // Update menu immediately with current validation (may be None)
                        let validation = *state.validation_summary.lock().unwrap();
                        if let Some(tray) = app_handle_active.tray_by_id("main") {
                            tray::update_menu(&app_handle_active, &tray, &active, validation);
                        }

                        // Spawn async validation in background
                        let app_for_validation = app_handle_active.clone();
                        let persisted_snapshot = state.persisted.lock().unwrap().clone();
                        std::thread::spawn(move || {
                            let summary = run_validation_summary(&persisted_snapshot);

                            if let Some(state) = app_for_validation.try_state::<AppState>() {
                                *state.validation_summary.lock().unwrap() = Some(summary);
                                let active = state.active_projects.lock().unwrap().clone();
                                if let Some(tray) = app_for_validation.tray_by_id("main") {
                                    tray::update_menu(
                                        &app_for_validation,
                                        &tray,
                                        &active,
                                        Some(summary),
                                    );
                                }
                            }
                        });
                    }
                    let _ = app_handle_active.emit("projects-changed", ());
                });

            tauri::async_runtime::spawn(engine.start());

            tracing::info!("KiCodex tray app initialized, server on port {}", port);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building KiCodex tray application")
        .run(|_app, event| {
            // Prevent exit when the last window closes — this is a tray app.
            // The user quits explicitly via the "Quit" tray menu item.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}

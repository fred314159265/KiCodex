use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Emitter, Manager, WebviewWindowBuilder, WebviewUrl};

use crate::ActiveProject;
use crate::AppState;

/// Build the tray menu with current project list and optional validation summary.
pub fn build_menu(
    app: &AppHandle,
    projects: &[ActiveProject],
    validation: Option<(usize, usize)>,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let status_text = match validation {
        Some((0, 0)) => "Status: All clear".to_string(),
        Some((errors, warnings)) => format!("Status: {} error(s), {} warning(s)", errors, warnings),
        None => "Status: Running".to_string(),
    };

    let mut builder = MenuBuilder::new(app)
        .item(
            &MenuItemBuilder::with_id("title", "KiCodex")
                .enabled(false)
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("status", &status_text)
                .enabled(false)
                .build(app)?,
        )
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("open-dashboard", "Open Dashboard").build(app)?);

    builder = builder.item(&PredefinedMenuItem::separator(app)?);

    if projects.is_empty() {
        builder = builder.item(
            &MenuItemBuilder::with_id("no-projects", "No projects registered")
                .enabled(false)
                .build(app)?,
        );
    } else {
        for (i, project) in projects.iter().enumerate() {
            let dir_name = project
                .project_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let label = format!("{} — {}", project.name, dir_name);

            let submenu = SubmenuBuilder::with_id(app, format!("project-{i}"), &label)
                .item(
                    &MenuItemBuilder::with_id(format!("project-{i}-dashboard"), "Open in Dashboard")
                        .build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id(format!("project-{i}-validate"), "Validate")
                        .build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id(format!("project-{i}-folder"), "Open Folder")
                        .build(app)?,
                )
                .build()?;

            builder = builder.item(&submenu);
        }
    }

    builder = builder
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("open-config", "Open Settings Folder").build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?);

    builder.build()
}

/// Update the tray menu with the current project list and validation summary.
pub fn update_menu(
    app: &AppHandle,
    tray: &TrayIcon,
    projects: &[ActiveProject],
    validation: Option<(usize, usize)>,
) {
    match build_menu(app, projects, validation) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                tracing::error!("Failed to update tray menu: {}", e);
            }
            let tooltip = match validation {
                Some((0, 0)) => {
                    format!("KiCodex — {} project(s) active", projects.len())
                }
                Some((errors, warnings)) => {
                    format!(
                        "KiCodex — {} project(s) active — {} error(s), {} warning(s)",
                        projects.len(),
                        errors,
                        warnings,
                    )
                }
                None => format!("KiCodex — {} project(s) active", projects.len()),
            };
            let _ = tray.set_tooltip(Some(&tooltip));
        }
        Err(e) => {
            tracing::error!("Failed to build tray menu: {}", e);
        }
    }
}

/// Open or focus the dashboard window.
pub fn open_dashboard(app: &AppHandle) {
    // If window already exists, focus it
    if let Some(window) = app.get_webview_window("dashboard") {
        let _ = window.set_focus();
        return;
    }

    // Create new dashboard window
    match WebviewWindowBuilder::new(app, "dashboard", WebviewUrl::default())
        .title("KiCodex")
        .inner_size(1000.0, 700.0)
        .min_inner_size(800.0, 500.0)
        .build()
    {
        Ok(_) => {
            tracing::info!("Dashboard window opened");
        }
        Err(e) => {
            tracing::error!("Failed to open dashboard window: {}", e);
        }
    }
}

/// Handle tray menu item clicks.
pub fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "quit" => {
            tracing::info!("Quit requested from tray menu");
            app.exit(0);
        }
        "open-dashboard" => {
            open_dashboard(app);
        }
        "open-config" => {
            if let Some(config_dir) = dirs::config_dir() {
                let kicodex_dir = config_dir.join("kicodex");
                if kicodex_dir.exists() {
                    let _ = open::that(&kicodex_dir);
                } else {
                    let _ = open::that(&config_dir);
                }
            }
        }
        id if id.starts_with("project-") => {
            // Parse: project-{i}-{action}
            let rest = id.strip_prefix("project-").unwrap();
            let parts: Vec<&str> = rest.splitn(2, '-').collect();
            let idx: usize = match parts[0].parse() {
                Ok(i) => i,
                Err(_) => return,
            };

            let action = parts.get(1).copied().unwrap_or("folder");

            if let Some(state) = app.try_state::<AppState>() {
                let active = state.active_projects.lock().unwrap();
                if let Some(project) = active.get(idx) {
                    match action {
                        "dashboard" => {
                            let path = project.project_path.to_string_lossy().to_string();
                            drop(active);
                            open_dashboard(app);
                            let hash = format!(
                                "project?path={}",
                                urlencoding::encode(&path)
                            );
                            let _ = app.emit("navigate", hash);
                        }
                        "validate" => {
                            let project_path =
                                project.project_path.to_string_lossy().to_string();
                            // Find first library for this project
                            let persisted = state.persisted.lock().unwrap();
                            let lib_path = persisted
                                .projects
                                .iter()
                                .find(|p| p.project_path.as_deref() == Some(project_path.as_str()))
                                .map(|p| p.library_path.clone());
                            drop(persisted);
                            drop(active);

                            open_dashboard(app);
                            if let Some(lib) = lib_path {
                                let clean_lib = lib
                                    .strip_prefix(r"\\?\")
                                    .unwrap_or(&lib)
                                    .to_string();
                                let hash = format!(
                                    "validate?lib={}&project={}",
                                    urlencoding::encode(&clean_lib),
                                    urlencoding::encode(&project_path),
                                );
                                let _ = app.emit("navigate", hash);
                            }
                        }
                        _ => {
                            if project.project_path.exists() {
                                let _ = open::that(&project.project_path);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// Build the tray menu with current project list.
pub fn build_menu(app: &AppHandle, project_names: &[String]) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let mut builder = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("title", "KiCodex").enabled(false).build(app)?)
        .item(&MenuItemBuilder::with_id("status", "Status: Running").enabled(false).build(app)?)
        .item(&PredefinedMenuItem::separator(app)?);

    if project_names.is_empty() {
        builder = builder.item(
            &MenuItemBuilder::with_id("no-projects", "No projects registered")
                .enabled(false)
                .build(app)?,
        );
    } else {
        for (i, name) in project_names.iter().enumerate() {
            builder = builder.item(
                &MenuItemBuilder::with_id(format!("project-{i}"), name)
                    .build(app)?,
            );
        }
    }

    builder = builder
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("open-config", "Open Config Directory").build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?);

    builder.build()
}

/// Update the tray menu with the current project list.
pub fn update_menu(app: &AppHandle, tray: &TrayIcon, project_names: &[String]) {
    match build_menu(app, project_names) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                tracing::error!("Failed to update tray menu: {}", e);
            }
            let tooltip = format!("KiCodex — {} project(s) active", project_names.len());
            let _ = tray.set_tooltip(Some(&tooltip));
        }
        Err(e) => {
            tracing::error!("Failed to build tray menu: {}", e);
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
            if let Ok(idx) = id.strip_prefix("project-").unwrap().parse::<usize>() {
                if let Some(state) = app.try_state::<AppState>() {
                    let active = state.active_projects.lock().unwrap();
                    if let Some(project) = active.get(idx) {
                        if project.project_path.exists() {
                            let _ = open::that(&project.project_path);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

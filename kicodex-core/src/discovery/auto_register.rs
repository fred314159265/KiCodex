use std::path::Path;
use std::sync::Arc;

use crate::data::project;
use crate::registry::{PersistedRegistry, ProjectEntry, ProjectRegistry};
use crate::server;

/// Generate the expected `.kicad_httplib` content for a library.
fn expected_httplib_content(name: &str, description: &str, token: &str, port: u16) -> String {
    format!(
        r#"{{
    "meta": {{
        "version": 1.0
    }},
    "name": "{name}",
    "description": "{description}",
    "source": {{
        "type": "REST_API",
        "api_version": "v1",
        "root_url": "http://127.0.0.1:{port}",
        "token": "{token}"
    }}
}}"#
    )
}

/// Build a description string, falling back to a template if none provided.
fn resolve_description(name: &str, description: Option<&str>) -> String {
    description
        .map(|d| d.to_string())
        .unwrap_or_else(|| format!("KiCodex HTTP Library for {name}"))
}

/// Ensure the `.kicad_httplib` file exists and has the correct token/port.
/// Rewrites the file if it's missing, has a stale token, or wrong port.
fn ensure_httplib_file(
    project_dir: &Path,
    name: &str,
    description: Option<&str>,
    token: &str,
    port: u16,
) -> Result<(), std::io::Error> {
    let httplib_path = project_dir.join(format!("{}.kicad_httplib", name));
    let desc = resolve_description(name, description);
    let expected = expected_httplib_content(name, &desc, token, port);

    // Check if the file already has the right content
    if httplib_path.exists() {
        if let Ok(existing) = std::fs::read_to_string(&httplib_path) {
            if existing == expected {
                return Ok(());
            }
            tracing::info!(
                "Updating {} (token or port changed)",
                httplib_path.display()
            );
        }
    } else {
        tracing::info!("Writing {}", httplib_path.display());
    }

    std::fs::write(&httplib_path, &expected)?;
    Ok(())
}

/// Try to auto-register a project directory with KiCodex.
///
/// Checks for `kicodex.yaml` in the project directory. If found and not already
/// registered, generates tokens, loads the library, registers it, and writes
/// `.kicad_httplib` files. For already-registered libraries, ensures the
/// `.kicad_httplib` file exists and has the correct token/port.
///
/// Returns the number of newly registered libraries (0 if all were already registered
/// or no kicodex.yaml found).
pub fn try_auto_register(
    project_dir: &Path,
    persisted: &mut PersistedRegistry,
    registry: &Arc<ProjectRegistry>,
    port: u16,
) -> Result<usize, AutoRegisterError> {
    let config = match project::load_project_config(project_dir) {
        Ok(c) => c,
        Err(project::ProjectError::Io(_)) => {
            // No kicodex.yaml — not a KiCodex project, skip silently
            return Ok(0);
        }
        Err(e) => return Err(AutoRegisterError::Project(e)),
    };

    let project_path_str = project_dir.to_string_lossy().to_string();
    let mut newly_registered = 0;

    for lib_ref in &config.libraries {
        // Check if already registered by matching project_path + library name
        let existing = persisted
            .projects
            .iter()
            .find(|p| p.project_path.as_deref() == Some(project_path_str.as_str()) && p.name == lib_ref.name);

        if let Some(entry) = existing {
            // Already registered — just ensure the .kicad_httplib file is correct
            if let Err(e) = ensure_httplib_file(
                project_dir,
                &lib_ref.name,
                entry.description.as_deref(),
                &entry.token,
                port,
            ) {
                tracing::warn!(
                    "Failed to update .kicad_httplib for {}: {}",
                    lib_ref.name,
                    e
                );
            }
            continue;
        }

        let library_path = project_dir.join(&lib_ref.path);
        let library_path = library_path
            .canonicalize()
            .unwrap_or_else(|_| library_path.clone());

        // Validate that the library can actually be loaded
        let library = server::load_library(&library_path)?;
        tracing::info!(
            "Auto-discovered library '{}' at {}",
            library.name,
            library_path.display()
        );

        let token = uuid::Uuid::new_v4().to_string();

        let description = library.description.clone();

        persisted.upsert(ProjectEntry {
            token: token.clone(),
            project_path: Some(project_path_str.clone()),
            library_path: library_path.to_string_lossy().to_string(),
            name: lib_ref.name.clone(),
            description: description.clone(),
        });

        // Register in runtime registry
        registry.insert(&token, library);

        // Write .kicad_httplib file
        ensure_httplib_file(
            project_dir,
            &lib_ref.name,
            description.as_deref(),
            &token,
            port,
        )
        .map_err(AutoRegisterError::Io)?;

        newly_registered += 1;
    }

    // Save persisted registry if we registered anything
    if newly_registered > 0 {
        if let Some(registry_path) = PersistedRegistry::default_path() {
            persisted.save(&registry_path).map_err(|e| match e {
                crate::registry::RegistryError::Io(io) => AutoRegisterError::Io(io),
                other => AutoRegisterError::Registry(other.to_string()),
            })?;
            tracing::info!(
                "Registry saved with {} new library/libraries",
                newly_registered
            );
        }
    }

    Ok(newly_registered)
}

#[derive(Debug, thiserror::Error)]
pub enum AutoRegisterError {
    #[error("project config error: {0}")]
    Project(#[from] project::ProjectError),
    #[error("server error loading library: {0}")]
    Server(#[from] server::ServerError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("registry error: {0}")]
    Registry(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_minimal_library(dir: &Path) {
        let schemas_dir = dir.join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        fs::write(
            dir.join("library.yaml"),
            r#"name: test-lib
schemas_path: schemas
tables:
  - name: Resistors
    file: resistors.csv
    schema: resistors
"#,
        )
        .unwrap();

        fs::write(
            schemas_dir.join("resistors.yaml"),
            r#"fields:
  value:
    display_name: Value
    visible: true
  description:
    display_name: Description
    visible: true
"#,
        )
        .unwrap();

        fs::write(
            dir.join("resistors.csv"),
            "id,value,description\n1,10k,10k Resistor\n",
        )
        .unwrap();
    }

    fn setup_project(tmp: &TempDir) -> (&Path, std::path::PathBuf) {
        let project_dir = tmp.path();
        let lib_dir = project_dir.join("libs").join("components");
        fs::create_dir_all(&lib_dir).unwrap();
        create_minimal_library(&lib_dir);
        fs::write(
            project_dir.join("kicodex.yaml"),
            "libraries:\n  - name: components\n    path: libs/components\n",
        )
        .unwrap();
        (project_dir, lib_dir)
    }

    #[test]
    fn test_auto_register_with_kicodex_yaml() {
        let tmp = TempDir::new().unwrap();
        let (project_dir, _) = setup_project(&tmp);

        let mut persisted = PersistedRegistry::default();
        let registry = Arc::new(ProjectRegistry::new());

        let count = try_auto_register(project_dir, &mut persisted, &registry, 18734).unwrap();

        assert_eq!(count, 1);
        assert_eq!(persisted.projects.len(), 1);
        assert_eq!(persisted.projects[0].name, "components");
        assert!(registry.tokens().len() == 1);

        // Check .kicad_httplib was written
        let httplib = project_dir.join("components.kicad_httplib");
        assert!(httplib.exists());
    }

    #[test]
    fn test_auto_register_skips_already_registered() {
        let tmp = TempDir::new().unwrap();
        let (project_dir, lib_dir) = setup_project(&tmp);

        let mut persisted = PersistedRegistry::default();
        persisted.upsert(ProjectEntry {
            token: "existing-token".to_string(),
            project_path: Some(project_dir.to_string_lossy().to_string()),
            library_path: lib_dir.to_string_lossy().to_string(),
            name: "components".to_string(),
            description: None,
        });

        let registry = Arc::new(ProjectRegistry::new());
        let count = try_auto_register(project_dir, &mut persisted, &registry, 18734).unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_auto_register_no_kicodex_yaml() {
        let tmp = TempDir::new().unwrap();
        let mut persisted = PersistedRegistry::default();
        let registry = Arc::new(ProjectRegistry::new());

        let count = try_auto_register(tmp.path(), &mut persisted, &registry, 18734).unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_ensures_httplib_when_already_registered() {
        let tmp = TempDir::new().unwrap();
        let (project_dir, lib_dir) = setup_project(&tmp);

        let mut persisted = PersistedRegistry::default();
        persisted.upsert(ProjectEntry {
            token: "my-token".to_string(),
            project_path: Some(project_dir.to_string_lossy().to_string()),
            library_path: lib_dir.to_string_lossy().to_string(),
            name: "components".to_string(),
            description: None,
        });

        let registry = Arc::new(ProjectRegistry::new());

        // No .kicad_httplib file exists yet
        let httplib = project_dir.join("components.kicad_httplib");
        assert!(!httplib.exists());

        let count = try_auto_register(project_dir, &mut persisted, &registry, 18734).unwrap();
        assert_eq!(count, 0); // Not newly registered

        // But the file should now exist with the correct token
        assert!(httplib.exists());
        let content = fs::read_to_string(&httplib).unwrap();
        assert!(content.contains("my-token"));
        assert!(content.contains("18734"));
    }

    #[test]
    fn test_rewrites_httplib_with_stale_token() {
        let tmp = TempDir::new().unwrap();
        let (project_dir, lib_dir) = setup_project(&tmp);

        let mut persisted = PersistedRegistry::default();
        persisted.upsert(ProjectEntry {
            token: "correct-token".to_string(),
            project_path: Some(project_dir.to_string_lossy().to_string()),
            library_path: lib_dir.to_string_lossy().to_string(),
            name: "components".to_string(),
            description: None,
        });

        // Write a stale .kicad_httplib with wrong token
        let httplib = project_dir.join("components.kicad_httplib");
        fs::write(&httplib, r#"{"source":{"token":"wrong-token"}}"#).unwrap();

        let registry = Arc::new(ProjectRegistry::new());
        try_auto_register(project_dir, &mut persisted, &registry, 18734).unwrap();

        // File should now have the correct token
        let content = fs::read_to_string(&httplib).unwrap();
        assert!(content.contains("correct-token"));
        assert!(!content.contains("wrong-token"));
    }
}

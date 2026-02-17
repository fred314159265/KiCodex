use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::server::{LoadedLibrary, ServerError};

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("failed to read/write registry: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse registry JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server error loading library: {0}")]
    Server(#[from] ServerError),
}

/// A persisted project entry in the registry JSON file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub library_path: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Persistent registry stored as JSON on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedRegistry {
    pub projects: Vec<ProjectEntry>,
}

impl PersistedRegistry {
    /// Get the default registry file path.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("kicodex").join("projects.json"))
    }

    /// Load the registry from disk. Returns empty registry if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let registry: Self = serde_json::from_str(&content)?;
        Ok(registry)
    }

    /// Save the registry to disk, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> Result<(), RegistryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Add or update a project entry. For standalone entries (project_path is None),
    /// dedup by library_path alone. For project entries, dedup by project_path + library_path pair.
    pub fn upsert(&mut self, entry: ProjectEntry) {
        self.projects.retain(|p| {
            if entry.project_path.is_none() && p.project_path.is_none() {
                // Standalone: dedup by library_path alone
                p.library_path != entry.library_path
            } else {
                // Project-attached: dedup by project_path + library_path pair
                !(p.project_path == entry.project_path && p.library_path == entry.library_path)
            }
        });
        self.projects.push(entry);
    }

    /// Remove a project by its project path.
    pub fn remove_by_path(&mut self, project_path: &str) {
        self.projects
            .retain(|p| p.project_path.as_deref() != Some(project_path));
    }

    /// Remove a standalone library entry by its library path.
    pub fn remove_by_library_path(&mut self, library_path: &str) {
        self.projects
            .retain(|p| !(p.project_path.is_none() && p.library_path == library_path));
    }

    /// Find a project entry by token.
    pub fn find_by_token(&self, token: &str) -> Option<&ProjectEntry> {
        self.projects.iter().find(|p| p.token == token)
    }
}

/// Runtime registry holding loaded libraries keyed by auth token.
pub struct ProjectRegistry {
    libraries: DashMap<String, Arc<LoadedLibrary>>,
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectRegistry {
    /// Create an empty runtime registry.
    pub fn new() -> Self {
        Self {
            libraries: DashMap::new(),
        }
    }

    /// Insert a loaded library under the given token.
    pub fn insert(&self, token: &str, library: LoadedLibrary) {
        self.libraries.insert(token.to_string(), Arc::new(library));
    }

    /// Build a runtime registry from a persisted registry, loading all libraries.
    pub fn from_persisted(persisted: &PersistedRegistry) -> Result<Self, RegistryError> {
        let libraries = DashMap::new();
        for entry in &persisted.projects {
            let library_path = PathBuf::from(&entry.library_path);
            match crate::server::load_library(&library_path) {
                Ok(library) => {
                    tracing::info!(
                        "Loaded library '{}' for project '{}' (token: {}...)",
                        library.name,
                        entry.name,
                        &entry.token[..entry.token.len().min(8)]
                    );
                    libraries.insert(entry.token.clone(), Arc::new(library));
                }
                Err(e) => {
                    tracing::error!("Failed to load library for project '{}': {}", entry.name, e);
                }
            }
        }
        Ok(Self { libraries })
    }

    /// Get a loaded library by auth token.
    pub fn get(&self, token: &str) -> Option<Arc<LoadedLibrary>> {
        self.libraries.get(token).map(|r| r.value().clone())
    }

    /// Reload a library for the given token from the given path.
    pub fn reload(&self, token: &str, library_path: &Path) -> Result<(), ServerError> {
        let library = crate::server::load_library(library_path)?;
        self.libraries.insert(token.to_string(), Arc::new(library));
        Ok(())
    }

    /// Remove a library by its auth token.
    pub fn remove(&self, token: &str) {
        self.libraries.remove(token);
    }

    /// Get all tokens currently registered.
    pub fn tokens(&self) -> Vec<String> {
        self.libraries.iter().map(|r| r.key().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_persisted_registry_save_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("projects.json");

        let mut registry = PersistedRegistry::default();
        registry.upsert(ProjectEntry {
            token: "abc123".to_string(),
            project_path: Some("/home/user/project1".to_string()),
            library_path: "/home/user/project1/libs/components".to_string(),
            name: "Project 1".to_string(),
            description: None,
        });

        registry.save(&path).unwrap();
        let loaded = PersistedRegistry::load(&path).unwrap();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].token, "abc123");
        assert_eq!(loaded.projects[0].name, "Project 1");
    }

    #[test]
    fn test_persisted_registry_upsert_replaces_same_path() {
        let mut registry = PersistedRegistry::default();
        registry.upsert(ProjectEntry {
            token: "token1".to_string(),
            project_path: Some("/project".to_string()),
            library_path: "/project/libs".to_string(),
            name: "Project".to_string(),
            description: None,
        });
        registry.upsert(ProjectEntry {
            token: "token2".to_string(),
            project_path: Some("/project".to_string()),
            library_path: "/project/libs".to_string(),
            name: "Project Updated".to_string(),
            description: None,
        });

        assert_eq!(registry.projects.len(), 1);
        assert_eq!(registry.projects[0].token, "token2");
        assert_eq!(registry.projects[0].name, "Project Updated");
    }

    #[test]
    fn test_persisted_registry_find_by_token() {
        let mut registry = PersistedRegistry::default();
        registry.upsert(ProjectEntry {
            token: "abc".to_string(),
            project_path: Some("/p1".to_string()),
            library_path: "/p1/libs".to_string(),
            name: "P1".to_string(),
            description: None,
        });
        registry.upsert(ProjectEntry {
            token: "def".to_string(),
            project_path: Some("/p2".to_string()),
            library_path: "/p2/libs".to_string(),
            name: "P2".to_string(),
            description: None,
        });

        assert_eq!(registry.find_by_token("abc").unwrap().name, "P1");
        assert_eq!(registry.find_by_token("def").unwrap().name, "P2");
        assert!(registry.find_by_token("xyz").is_none());
    }

    #[test]
    fn test_persisted_registry_remove_by_path() {
        let mut registry = PersistedRegistry::default();
        registry.upsert(ProjectEntry {
            token: "abc".to_string(),
            project_path: Some("/p1".to_string()),
            library_path: "/p1/libs".to_string(),
            name: "P1".to_string(),
            description: None,
        });
        registry.remove_by_path("/p1");
        assert!(registry.projects.is_empty());
    }

    #[test]
    fn test_standalone_upsert_dedup_by_library_path() {
        let mut registry = PersistedRegistry::default();
        registry.upsert(ProjectEntry {
            token: "token1".to_string(),
            project_path: None,
            library_path: "/libs/my-lib".to_string(),
            name: "My Lib".to_string(),
            description: None,
        });
        registry.upsert(ProjectEntry {
            token: "token2".to_string(),
            project_path: None,
            library_path: "/libs/my-lib".to_string(),
            name: "My Lib Updated".to_string(),
            description: None,
        });

        assert_eq!(registry.projects.len(), 1);
        assert_eq!(registry.projects[0].token, "token2");
    }

    #[test]
    fn test_remove_by_library_path() {
        let mut registry = PersistedRegistry::default();
        registry.upsert(ProjectEntry {
            token: "standalone".to_string(),
            project_path: None,
            library_path: "/libs/my-lib".to_string(),
            name: "My Lib".to_string(),
            description: None,
        });
        registry.upsert(ProjectEntry {
            token: "attached".to_string(),
            project_path: Some("/project".to_string()),
            library_path: "/libs/my-lib".to_string(),
            name: "My Lib".to_string(),
            description: None,
        });
        registry.remove_by_library_path("/libs/my-lib");
        // Only standalone entry removed, project-attached entry remains
        assert_eq!(registry.projects.len(), 1);
        assert_eq!(registry.projects[0].token, "attached");
    }

    #[test]
    fn test_load_nonexistent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let registry = PersistedRegistry::load(&path).unwrap();
        assert!(registry.projects.is_empty());
    }

    #[test]
    fn test_persisted_registry_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let path = tmp
            .path()
            .join("subdir")
            .join("nested")
            .join("projects.json");
        let registry = PersistedRegistry::default();
        registry.save(&path).unwrap();
        assert!(path.exists());
    }
}

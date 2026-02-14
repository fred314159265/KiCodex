use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("failed to read kicodex.yaml: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse kicodex.yaml: {0}")]
    Yaml(#[from] serde_yml::Error),
}

/// Top-level structure of kicodex.yaml in a KiCad project.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub libraries: Vec<LibraryRef>,
}

/// A reference to a library directory within the project.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryRef {
    pub name: String,
    pub path: String,
}

/// Load kicodex.yaml from the given project directory.
pub fn load_project_config(project_dir: &Path) -> Result<ProjectConfig, ProjectError> {
    let config_path = project_dir.join("kicodex.yaml");
    let content = std::fs::read_to_string(&config_path)?;
    let config: ProjectConfig = serde_yml::from_str(&content)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_project_config() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("kicodex.yaml"),
            r#"libraries:
  - name: components
    path: libs/components
"#,
        )
        .unwrap();

        let config = load_project_config(tmp.path()).unwrap();
        assert_eq!(config.libraries.len(), 1);
        assert_eq!(config.libraries[0].name, "components");
        assert_eq!(config.libraries[0].path, "libs/components");
    }

    #[test]
    fn test_load_project_config_multiple_libraries() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("kicodex.yaml"),
            r#"libraries:
  - name: components
    path: libs/components
  - name: connectors
    path: libs/connectors
"#,
        )
        .unwrap();

        let config = load_project_config(tmp.path()).unwrap();
        assert_eq!(config.libraries.len(), 2);
        assert_eq!(config.libraries[1].name, "connectors");
    }

    #[test]
    fn test_load_project_config_missing_file() {
        let tmp = TempDir::new().unwrap();
        let result = load_project_config(tmp.path());
        assert!(result.is_err());
    }
}

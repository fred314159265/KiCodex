use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("failed to read library.yaml: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse library.yaml: {0}")]
    Yaml(#[from] serde_yml::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(alias = "schemas_path")]
    pub templates_path: String,
    #[serde(alias = "tables")]
    pub component_types: Vec<ComponentTypeDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ComponentTypeDef {
    pub file: String,
    #[serde(alias = "schema")]
    pub template: String,
    pub name: String,
}

/// Load library.yaml from the given library root directory.
pub fn load_library_manifest(library_root: &Path) -> Result<LibraryManifest, LibraryError> {
    let manifest_path = library_root.join("library.yaml");
    let content = std::fs::read_to_string(&manifest_path)?;
    let manifest: LibraryManifest = serde_yml::from_str(&content)?;
    Ok(manifest)
}

/// Save a library manifest (library.yaml) to the given library root directory.
pub fn save_library_manifest(
    library_root: &Path,
    manifest: &LibraryManifest,
) -> Result<(), LibraryError> {
    let manifest_path = library_root.join("library.yaml");
    let yaml = serde_yml::to_string(manifest)?;
    std::fs::write(&manifest_path, yaml)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_library_manifest() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("library.yaml"),
            r#"name: "My Components Library"
schemas_path: schemas
tables:
  - file: data/resistors.csv
    schema: resistor
    name: "Resistors"
  - file: data/capacitors.csv
    schema: capacitor
    name: "Capacitors"
"#,
        )
        .unwrap();

        let manifest = load_library_manifest(tmp.path()).unwrap();
        assert_eq!(manifest.name, "My Components Library");
        assert_eq!(manifest.templates_path, "schemas");
        assert_eq!(manifest.component_types.len(), 2);
        assert_eq!(manifest.component_types[0].file, "data/resistors.csv");
        assert_eq!(manifest.component_types[0].template, "resistor");
        assert_eq!(manifest.component_types[0].name, "Resistors");
    }

    #[test]
    fn test_load_library_manifest_old_keys() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("library.yaml"),
            r#"name: "My Components Library"
schemas_path: schemas
tables:
  - file: data/resistors.csv
    schema: resistor
    name: "Resistors"
"#,
        )
        .unwrap();

        let manifest = load_library_manifest(tmp.path()).unwrap();
        assert_eq!(manifest.templates_path, "schemas");
        assert_eq!(manifest.component_types.len(), 1);
        assert_eq!(manifest.component_types[0].template, "resistor");
    }
}

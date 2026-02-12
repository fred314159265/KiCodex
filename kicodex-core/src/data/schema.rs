use indexmap::IndexMap;
use serde::Deserialize;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("failed to read schema file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse schema YAML: {0}")]
    Yaml(#[from] serde_yml::Error),
    #[error("base schema '_base.yaml' not found in {0}")]
    MissingBase(String),
    #[error("inherited schema '{0}' not found")]
    MissingParent(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawSchema {
    pub inherits: Option<String>,
    pub fields: IndexMap<String, FieldDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub display_name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,
}

/// A fully resolved schema with inherited fields merged in.
#[derive(Debug, Clone)]
pub struct ResolvedSchema {
    pub fields: IndexMap<String, FieldDef>,
}

/// Load and resolve a named schema from a schemas directory.
/// The `schema_name` should not include the `.yaml` extension.
pub fn load_schema(schemas_dir: &Path, schema_name: &str) -> Result<ResolvedSchema, SchemaError> {
    let base_path = schemas_dir.join("_base.yaml");
    let base: Option<RawSchema> = if base_path.exists() {
        let content = std::fs::read_to_string(&base_path)?;
        Some(serde_yml::from_str(&content)?)
    } else {
        None
    };

    if schema_name == "_base" {
        let base =
            base.ok_or_else(|| SchemaError::MissingBase(schemas_dir.display().to_string()))?;
        return Ok(ResolvedSchema {
            fields: base.fields,
        });
    }

    let schema_path = schemas_dir.join(format!("{schema_name}.yaml"));
    let content = std::fs::read_to_string(&schema_path)
        .map_err(|_| SchemaError::MissingParent(schema_name.to_string()))?;
    let raw: RawSchema = serde_yml::from_str(&content)?;

    let mut fields = IndexMap::new();

    // If this schema inherits from base, start with base fields
    if let Some(ref parent_name) = raw.inherits {
        if parent_name == "_base" {
            let base =
                base.ok_or_else(|| SchemaError::MissingBase(schemas_dir.display().to_string()))?;
            fields.extend(base.fields);
        } else {
            // Recursive inheritance could be supported here; for now just one level
            let parent = load_schema(schemas_dir, parent_name)?;
            fields.extend(parent.fields);
        }
    }

    // Type-specific fields override/extend base fields
    fields.extend(raw.fields);

    Ok(ResolvedSchema { fields })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_test_schemas(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("_base.yaml"),
            r#"fields:
  mpn:
    display_name: "MPN"
    required: true
  manufacturer:
    display_name: "Manufacturer"
    required: true
  description:
    display_name: "Description"
    required: true
  value:
    display_name: "Value"
    required: true
  symbol:
    display_name: "Symbol"
    required: true
    type: kicad_symbol
  footprint:
    display_name: "Footprint"
    required: true
    type: kicad_footprint
  datasheet:
    display_name: "Datasheet"
    required: false
    type: url
"#,
        )
        .unwrap();

        fs::write(
            dir.join("resistor.yaml"),
            r#"inherits: _base
fields:
  resistance:
    display_name: "Resistance"
    required: true
  tolerance:
    display_name: "Tolerance"
    required: false
  power_rating:
    display_name: "Power Rating"
    required: false
  package:
    display_name: "Package"
    required: true
"#,
        )
        .unwrap();
    }

    #[test]
    fn test_load_base_schema() {
        let tmp = TempDir::new().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        write_test_schemas(&schemas_dir);

        let schema = load_schema(&schemas_dir, "_base").unwrap();
        assert_eq!(schema.fields.len(), 7);
        assert!(schema.fields.contains_key("mpn"));
        assert!(schema.fields.contains_key("datasheet"));
    }

    #[test]
    fn test_load_inherited_schema() {
        let tmp = TempDir::new().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        write_test_schemas(&schemas_dir);

        let schema = load_schema(&schemas_dir, "resistor").unwrap();
        // 7 base fields + 4 resistor fields = 11
        assert_eq!(schema.fields.len(), 11);
        assert!(schema.fields.contains_key("mpn")); // from base
        assert!(schema.fields.contains_key("resistance")); // from resistor
        assert!(schema.fields.contains_key("package")); // from resistor

        // Verify field order: base fields first, then type-specific
        let keys: Vec<&String> = schema.fields.keys().collect();
        assert_eq!(keys[0], "mpn");
        assert_eq!(keys[7], "resistance");
    }

    #[test]
    fn test_missing_parent_schema() {
        let tmp = TempDir::new().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        write_test_schemas(&schemas_dir);

        let result = load_schema(&schemas_dir, "nonexistent");
        assert!(result.is_err());
    }
}

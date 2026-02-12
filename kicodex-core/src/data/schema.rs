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
    #[serde(default)]
    pub exclude_from_bom: bool,
    #[serde(default)]
    pub exclude_from_board: bool,
    #[serde(default)]
    pub exclude_from_sim: bool,
    pub fields: IndexMap<String, FieldDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub display_name: String,
    #[serde(default)]
    pub required: bool,
    /// Whether this field is visible on the schematic. Defaults to false;
    /// the server makes `value` and `reference` visible regardless.
    #[serde(default)]
    pub visible: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,
}


/// A fully resolved schema with inherited fields merged in.
#[derive(Debug, Clone)]
pub struct ResolvedSchema {
    pub exclude_from_bom: bool,
    pub exclude_from_board: bool,
    pub exclude_from_sim: bool,
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
            exclude_from_bom: base.exclude_from_bom,
            exclude_from_board: base.exclude_from_board,
            exclude_from_sim: base.exclude_from_sim,
            fields: base.fields,
        });
    }

    let schema_path = schemas_dir.join(format!("{schema_name}.yaml"));
    let content = std::fs::read_to_string(&schema_path)
        .map_err(|_| SchemaError::MissingParent(schema_name.to_string()))?;
    let raw: RawSchema = serde_yml::from_str(&content)?;

    let mut fields = IndexMap::new();
    let mut exclude_from_bom = raw.exclude_from_bom;
    let mut exclude_from_board = raw.exclude_from_board;
    let mut exclude_from_sim = raw.exclude_from_sim;

    // If this schema inherits from base, start with base values
    if let Some(ref parent_name) = raw.inherits {
        let parent = if parent_name == "_base" {
            let base =
                base.ok_or_else(|| SchemaError::MissingBase(schemas_dir.display().to_string()))?;
            ResolvedSchema {
                exclude_from_bom: base.exclude_from_bom,
                exclude_from_board: base.exclude_from_board,
                exclude_from_sim: base.exclude_from_sim,
                fields: base.fields,
            }
        } else {
            load_schema(schemas_dir, parent_name)?
        };
        fields.extend(parent.fields);
        // Child overrides parent only if explicitly set (non-default).
        // Since we can't distinguish "not set" from "set to false" with serde defaults,
        // the child's values always win.
        if !raw.exclude_from_bom {
            exclude_from_bom = parent.exclude_from_bom;
        }
        if !raw.exclude_from_board {
            exclude_from_board = parent.exclude_from_board;
        }
        if !raw.exclude_from_sim {
            exclude_from_sim = parent.exclude_from_sim;
        }
    }

    // Type-specific fields override/extend base fields
    fields.extend(raw.fields);

    Ok(ResolvedSchema {
        exclude_from_bom,
        exclude_from_board,
        exclude_from_sim,
        fields,
    })
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

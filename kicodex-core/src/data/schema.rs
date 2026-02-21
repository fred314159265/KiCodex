use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawSchema {
    #[serde(alias = "inherits")]
    pub based_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_from_bom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_from_board: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_from_sim: Option<bool>,
    pub fields: IndexMap<String, FieldDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
            exclude_from_bom: base.exclude_from_bom.unwrap_or(false),
            exclude_from_board: base.exclude_from_board.unwrap_or(false),
            exclude_from_sim: base.exclude_from_sim.unwrap_or(false),
            fields: base.fields,
        });
    }

    let schema_path = schemas_dir.join(format!("{schema_name}.yaml"));
    let content = std::fs::read_to_string(&schema_path)
        .map_err(|_| SchemaError::MissingParent(schema_name.to_string()))?;
    let raw: RawSchema = serde_yml::from_str(&content)?;

    let mut fields = IndexMap::new();
    let mut exclude_from_bom = raw.exclude_from_bom.unwrap_or(false);
    let mut exclude_from_board = raw.exclude_from_board.unwrap_or(false);
    let mut exclude_from_sim = raw.exclude_from_sim.unwrap_or(false);

    // If this schema inherits from a parent, start with parent values
    // and let child override only when explicitly set (Some).
    if let Some(ref parent_name) = raw.based_on {
        let parent = if parent_name == "_base" {
            let base =
                base.ok_or_else(|| SchemaError::MissingBase(schemas_dir.display().to_string()))?;
            ResolvedSchema {
                exclude_from_bom: base.exclude_from_bom.unwrap_or(false),
                exclude_from_board: base.exclude_from_board.unwrap_or(false),
                exclude_from_sim: base.exclude_from_sim.unwrap_or(false),
                fields: base.fields,
            }
        } else {
            load_schema(schemas_dir, parent_name)?
        };
        fields.extend(parent.fields);
        // None → inherit from parent; Some(value) → use child's explicit value
        exclude_from_bom = raw.exclude_from_bom.unwrap_or(parent.exclude_from_bom);
        exclude_from_board = raw.exclude_from_board.unwrap_or(parent.exclude_from_board);
        exclude_from_sim = raw.exclude_from_sim.unwrap_or(parent.exclude_from_sim);
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

/// Write a raw schema to a YAML file in the schemas directory.
pub fn write_schema(
    schemas_dir: &Path,
    name: &str,
    schema: &RawSchema,
) -> Result<(), SchemaError> {
    std::fs::create_dir_all(schemas_dir)?;
    let path = schemas_dir.join(format!("{}.yaml", name));
    let yaml = serde_yml::to_string(schema)?;
    std::fs::write(&path, yaml)?;
    Ok(())
}

// Aliases for the template-based naming convention
pub type RawTemplate = RawSchema;

/// Alias for `load_schema`.
pub fn load_template(schemas_dir: &Path, schema_name: &str) -> Result<ResolvedSchema, SchemaError> {
    load_schema(schemas_dir, schema_name)
}

/// Alias for `write_schema`.
pub fn write_template(schemas_dir: &Path, name: &str, schema: &RawSchema) -> Result<(), SchemaError> {
    write_schema(schemas_dir, name, schema)
}

/// Returns the default RawSchema used when scaffolding a new part table.
pub fn default_schema() -> RawSchema {
    let mut fields = IndexMap::new();
    fields.insert("value".to_string(),       FieldDef { display_name: "Value".to_string(),       required: true,  visible: true,  description: None, field_type: None });
    fields.insert("description".to_string(), FieldDef { display_name: "Description".to_string(), required: true,  visible: false, description: None, field_type: None });
    fields.insert("footprint".to_string(),   FieldDef { display_name: "Footprint".to_string(),   required: true,  visible: false, description: None, field_type: Some("kicad_footprint".to_string()) });
    fields.insert("symbol".to_string(),      FieldDef { display_name: "Symbol".to_string(),      required: true,  visible: false, description: None, field_type: Some("kicad_symbol".to_string()) });
    fields.insert("datasheet".to_string(),   FieldDef { display_name: "Datasheet".to_string(),   required: false, visible: false, description: None, field_type: Some("url".to_string()) });
    RawSchema { based_on: None, exclude_from_bom: None, exclude_from_board: None, exclude_from_sim: None, fields }
}

/// Returns the default CSV header row for a new part table (matches default_schema field order).
pub fn default_csv_headers() -> &'static str {
    "id,mpn,value,description,footprint,symbol,datasheet\n"
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
    display_name: "Name"
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
    fn test_load_inherited_template_old_key() {
        let tmp = TempDir::new().unwrap();
        let templates_dir = tmp.path().join("templates");
        fs::create_dir_all(&templates_dir).unwrap();
        fs::write(
            templates_dir.join("_base.yaml"),
            "fields:\n  mpn:\n    display_name: MPN\n    required: true\n",
        ).unwrap();
        fs::write(
            templates_dir.join("test.yaml"),
            "inherits: _base\nfields:\n  extra:\n    display_name: Extra\n",
        ).unwrap();

        let template = load_template(&templates_dir, "test").unwrap();
        assert_eq!(template.fields.len(), 2);
        assert!(template.fields.contains_key("mpn"));
        assert!(template.fields.contains_key("extra"));
    }

    #[test]
    fn test_missing_parent_template() {
        let tmp = TempDir::new().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        write_test_schemas(&schemas_dir);

        let result = load_schema(&schemas_dir, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_child_can_override_parent_bool_to_false() {
        let tmp = TempDir::new().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        // Parent sets exclude_from_bom: true
        fs::write(
            schemas_dir.join("_base.yaml"),
            "exclude_from_bom: true\nfields:\n  mpn:\n    display_name: MPN\n    required: true\n",
        ).unwrap();

        // Child explicitly sets exclude_from_bom: false
        fs::write(
            schemas_dir.join("child.yaml"),
            "based_on: _base\nexclude_from_bom: false\nfields:\n  extra:\n    display_name: Extra\n",
        ).unwrap();

        let schema = load_schema(&schemas_dir, "child").unwrap();
        assert!(!schema.exclude_from_bom, "child should override parent's true to false");
    }

    #[test]
    fn test_child_inherits_parent_bool_when_omitted() {
        let tmp = TempDir::new().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        // Parent sets exclude_from_bom: true
        fs::write(
            schemas_dir.join("_base.yaml"),
            "exclude_from_bom: true\nfields:\n  mpn:\n    display_name: MPN\n    required: true\n",
        ).unwrap();

        // Child omits exclude_from_bom entirely
        fs::write(
            schemas_dir.join("child.yaml"),
            "based_on: _base\nfields:\n  extra:\n    display_name: Extra\n",
        ).unwrap();

        let schema = load_schema(&schemas_dir, "child").unwrap();
        assert!(schema.exclude_from_bom, "child should inherit parent's true when field is omitted");
    }
}

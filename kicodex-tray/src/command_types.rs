use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub project_path: String,
    pub library_path: String,
    pub active: bool,
    pub table_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryInfo {
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub tables: Vec<TableInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub schema_name: String,
    pub row_count: usize,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableData {
    pub name: String,
    pub schema: SchemaInfo,
    pub rows: Vec<indexmap::IndexMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaInfo {
    pub inherits: Option<String>,
    pub exclude_from_bom: bool,
    pub exclude_from_board: bool,
    pub exclude_from_sim: bool,
    pub fields: Vec<FieldInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInfo {
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub visible: bool,
    pub description: Option<String>,
    pub field_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub library: String,
    pub tables: Vec<ValidationTableResult>,
    pub error_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationTableResult {
    pub name: String,
    pub file: String,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub row: Option<usize>,
    pub id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub name: String,
    pub path: String,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanProjectResult {
    pub has_config: bool,
    pub already_registered: bool,
    pub libraries: Vec<ScanResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddProjectResult {
    pub registered_count: usize,
    pub httplib_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredProject {
    pub project_path: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawSchemaInput {
    pub inherits: Option<String>,
    #[serde(default)]
    pub exclude_from_bom: bool,
    #[serde(default)]
    pub exclude_from_board: bool,
    #[serde(default)]
    pub exclude_from_sim: bool,
    pub fields: Vec<FieldInfo>,
}

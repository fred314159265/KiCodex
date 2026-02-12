use indexmap::IndexMap;
use serde::Serialize;

/// Response for GET /v1/
#[derive(Debug, Serialize)]
pub struct RootResponse {
    pub categories: String,
    pub parts: String,
}

/// A category (maps to a CSV table).
#[derive(Debug, Serialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A part summary (returned in category listing).
#[derive(Debug, Serialize)]
pub struct PartSummary {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A field value in the part detail response.
#[derive(Debug, Serialize)]
pub struct FieldValue {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<String>,
}

/// Full part detail response.
#[derive(Debug, Serialize)]
pub struct PartDetail {
    pub id: String,
    pub name: String,
    #[serde(rename = "symbolIdStr")]
    pub symbol_id_str: String,
    pub exclude_from_bom: String,
    pub exclude_from_board: String,
    pub exclude_from_sim: String,
    pub fields: IndexMap<String, FieldValue>,
}

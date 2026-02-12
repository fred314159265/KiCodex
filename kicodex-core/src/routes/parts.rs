use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use indexmap::IndexMap;

use crate::data::schema::ResolvedSchema;
use crate::models::{FieldValue, PartDetail, PartSummary};
use crate::server::AppState;

pub async fn get_parts_by_category(
    State(state): State<AppState>,
    Path(category_id): Path<String>,
) -> Result<Json<Vec<PartSummary>>, StatusCode> {
    let category_id = category_id.strip_suffix(".json").unwrap_or(&category_id);
    let idx: usize = category_id
        .parse::<usize>()
        .map_err(|_| StatusCode::NOT_FOUND)?
        .checked_sub(1)
        .ok_or(StatusCode::NOT_FOUND)?;

    let table = state.tables.get(idx).ok_or(StatusCode::NOT_FOUND)?;

    let parts: Vec<PartSummary> = table
        .rows
        .iter()
        .map(|row| {
            let id = row.get("id").cloned().unwrap_or_default();
            let name = row.get("mpn").cloned().unwrap_or_default();
            let description = row.get("description").cloned().unwrap_or_default();
            PartSummary {
                id,
                name,
                description,
            }
        })
        .collect();

    Ok(Json(parts))
}

pub async fn get_part_detail(
    State(state): State<AppState>,
    Path(part_id): Path<String>,
) -> Result<Json<PartDetail>, StatusCode> {
    let part_id = part_id.strip_suffix(".json").unwrap_or(&part_id);
    // Search all tables for the part
    for table in &state.tables {
        if let Some(row) = table
            .rows
            .iter()
            .find(|r| r.get("id").map(|s| s.as_str()) == Some(part_id))
        {
            return Ok(Json(build_part_detail(row, &table.schema)));
        }
    }

    Err(StatusCode::NOT_FOUND)
}

/// Special CSV columns that map to top-level API fields (not included in `fields` dict).
const TOP_LEVEL_COLUMNS: &[&str] = &["id", "symbol"];

/// Columns that become fields with visible=False.
const HIDDEN_COLUMNS: &[&str] = &[
    "footprint",
    "datasheet",
    "description",
    "manufacturer",
    "mpn",
];

/// Get the display name for a CSV column. Uses schema display_name if available,
/// otherwise uses the column name as-is.
fn display_name_for(column: &str, schema: &ResolvedSchema) -> String {
    schema
        .fields
        .get(column)
        .map(|f| f.display_name.clone())
        .unwrap_or_else(|| column.to_string())
}

fn build_part_detail(row: &IndexMap<String, String>, schema: &ResolvedSchema) -> PartDetail {
    let id = row.get("id").cloned().unwrap_or_default();
    let name = row.get("mpn").cloned().unwrap_or_default();
    let symbol_id_str = row.get("symbol").cloned().unwrap_or_default();

    let mut fields = IndexMap::new();

    // Add reference field if present
    if let Some(reference) = row.get("reference") {
        fields.insert(
            display_name_for("reference", schema),
            FieldValue {
                value: reference.clone(),
                visible: None,
            },
        );
    }

    // Add value field
    if let Some(value) = row.get("value") {
        fields.insert(
            display_name_for("value", schema),
            FieldValue {
                value: value.clone(),
                visible: None,
            },
        );
    }

    // Add all other fields
    for (key, value) in row {
        if TOP_LEVEL_COLUMNS.contains(&key.as_str()) || key == "value" || key == "reference" {
            continue;
        }

        let visible = if HIDDEN_COLUMNS.contains(&key.as_str()) {
            Some("False".to_string())
        } else {
            None
        };

        fields.insert(
            display_name_for(key, schema),
            FieldValue {
                value: value.clone(),
                visible,
            },
        );
    }

    PartDetail {
        id,
        name,
        symbol_id_str,
        exclude_from_bom: "False".to_string(),
        exclude_from_board: "False".to_string(),
        exclude_from_sim: "True".to_string(),
        fields,
    }
}

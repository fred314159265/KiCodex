use axum::extract::Path;
use axum::http::StatusCode;
use axum::{Extension, Json};
use indexmap::IndexMap;

use crate::data::schema::ResolvedSchema;
use crate::middleware::AuthenticatedLibrary;
use crate::models::{FieldValue, PartDetail, PartSummary};

pub async fn get_parts_by_category(
    Extension(AuthenticatedLibrary(library)): Extension<AuthenticatedLibrary>,
    Path(category_id): Path<String>,
) -> Result<Json<Vec<PartSummary>>, StatusCode> {
    let category_id = category_id.strip_suffix(".json").unwrap_or(&category_id);
    let idx: usize = category_id
        .parse::<usize>()
        .map_err(|_| StatusCode::NOT_FOUND)?
        .checked_sub(1)
        .ok_or(StatusCode::NOT_FOUND)?;

    let ct = library.part_tables.get(idx).ok_or(StatusCode::NOT_FOUND)?;

    let parts: Vec<PartSummary> = ct
        .components
        .iter()
        .map(|row| {
            let id = row.get("id").cloned().unwrap_or_default();
            let name = row.get("mpn").or_else(|| row.get("value")).cloned().unwrap_or_default();
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
    Extension(AuthenticatedLibrary(library)): Extension<AuthenticatedLibrary>,
    Path(part_id): Path<String>,
) -> Result<Json<PartDetail>, StatusCode> {
    let part_id = part_id.strip_suffix(".json").unwrap_or(&part_id);
    // Search all part tables for the part
    for ct in &library.part_tables {
        if let Some(row) = ct
            .components
            .iter()
            .find(|r| r.get("id").map(|s| s.as_str()) == Some(part_id))
        {
            return Ok(Json(build_part_detail(row, &ct.template)));
        }
    }

    Err(StatusCode::NOT_FOUND)
}

/// Special CSV columns that map to top-level API fields (not included in `fields` dict).
const TOP_LEVEL_COLUMNS: &[&str] = &[
    "id",
    "symbol",
    "exclude_from_bom",
    "exclude_from_board",
    "exclude_from_sim",
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

/// Columns that are visible by default (matching KiCad conventions).
const VISIBLE_BY_DEFAULT: &[&str] = &["value", "reference"];

/// Get the visibility for a CSV column.
/// `value` and `reference` are always visible. Everything else is hidden
/// unless the schema explicitly sets `visible: true`.
fn visible_for(column: &str, schema: &ResolvedSchema) -> Option<String> {
    if VISIBLE_BY_DEFAULT.contains(&column) {
        None
    } else {
        match schema.fields.get(column) {
            Some(field) if field.visible => None,
            _ => Some("False".to_string()),
        }
    }
}

/// Convert a bool to KiCad's expected string format.
fn bool_to_kicad(b: bool) -> String {
    if b { "True" } else { "False" }.to_string()
}

/// Normalize a CSV boolean string to KiCad format.
fn bool_str(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" => "True".to_string(),
        _ => "False".to_string(),
    }
}

fn exclude_flag(row: &IndexMap<String, String>, field: &str, schema_default: bool) -> String {
    row.get(field)
        .filter(|v| !v.is_empty())
        .map(|v| bool_str(v))
        .unwrap_or_else(|| bool_to_kicad(schema_default))
}

fn build_part_detail(row: &IndexMap<String, String>, schema: &ResolvedSchema) -> PartDetail {
    let id = row.get("id").cloned().unwrap_or_default();
    let name = row.get("mpn").or_else(|| row.get("value")).cloned().unwrap_or_default();
    let symbol_id_str = row.get("symbol").cloned().unwrap_or_default();

    let mut fields = IndexMap::new();

    for (key, value) in row {
        if TOP_LEVEL_COLUMNS.contains(&key.as_str()) {
            continue;
        }

        fields.insert(
            display_name_for(key, schema),
            FieldValue {
                value: value.clone(),
                visible: visible_for(key, schema),
            },
        );
    }

    // Exclude flags: CSV column overrides schema default, which defaults to false
    let exclude_from_bom   = exclude_flag(row, "exclude_from_bom",   schema.exclude_from_bom);
    let exclude_from_board = exclude_flag(row, "exclude_from_board",  schema.exclude_from_board);
    let exclude_from_sim   = exclude_flag(row, "exclude_from_sim",    schema.exclude_from_sim);

    PartDetail {
        id,
        name,
        symbol_id_str,
        exclude_from_bom,
        exclude_from_board,
        exclude_from_sim,
        fields,
    }
}

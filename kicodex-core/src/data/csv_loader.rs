use indexmap::IndexMap;
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;
use tracing::warn;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CsvError {
    #[error("failed to read CSV file: {0}")]
    Io(#[from] std::io::Error),
    #[error("CSV parsing error: {0}")]
    Csv(#[from] csv::Error),
    #[error("CSV file has no headers")]
    NoHeaders,
}

/// A single row of CSV data, preserving column order via IndexMap.
pub type CsvRow = IndexMap<String, String>;

/// Load a CSV file, ensuring every row has a unique `id`.
/// Missing or duplicate IDs are auto-assigned and written back to disk.
pub fn load_csv_with_ids(path: &Path) -> Result<Vec<CsvRow>, CsvError> {
    let content = std::fs::read_to_string(path)?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(content.as_bytes());

    let headers: Vec<String> = reader
        .headers()
        .map_err(CsvError::Csv)?
        .iter()
        .map(|h| h.to_string())
        .collect();

    if headers.is_empty() {
        return Err(CsvError::NoHeaders);
    }

    let has_id_column = headers.iter().any(|h| h == "id");

    let mut rows: Vec<CsvRow> = Vec::new();
    for result in reader.records() {
        let record = result?;
        let mut row = IndexMap::new();
        for (i, header) in headers.iter().enumerate() {
            let value = record.get(i).unwrap_or("").to_string();
            row.insert(header.clone(), value);
        }
        rows.push(row);
    }

    // Collect existing IDs to detect duplicates
    let mut used_ids: HashSet<String> = HashSet::new();
    let mut needs_writeback = false;

    if has_id_column {
        for row in &rows {
            if let Some(id) = row.get("id") {
                if !id.is_empty() {
                    used_ids.insert(id.clone());
                }
            }
        }
    }

    // Assign IDs to rows that don't have one, or have duplicates
    let mut seen_ids: HashSet<String> = HashSet::new();
    for row in &mut rows {
        if !has_id_column {
            // Need to insert id as first column
            needs_writeback = true;
            let id = Uuid::new_v4().to_string();

            // Rebuild row with id first
            let mut new_row = IndexMap::new();
            new_row.insert("id".to_string(), id.clone());
            for (k, v) in row.iter() {
                new_row.insert(k.clone(), v.clone());
            }
            *row = new_row;
            seen_ids.insert(id);
        } else {
            let id = row.get("id").cloned().unwrap_or_default();
            if id.is_empty() || seen_ids.contains(&id) {
                if !id.is_empty() {
                    warn!("duplicate id '{}' detected, assigning new id", id);
                }
                let new_id = Uuid::new_v4().to_string();
                used_ids.insert(new_id.clone());
                row.insert("id".to_string(), new_id.clone());
                seen_ids.insert(new_id);
                needs_writeback = true;
            } else {
                seen_ids.insert(id);
            }
        }
    }

    if needs_writeback {
        write_csv(path, &rows)?;
    }

    Ok(rows)
}

/// Write rows back to a CSV file using temp file + rename for safety.
pub fn write_csv(path: &Path, rows: &[CsvRow]) -> Result<(), CsvError> {
    if rows.is_empty() {
        return Ok(());
    }

    let headers: Vec<String> = rows[0].keys().cloned().collect();

    // Write to a temp file in the same directory, then rename
    let parent = path.parent().unwrap_or(Path::new("."));
    let temp_path = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));

    let mut writer = csv::WriterBuilder::new().from_path(&temp_path)?;
    writer.write_record(&headers)?;

    for row in rows {
        let record: Vec<&str> = headers
            .iter()
            .map(|h| row.get(h).map(|s| s.as_str()).unwrap_or(""))
            .collect();
        writer.write_record(&record)?;
    }

    writer.flush()?;
    drop(writer);

    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Append a new row to a CSV file, auto-assigning an ID.
/// Returns the assigned ID.
pub fn append_row(path: &Path, fields: &CsvRow) -> Result<String, CsvError> {
    let mut rows = load_csv_with_ids(path)?;

    let new_id = Uuid::new_v4().to_string();

    // Build new row with id first, then existing columns from first row as template
    let mut new_row = IndexMap::new();
    new_row.insert("id".to_string(), new_id.clone());

    if let Some(first) = rows.first() {
        for key in first.keys() {
            if key != "id" {
                let value = fields.get(key).cloned().unwrap_or_default();
                new_row.insert(key.clone(), value);
            }
        }
    }
    // Add any extra fields not already in headers
    for (key, value) in fields {
        if !new_row.contains_key(key) && key != "id" {
            new_row.insert(key.clone(), value.clone());
        }
    }

    rows.push(new_row);
    write_csv(path, &rows)?;
    Ok(new_id)
}

/// Update an existing row by ID.
pub fn update_row(path: &Path, id: &str, fields: &CsvRow) -> Result<(), CsvError> {
    let mut rows = load_csv_with_ids(path)?;

    let row = rows
        .iter_mut()
        .find(|r| r.get("id").map(|v| v.as_str()) == Some(id));

    match row {
        Some(row) => {
            for (key, value) in fields {
                if key != "id" {
                    row.insert(key.clone(), value.clone());
                }
            }
            write_csv(path, &rows)?;
            Ok(())
        }
        None => Err(row_not_found(id)),
    }
}

/// Rename columns in a CSV file. Each entry in `renames` is `(old_key, new_key)`.
/// Columns not found are silently skipped. Writes back via `write_csv`.
pub fn rename_csv_columns(path: &Path, renames: &[(String, String)]) -> Result<(), CsvError> {
    if renames.is_empty() || !path.exists() {
        return Ok(());
    }

    let mut rows = load_csv_with_ids(path)?;
    if rows.is_empty() {
        return Ok(());
    }

    // Build a rename map from old_key -> new_key
    let rename_map: std::collections::HashMap<&str, &str> = renames
        .iter()
        .map(|(old, new)| (old.as_str(), new.as_str()))
        .collect();

    // Rewrite each row's keys
    for row in &mut rows {
        let old_row = std::mem::take(row);
        for (key, value) in old_row {
            let new_key = match rename_map.get(key.as_str()) {
                Some(&renamed) => renamed.to_string(),
                None => key,
            };
            row.insert(new_key, value);
        }
    }

    write_csv(path, &rows)
}

/// Remove columns from a CSV file. Columns not found are silently skipped.
pub fn remove_csv_columns(path: &Path, columns: &[String]) -> Result<(), CsvError> {
    if columns.is_empty() || !path.exists() {
        return Ok(());
    }

    let mut rows = load_csv_with_ids(path)?;
    if rows.is_empty() {
        return Ok(());
    }

    let to_remove: HashSet<&str> = columns.iter().map(|s| s.as_str()).collect();

    for row in &mut rows {
        row.retain(|key, _| !to_remove.contains(key.as_str()));
    }

    write_csv(path, &rows)
}

fn row_not_found(id: &str) -> CsvError {
    CsvError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("row with id '{}' not found", id),
    ))
}

// Aliases for component-based naming convention
pub fn append_component(path: &Path, fields: &CsvRow) -> Result<String, CsvError> {
    append_row(path, fields)
}

pub fn update_component(path: &Path, id: &str, fields: &CsvRow) -> Result<(), CsvError> {
    update_row(path, id, fields)
}

pub fn delete_component(path: &Path, id: &str) -> Result<(), CsvError> {
    delete_row(path, id)
}

/// Delete a row by ID.
pub fn delete_row(path: &Path, id: &str) -> Result<(), CsvError> {
    let mut rows = load_csv_with_ids(path)?;
    let original_len = rows.len();
    rows.retain(|r| r.get("id").map(|v| v.as_str()) != Some(id));

    if rows.len() == original_len {
        return Err(row_not_found(id));
    }

    write_csv(path, &rows)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_csv_with_existing_ids() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "id,mpn,value\n1,RC0603FR-0710KL,10K\n2,RC0603FR-07100KL,100K\n",
        )
        .unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], "1");
        assert_eq!(rows[1]["id"], "2");

        // File should not have been rewritten (no changes needed)
        let content = fs::read_to_string(&csv_path).unwrap();
        assert!(content.contains("1,RC0603FR-0710KL,10K"));
    }

    #[test]
    fn test_load_csv_assigns_missing_ids() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "id,mpn,value\n1,RC0603FR-0710KL,10K\n,RC0603FR-07100KL,100K\n",
        )
        .unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], "1");
        // auto-assigned ID should be a non-empty UUID
        let auto_id = &rows[1]["id"];
        assert!(!auto_id.is_empty());
        assert!(uuid::Uuid::parse_str(auto_id).is_ok(), "expected UUID, got {}", auto_id);

        // Verify writeback contains the auto-assigned ID
        let content = fs::read_to_string(&csv_path).unwrap();
        assert!(content.contains(&format!("{},RC0603FR-07100KL,100K", auto_id)));
    }

    #[test]
    fn test_load_csv_handles_duplicate_ids() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "id,mpn,value\n1,RC0603FR-0710KL,10K\n1,RC0603FR-07100KL,100K\n",
        )
        .unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], "1");
        assert_ne!(rows[1]["id"], "1"); // should get a new id
    }

    #[test]
    fn test_load_csv_without_id_column() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "mpn,value\nRC0603FR-0710KL,10K\nRC0603FR-07100KL,100K\n",
        )
        .unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!rows[0]["id"].is_empty());
        assert!(!rows[1]["id"].is_empty());

        // id should be first column
        let keys: Vec<&String> = rows[0].keys().collect();
        assert_eq!(keys[0], "id");

        // Verify writeback includes id column
        let content = fs::read_to_string(&csv_path).unwrap();
        assert!(content.starts_with("id,mpn,value"));
    }

    #[test]
    fn test_rename_csv_columns() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "id,mpn,value\n1,RC0603FR-0710KL,10K\n2,RC0603FR-07100KL,100K\n",
        )
        .unwrap();

        rename_csv_columns(
            &csv_path,
            &[("mpn".to_string(), "manufacturer_pn".to_string())],
        )
        .unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains_key("manufacturer_pn"));
        assert!(!rows[0].contains_key("mpn"));
        assert_eq!(rows[0]["manufacturer_pn"], "RC0603FR-0710KL");
        assert_eq!(rows[1]["manufacturer_pn"], "RC0603FR-07100KL");
        // Other columns unchanged
        assert_eq!(rows[0]["value"], "10K");
    }

    #[test]
    fn test_rename_csv_columns_missing_column() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "id,mpn,value\n1,RC0603FR-0710KL,10K\n",
        )
        .unwrap();

        // Renaming a non-existent column should succeed silently
        rename_csv_columns(
            &csv_path,
            &[("nonexistent".to_string(), "something".to_string())],
        )
        .unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows[0]["mpn"], "RC0603FR-0710KL");
    }

    #[test]
    fn test_remove_csv_columns() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "id,mpn,value,description\n1,RC0603FR-0710KL,10K,some desc\n2,RC0603FR-07100KL,100K,other desc\n",
        )
        .unwrap();

        remove_csv_columns(&csv_path, &["description".to_string()]).unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].contains_key("description"));
        assert_eq!(rows[0]["mpn"], "RC0603FR-0710KL");
        assert_eq!(rows[0]["value"], "10K");
    }

    #[test]
    fn test_remove_csv_columns_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        fs::write(
            &csv_path,
            "id,mpn,value\n1,RC0603FR-0710KL,10K\n",
        )
        .unwrap();

        // Removing a non-existent column should succeed silently
        remove_csv_columns(&csv_path, &["nonexistent".to_string()]).unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows[0]["mpn"], "RC0603FR-0710KL");
    }

    #[test]
    fn test_rename_and_remove_on_missing_file() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("does_not_exist.csv");

        // Both should succeed silently on missing files
        rename_csv_columns(&csv_path, &[("a".to_string(), "b".to_string())]).unwrap();
        remove_csv_columns(&csv_path, &["a".to_string()]).unwrap();
    }

    #[test]
    fn test_round_trip_preserves_data() {
        let tmp = TempDir::new().unwrap();
        let csv_path = tmp.path().join("test.csv");
        let original = "id,mpn,value,description\n1,RC0603FR-0710KL,10K,\"RES 10K OHM 1% 1/10W 0603\"\n2,RC0603FR-07100KL,100K,\"RES 100K OHM 1% 1/10W 0603\"\n";
        fs::write(&csv_path, original).unwrap();

        let rows = load_csv_with_ids(&csv_path).unwrap();
        assert_eq!(rows[0]["description"], "RES 10K OHM 1% 1/10W 0603");
        assert_eq!(rows[1]["description"], "RES 100K OHM 1% 1/10W 0603");
    }
}

use std::collections::HashSet;
use std::path::PathBuf;

use tauri::State;

use crate::command_types::*;
use crate::AppState;

/// Reload the in-memory registry entry for a library at the given path.
/// Finds the matching token from the persisted registry and calls registry.reload().
fn reload_registry_for_path(state: &AppState, lib_path: &std::path::Path) {
    let lib_path_str = lib_path.to_string_lossy();
    let persisted = state.persisted.lock().unwrap();
    for entry in &persisted.projects {
        // Match by library_path (normalize both for comparison)
        let entry_path = std::path::Path::new(&entry.library_path);
        if entry_path == lib_path
            || entry.library_path == lib_path_str.as_ref()
            || entry_path
                .canonicalize()
                .ok()
                .as_deref()
                == lib_path.canonicalize().ok().as_deref()
        {
            if let Err(e) = state.registry.reload(&entry.token, lib_path) {
                tracing::warn!("Failed to reload library in registry: {}", e);
            }
            return;
        }
    }
}

#[tauri::command]
pub fn remove_project(state: State<'_, AppState>, project_path: String) -> Result<usize, String> {
    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| "Could not determine config directory".to_string())?;

    let mut persisted = state.persisted.lock().unwrap();

    // Collect entries matching this project path
    let entries: Vec<_> = persisted
        .projects
        .iter()
        .filter(|p| p.project_path == project_path)
        .cloned()
        .collect();

    let count = entries.len();

    // Remove from runtime registry and delete .kicad_httplib files
    for entry in &entries {
        state.registry.remove(&entry.token);

        // Delete .kicad_httplib from the library directory
        let lib_dir = PathBuf::from(&entry.library_path);
        let httplib_in_lib = lib_dir.join(format!("{}.kicad_httplib", entry.name));
        let _ = std::fs::remove_file(&httplib_in_lib);

        // Delete .kicad_httplib from the project directory
        let proj_dir = PathBuf::from(&entry.project_path);
        let httplib_in_proj = proj_dir.join(format!("{}.kicad_httplib", entry.name));
        let _ = std::fs::remove_file(&httplib_in_proj);
    }

    // Remove from persisted registry and save
    persisted.remove_by_path(&project_path);
    persisted.save(&registry_path).map_err(|e| e.to_string())?;

    Ok(count)
}

#[tauri::command]
pub fn get_projects(state: State<'_, AppState>) -> Result<Vec<ProjectInfo>, String> {
    let persisted = state.persisted.lock().unwrap();
    let active = state.active_projects.lock().unwrap();
    let active_paths: HashSet<String> = active
        .iter()
        .map(|a| a.project_path.to_string_lossy().to_string())
        .collect();

    let mut projects = Vec::new();
    for entry in &persisted.projects {
        let part_table_count = state
            .registry
            .get(&entry.token)
            .map(|lib| lib.part_tables.len())

            .unwrap_or(0);

        let clean_lib_path = entry.library_path
            .strip_prefix(r"\\?\")
            .unwrap_or(&entry.library_path)
            .to_string();

        projects.push(ProjectInfo {
            name: entry.name.clone(),
            project_path: entry.project_path.clone(),
            library_path: clean_lib_path,
            active: active_paths.contains(&entry.project_path),
            part_table_count,
        });
    }
    Ok(projects)
}

#[tauri::command]
pub fn get_project_libraries(
    state: State<'_, AppState>,
    project_path: String,
) -> Result<Vec<LibraryInfo>, String> {
    let persisted = state.persisted.lock().unwrap();
    let entries: Vec<_> = persisted
        .projects
        .iter()
        .filter(|p| p.project_path == project_path)
        .collect();

    let mut libraries = Vec::new();
    for entry in entries {
        // Strip Windows extended-length path prefix for clean paths
        let clean_path = entry.library_path
            .strip_prefix(r"\\?\")
            .unwrap_or(&entry.library_path)
            .to_string();
        let library_root = PathBuf::from(&clean_path);

        // Load fresh from disk to always reflect current state
        let lib = match kicodex_core::server::load_library(&library_root) {
            Ok(lib) => lib,
            Err(e) => {
                tracing::warn!("Failed to load library at {}: {}", clean_path, e);
                continue;
            }
        };

        let manifest = kicodex_core::data::library::load_library_manifest(&library_root).ok();

        let ct_files: std::collections::HashMap<String, String> = manifest
            .as_ref()
            .map(|m| m.part_tables.iter().map(|t| (t.name.clone(), t.file.clone())).collect())
            .unwrap_or_default();

        let part_tables = lib
            .part_tables
            .iter()
            .map(|ct| PartTableInfo {
                name: ct.name.clone(),
                template_name: ct.template_name.clone(),
                component_count: ct.components.len(),
                file: ct_files.get(&ct.name).cloned().unwrap_or_default(),
            })
            .collect();

        libraries.push(LibraryInfo {
            name: lib.name.clone(),
            path: clean_path,
            description: lib.description.clone(),
            part_tables,
        });
    }
    Ok(libraries)
}

#[tauri::command]
pub fn scan_for_libraries(path: String) -> Result<Vec<ScanResult>, String> {
    let scan_dir = PathBuf::from(&path);
    let pattern = format!("{}/**/library.yaml", scan_dir.display());
    let entries: Vec<PathBuf> = glob::glob(&pattern)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();

    // Load existing config if present
    let existing_names: HashSet<String> = if scan_dir.join("kicodex.yaml").exists() {
        kicodex_core::data::project::load_project_config(&scan_dir)
            .map(|c| c.libraries.iter().map(|l| l.name.clone()).collect())
            .unwrap_or_default()
    } else {
        HashSet::new()
    };

    let mut results = Vec::new();
    for entry in &entries {
        let lib_dir = entry.parent().unwrap();
        if let Ok(manifest) =
            kicodex_core::data::library::load_library_manifest(lib_dir)
        {
            let rel_path =
                pathdiff::diff_paths(lib_dir, &scan_dir).unwrap_or_else(|| lib_dir.to_path_buf());
            let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");

            let is_new = !existing_names.contains(&manifest.name);
            results.push(ScanResult {
                name: manifest.name,
                path: rel_path_str,
                is_new,
            });
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn apply_scan(path: String, libraries: Vec<ScanResult>) -> Result<(), String> {
    let scan_dir = PathBuf::from(&path);
    let config_path = scan_dir.join("kicodex.yaml");

    let mut config = if config_path.exists() {
        kicodex_core::data::project::load_project_config(&scan_dir).map_err(|e| e.to_string())?
    } else {
        kicodex_core::data::project::ProjectConfig {
            libraries: Vec::new(),
        }
    };

    let existing: HashSet<String> = config.libraries.iter().map(|l| l.name.clone()).collect();

    for lib in &libraries {
        if lib.is_new && !existing.contains(&lib.name) {
            config
                .libraries
                .push(kicodex_core::data::project::LibraryRef {
                    name: lib.name.clone(),
                    path: lib.path.clone(),
                });
        }
    }

    let yaml = serde_yml::to_string(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, yaml).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn validate_library(
    lib_path: String,
    project_path: Option<String>,
) -> Result<ValidationResult, String> {
    let library_root = PathBuf::from(&lib_path);
    let library =
        kicodex_core::server::load_library(&library_root).map_err(|e| e.to_string())?;
    let manifest = kicodex_core::data::library::load_library_manifest(&library_root)
        .map_err(|e| e.to_string())?;

    let project_dir = project_path.map(PathBuf::from);
    let kicad_libs =
        kicodex_core::data::kicad_libs::KicadLibraries::load(project_dir.as_deref()).ok();

    let ct_files: std::collections::HashMap<String, String> = manifest
        .part_tables
        .iter()
        .map(|t| (t.name.clone(), t.file.clone()))
        .collect();

    let mut ct_results = Vec::new();

    for ct in &library.part_tables {
        let csv_file = ct_files
            .get(&ct.name)
            .cloned()
            .unwrap_or_else(|| ct.template_name.clone());

        let csv_headers: HashSet<&String> = ct
            .components
            .first()
            .map(|r| r.keys().collect())
            .unwrap_or_default();

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check required fields as columns
        for (field_name, field_def) in &ct.template.fields {
            if field_def.required && !csv_headers.contains(field_name) {
                errors.push(ValidationIssue {
                    row: None,
                    id: None,
                    message: format!(
                        "required field '{}' is missing from CSV columns",
                        field_name
                    ),
                });
            }
        }

        // Per-row checks
        let mut seen_ids: HashSet<String> = HashSet::new();

        for (row_idx, row) in ct.components.iter().enumerate() {
            let row_num = row_idx + 1;
            let row_id = row.get("id").cloned().unwrap_or_default();

            if !row_id.is_empty() && !seen_ids.insert(row_id.clone()) {
                errors.push(ValidationIssue {
                    row: Some(row_num),
                    id: Some(row_id.clone()),
                    message: format!("duplicate id '{}'", row_id),
                });
            }

            for (field_name, field_def) in &ct.template.fields {
                let value = row.get(field_name).map(|s| s.as_str()).unwrap_or("");
                let field_type = field_def.field_type.as_deref();

                if field_def.required && value.is_empty() && csv_headers.contains(field_name) {
                    errors.push(ValidationIssue {
                        row: Some(row_num),
                        id: Some(row_id.clone()),
                        message: format!(
                            "required field '{}' is empty",
                            field_def.display_name
                        ),
                    });
                    continue;
                }

                if value.is_empty() {
                    continue;
                }

                // kicad_symbol / kicad_footprint format check
                if matches!(field_type, Some("kicad_symbol") | Some("kicad_footprint")) {
                    let colon_count = value.chars().filter(|&c| c == ':').count();
                    if colon_count != 1 {
                        let issue = ValidationIssue {
                            row: Some(row_num),
                            id: Some(row_id.clone()),
                            message: format!(
                                "field '{}' has invalid {} format '{}' (expected 'Library:Name')",
                                field_def.display_name,
                                field_type.unwrap(),
                                value
                            ),
                        };
                        if field_def.required {
                            errors.push(issue);
                        } else {
                            warnings.push(issue);
                        }
                    } else if let Some(ref klibs) = kicad_libs {
                        use kicodex_core::data::kicad_libs::LibLookup;
                        let result = if field_type == Some("kicad_symbol") {
                            klibs.has_symbol(value)
                        } else {
                            klibs.has_footprint(value)
                        };
                        let kind = if field_type == Some("kicad_symbol") {
                            "symbol"
                        } else {
                            "footprint"
                        };
                        match result {
                            LibLookup::Found => {}
                            LibLookup::LibraryNotFound(lib) => {
                                warnings.push(ValidationIssue {
                                    row: Some(row_num),
                                    id: Some(row_id.clone()),
                                    message: format!(
                                        "{} library '{}' not found in lib tables",
                                        kind, lib
                                    ),
                                });
                            }
                            LibLookup::EntryNotFound(lib, entry) => {
                                warnings.push(ValidationIssue {
                                    row: Some(row_num),
                                    id: Some(row_id.clone()),
                                    message: format!(
                                        "{} '{}' not found in library '{}'",
                                        kind, entry, lib
                                    ),
                                });
                            }
                            LibLookup::LibraryUnreadable(_) => {}
                        }
                    }
                }

                // URL format check
                if field_type == Some("url")
                    && !value.starts_with("http://")
                    && !value.starts_with("https://")
                {
                    let issue = ValidationIssue {
                        row: Some(row_num),
                        id: Some(row_id.clone()),
                        message: format!(
                            "field '{}' has invalid URL '{}' (must start with http:// or https://)",
                            field_def.display_name, value
                        ),
                    };
                    if field_def.required {
                        errors.push(issue);
                    } else {
                        warnings.push(issue);
                    }
                }
            }
        }

        ct_results.push(ValidationPartTableResult {
            name: ct.name.clone(),
            file: csv_file,
            errors,
            warnings,
        });
    }

    let error_count: usize = ct_results.iter().map(|t| t.errors.len()).sum();
    let warning_count: usize = ct_results.iter().map(|t| t.warnings.len()).sum();

    Ok(ValidationResult {
        library: library.name,
        part_tables: ct_results,
        error_count,
        warning_count,
    })
}

#[tauri::command]
pub fn init_project(
    state: State<'_, AppState>,
    project_path: String,
) -> Result<String, String> {
    let project_dir = PathBuf::from(&project_path);
    let config = kicodex_core::data::project::load_project_config(&project_dir)
        .map_err(|e| e.to_string())?;

    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| "Could not determine config directory".to_string())?;

    let mut persisted = state.persisted.lock().unwrap();
    let port = state.port;

    let mut count = 0;
    for lib_ref in &config.libraries {
        let library_path = project_dir.join(&lib_ref.path);
        let library_path = library_path
            .canonicalize()
            .unwrap_or_else(|_| library_path.clone());

        let library =
            kicodex_core::server::load_library(&library_path).map_err(|e| e.to_string())?;

        let token = uuid::Uuid::new_v4().to_string();
        let description = library.description.clone();
        let fallback = format!("KiCodex HTTP Library for {}", lib_ref.name);
        let desc_str = description.as_deref().unwrap_or(&fallback);

        persisted.upsert(kicodex_core::registry::ProjectEntry {
            token: token.clone(),
            project_path: project_dir.to_string_lossy().to_string(),
            library_path: library_path.to_string_lossy().to_string(),
            name: lib_ref.name.clone(),
            description: description.clone(),
        });

        // Write .kicad_httplib
        let httplib_path = project_dir.join(format!("{}.kicad_httplib", lib_ref.name));
        let httplib_content = format!(
            r#"{{
    "meta": {{
        "version": 1.0
    }},
    "name": "{}",
    "description": "{}",
    "source": {{
        "type": "REST_API",
        "api_version": "v1",
        "root_url": "http://127.0.0.1:{port}",
        "token": "{token}"
    }}
}}"#,
            lib_ref.name, desc_str
        );
        std::fs::write(&httplib_path, &httplib_content).map_err(|e| e.to_string())?;

        // Insert into runtime registry
        state.registry.insert(&token, library);
        count += 1;
    }

    persisted.save(&registry_path).map_err(|e| e.to_string())?;

    Ok(format!("Initialized {} library/libraries", count))
}

#[tauri::command]
pub fn create_library(name: String, parent_dir: String) -> Result<String, String> {
    let lib_dir = PathBuf::from(&parent_dir).join(&name);
    if lib_dir.join("library.yaml").exists() {
        return Err(format!("Library '{}' already exists", name));
    }

    std::fs::create_dir_all(&lib_dir).map_err(|e| e.to_string())?;
    let templates_dir = lib_dir.join("schemas");
    std::fs::create_dir_all(&templates_dir).map_err(|e| e.to_string())?;

    // Write library.yaml with no part tables — user adds them afterward
    let manifest = kicodex_core::data::library::LibraryManifest {
        name: name.clone(),
        description: Some(format!("KiCodex library: {}", name)),
        templates_path: "schemas".to_string(),
        part_tables: Vec::new(),
    };
    kicodex_core::data::library::save_library_manifest(&lib_dir, &manifest)
        .map_err(|e| e.to_string())?;

    Ok(lib_dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn add_part_table(
    state: State<'_, AppState>,
    lib_path: String,
    component_type_name: String,
    template: Option<RawTemplateInput>,
) -> Result<(), String> {
    let lib_dir = PathBuf::from(&lib_path);
    let mut manifest = kicodex_core::data::library::load_library_manifest(&lib_dir)
        .map_err(|e| e.to_string())?;

    if manifest.part_tables.iter().any(|t| t.template == component_type_name) {
        return Err(format!("Part table '{}' already exists", component_type_name));
    }

    let templates_dir = lib_dir.join(&manifest.templates_path);
    std::fs::create_dir_all(&templates_dir).map_err(|e| e.to_string())?;

    if let Some(ref tmpl) = template {
        // Build template YAML from user-provided fields
        let mut fields = indexmap::IndexMap::new();
        for f in &tmpl.fields {
            fields.insert(
                f.key.clone(),
                kicodex_core::data::schema::FieldDef {
                    display_name: f.display_name.clone(),
                    required: f.required,
                    visible: f.visible,
                    description: f.description.clone(),
                    field_type: f.field_type.clone(),
                },
            );
        }

        let raw = kicodex_core::data::schema::RawTemplate {
            based_on: tmpl.based_on.clone(),
            exclude_from_bom: tmpl.exclude_from_bom,
            exclude_from_board: tmpl.exclude_from_board,
            exclude_from_sim: tmpl.exclude_from_sim,
            fields,
        };

        kicodex_core::data::schema::write_template(&templates_dir, &component_type_name, &raw)
            .map_err(|e| e.to_string())?;

        // Derive CSV headers from template fields
        let mut headers = vec!["id".to_string(), "mpn".to_string()];
        for f in &tmpl.fields {
            if f.key != "id" && f.key != "mpn" {
                headers.push(f.key.clone());
            }
        }
        let csv_header = headers.join(",") + "\n";
        std::fs::write(
            lib_dir.join(format!("{}.csv", component_type_name)),
            csv_header,
        )
        .map_err(|e| e.to_string())?;
    } else {
        // Default template (CLI compat)
        let template_content = "fields:\n  value:\n    display_name: Name\n    visible: true\n  description:\n    display_name: Description\n    visible: true\n  footprint:\n    display_name: Footprint\n    visible: true\n    type: kicad_footprint\n  symbol:\n    display_name: Symbol\n    visible: true\n    type: kicad_symbol\n";
        std::fs::write(
            templates_dir.join(format!("{}.yaml", component_type_name)),
            template_content,
        )
        .map_err(|e| e.to_string())?;

        std::fs::write(
            lib_dir.join(format!("{}.csv", component_type_name)),
            "id,mpn,value,description,footprint,symbol\n",
        )
        .map_err(|e| e.to_string())?;
    }

    manifest.part_tables.push(kicodex_core::data::library::PartTableDef {
        name: component_type_name.clone(),
        file: format!("{}.csv", component_type_name),
        template: component_type_name,
    });

    kicodex_core::data::library::save_library_manifest(&lib_dir, &manifest)
        .map_err(|e| e.to_string())?;

    reload_registry_for_path(&state, &lib_dir);

    Ok(())
}

#[tauri::command]
pub fn get_part_table_data(
    lib_path: String,
    component_type_name: String,
) -> Result<PartTableData, String> {
    let library_root = PathBuf::from(&lib_path);
    let library =
        kicodex_core::server::load_library(&library_root).map_err(|e| e.to_string())?;

    let ct = library
        .part_tables
        .iter()
        .find(|t| t.name == component_type_name || t.template_name == component_type_name)
        .ok_or_else(|| format!("Part table '{}' not found", component_type_name))?;

    let fields: Vec<FieldInfo> = ct
        .template
        .fields
        .iter()
        .map(|(key, def)| FieldInfo {
            key: key.clone(),
            display_name: def.display_name.clone(),
            required: def.required,
            visible: def.visible,
            description: def.description.clone(),
            field_type: def.field_type.clone(),
        })
        .collect();

    Ok(PartTableData {
        name: ct.name.clone(),
        template_name: ct.template_name.clone(),
        template: TemplateInfo {
            based_on: None,
            exclude_from_bom: ct.template.exclude_from_bom,
            exclude_from_board: ct.template.exclude_from_board,
            exclude_from_sim: ct.template.exclude_from_sim,
            fields,
        },
        components: ct.components.clone(),
    })
}

#[tauri::command]
pub fn add_component(
    state: State<'_, AppState>,
    lib_path: String,
    component_type_name: String,
    fields: indexmap::IndexMap<String, String>,
) -> Result<String, String> {
    let library_root = PathBuf::from(&lib_path);
    let manifest = kicodex_core::data::library::load_library_manifest(&library_root)
        .map_err(|e| e.to_string())?;

    let ct_def = manifest
        .part_tables
        .iter()
        .find(|t| t.name == component_type_name || t.template == component_type_name)
        .ok_or_else(|| format!("Part table '{}' not found", component_type_name))?;

    let csv_path = library_root.join(&ct_def.file);
    let id = kicodex_core::data::csv_loader::append_component(&csv_path, &fields)
        .map_err(|e| e.to_string())?;

    reload_registry_for_path(&state, &library_root);

    Ok(id)
}

#[tauri::command]
pub fn update_component(
    state: State<'_, AppState>,
    lib_path: String,
    component_type_name: String,
    id: String,
    fields: indexmap::IndexMap<String, String>,
) -> Result<(), String> {
    let library_root = PathBuf::from(&lib_path);
    let manifest = kicodex_core::data::library::load_library_manifest(&library_root)
        .map_err(|e| e.to_string())?;

    let ct_def = manifest
        .part_tables
        .iter()
        .find(|t| t.name == component_type_name || t.template == component_type_name)
        .ok_or_else(|| format!("Part table '{}' not found", component_type_name))?;

    let csv_path = library_root.join(&ct_def.file);
    kicodex_core::data::csv_loader::update_component(&csv_path, &id, &fields)
        .map_err(|e| e.to_string())?;

    reload_registry_for_path(&state, &library_root);

    Ok(())
}

#[tauri::command]
pub fn delete_component(
    state: State<'_, AppState>,
    lib_path: String,
    component_type_name: String,
    id: String,
) -> Result<(), String> {
    let library_root = PathBuf::from(&lib_path);
    let manifest = kicodex_core::data::library::load_library_manifest(&library_root)
        .map_err(|e| e.to_string())?;

    let ct_def = manifest
        .part_tables
        .iter()
        .find(|t| t.name == component_type_name || t.template == component_type_name)
        .ok_or_else(|| format!("Part table '{}' not found", component_type_name))?;

    let csv_path = library_root.join(&ct_def.file);
    kicodex_core::data::csv_loader::delete_component(&csv_path, &id).map_err(|e| e.to_string())?;

    reload_registry_for_path(&state, &library_root);

    Ok(())
}

#[tauri::command]
pub fn get_template(
    lib_path: String,
    template_name: String,
) -> Result<TemplateInfo, String> {
    let library_root = PathBuf::from(&lib_path);
    let manifest = kicodex_core::data::library::load_library_manifest(&library_root)
        .map_err(|e| e.to_string())?;
    let templates_dir = library_root.join(&manifest.templates_path);

    // Load raw template for based_on info
    let template_path = templates_dir.join(format!("{}.yaml", template_name));
    let raw: Option<kicodex_core::data::schema::RawTemplate> = if template_path.exists() {
        let content = std::fs::read_to_string(&template_path).map_err(|e| e.to_string())?;
        Some(serde_yml::from_str(&content).map_err(|e| e.to_string())?)
    } else {
        None
    };

    let resolved = kicodex_core::data::schema::load_template(&templates_dir, &template_name)
        .map_err(|e| e.to_string())?;

    let fields: Vec<FieldInfo> = resolved
        .fields
        .iter()
        .map(|(key, def)| FieldInfo {
            key: key.clone(),
            display_name: def.display_name.clone(),
            required: def.required,
            visible: def.visible,
            description: def.description.clone(),
            field_type: def.field_type.clone(),
        })
        .collect();

    Ok(TemplateInfo {
        based_on: raw.and_then(|r| r.based_on),
        exclude_from_bom: resolved.exclude_from_bom,
        exclude_from_board: resolved.exclude_from_board,
        exclude_from_sim: resolved.exclude_from_sim,
        fields,
    })
}

#[tauri::command]
pub fn save_template(
    state: State<'_, AppState>,
    lib_path: String,
    template_name: String,
    template: RawTemplateInput,
    renames: Option<Vec<RenameEntry>>,
    deletions: Option<Vec<String>>,
) -> Result<(), String> {
    let library_root = PathBuf::from(&lib_path);
    let manifest = kicodex_core::data::library::load_library_manifest(&library_root)
        .map_err(|e| e.to_string())?;
    let templates_dir = library_root.join(&manifest.templates_path);

    // Apply CSV migrations before writing the template
    let has_renames = renames.as_ref().is_some_and(|r| !r.is_empty());
    let has_deletions = deletions.as_ref().is_some_and(|d| !d.is_empty());

    if has_renames || has_deletions {
        // Find all part tables using this template
        let csv_paths: Vec<std::path::PathBuf> = manifest
            .part_tables
            .iter()
            .filter(|ct| ct.template == template_name)
            .map(|ct| library_root.join(&ct.file))
            .collect();

        for csv_path in &csv_paths {
            if let Some(ref renames) = renames {
                let rename_pairs: Vec<(String, String)> = renames
                    .iter()
                    .map(|r| (r.from.clone(), r.to.clone()))
                    .collect();
                kicodex_core::data::csv_loader::rename_csv_columns(csv_path, &rename_pairs)
                    .map_err(|e| format!("Failed to rename CSV columns in {}: {}", csv_path.display(), e))?;
            }
            if let Some(ref deletions) = deletions {
                kicodex_core::data::csv_loader::remove_csv_columns(csv_path, deletions)
                    .map_err(|e| format!("Failed to remove CSV columns in {}: {}", csv_path.display(), e))?;
            }
        }
    }

    let mut fields = indexmap::IndexMap::new();
    for f in &template.fields {
        fields.insert(
            f.key.clone(),
            kicodex_core::data::schema::FieldDef {
                display_name: f.display_name.clone(),
                required: f.required,
                visible: f.visible,
                description: f.description.clone(),
                field_type: f.field_type.clone(),
            },
        );
    }

    let raw = kicodex_core::data::schema::RawTemplate {
        based_on: template.based_on,
        exclude_from_bom: template.exclude_from_bom,
        exclude_from_board: template.exclude_from_board,
        exclude_from_sim: template.exclude_from_sim,
        fields,
    };

    kicodex_core::data::schema::write_template(&templates_dir, &template_name, &raw)
        .map_err(|e| e.to_string())?;

    reload_registry_for_path(&state, &library_root);

    Ok(())
}

#[tauri::command]
pub fn list_templates(lib_path: String, exclude: Option<String>) -> Result<Vec<String>, String> {
    let library_root = PathBuf::from(&lib_path);
    let manifest = kicodex_core::data::library::load_library_manifest(&library_root)
        .map_err(|e| e.to_string())?;
    let templates_dir = library_root.join(&manifest.templates_path);

    let pattern = format!("{}/*.yaml", templates_dir.display());
    let names: Vec<String> = glob::glob(&pattern)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .filter(|name| exclude.as_ref().map_or(true, |ex| name != ex))
        .collect();

    Ok(names)
}

#[tauri::command]
pub fn list_kicad_libraries(
    state: State<'_, AppState>,
    kind: String,
) -> Result<Vec<String>, String> {
    let kicad_libs = state.kicad_libs.lock().unwrap();
    let libs = kicad_libs
        .as_ref()
        .ok_or_else(|| "KiCad libraries not loaded".to_string())?;

    match kind.as_str() {
        "symbol" => Ok(libs.list_symbol_libraries()),
        "footprint" => Ok(libs.list_footprint_libraries()),
        _ => Err(format!("Unknown library kind: {}", kind)),
    }
}

#[tauri::command]
pub fn list_kicad_entries(
    state: State<'_, AppState>,
    kind: String,
    lib_name: String,
) -> Result<Vec<String>, String> {
    let kicad_libs = state.kicad_libs.lock().unwrap();
    let libs = kicad_libs
        .as_ref()
        .ok_or_else(|| "KiCad libraries not loaded".to_string())?;

    match kind.as_str() {
        "symbol" => libs
            .list_symbols(&lib_name)
            .ok_or_else(|| format!("Symbol library '{}' not found", lib_name)),
        "footprint" => libs
            .list_footprints(&lib_name)
            .ok_or_else(|| format!("Footprint library '{}' not found", lib_name)),
        _ => Err(format!("Unknown library kind: {}", kind)),
    }
}

#[tauri::command]
pub fn open_in_explorer(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_discovered_projects(
    state: State<'_, AppState>,
) -> Result<Vec<DiscoveredProject>, String> {
    let active = state.active_projects.lock().unwrap();
    let persisted = state.persisted.lock().unwrap();

    let registered_paths: HashSet<String> = persisted
        .projects
        .iter()
        .map(|p| p.project_path.clone())
        .collect();

    let mut discovered = Vec::new();
    for ap in active.iter() {
        let path_str = ap.project_path.to_string_lossy().to_string();
        if !registered_paths.contains(&path_str) {
            let name = ap
                .project_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            discovered.push(DiscoveredProject {
                project_path: path_str,
                name,
            });
        }
    }
    Ok(discovered)
}

#[tauri::command]
pub fn scan_project(
    state: State<'_, AppState>,
    project_path: String,
) -> Result<ScanProjectResult, String> {
    let project_dir = PathBuf::from(&project_path);
    let has_config = project_dir.join("kicodex.yaml").exists();

    let persisted = state.persisted.lock().unwrap();
    let already_registered = persisted
        .projects
        .iter()
        .any(|p| p.project_path == project_path);

    let libraries = scan_for_libraries(project_path)?;

    Ok(ScanProjectResult {
        has_config,
        already_registered,
        libraries,
    })
}

#[tauri::command]
pub fn add_project(
    state: State<'_, AppState>,
    project_path: String,
    libraries: Vec<ScanResult>,
) -> Result<AddProjectResult, String> {
    let project_dir = PathBuf::from(&project_path);

    // Write kicodex.yaml (reuse apply_scan logic)
    apply_scan(project_path.clone(), libraries.clone())?;

    // Register each library
    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| "Could not determine config directory".to_string())?;

    let mut persisted = state.persisted.lock().unwrap();
    let port = state.port;

    let mut count = 0;
    let mut httplib_paths = Vec::new();

    for lib in &libraries {
        if !lib.is_new {
            continue;
        }
        let library_path = project_dir.join(&lib.path);
        let library_path = library_path
            .canonicalize()
            .unwrap_or_else(|_| library_path.clone());

        let library =
            kicodex_core::server::load_library(&library_path).map_err(|e| e.to_string())?;

        let token = uuid::Uuid::new_v4().to_string();
        let description = library.description.clone();
        let fallback = format!("KiCodex HTTP Library for {}", lib.name);
        let desc_str = description.as_deref().unwrap_or(&fallback);

        persisted.upsert(kicodex_core::registry::ProjectEntry {
            token: token.clone(),
            project_path: project_dir.to_string_lossy().to_string(),
            library_path: library_path.to_string_lossy().to_string(),
            name: lib.name.clone(),
            description: description.clone(),
        });

        // Write .kicad_httplib in the library directory (co-located)
        let httplib_path = library_path.join(format!("{}.kicad_httplib", lib.name));
        let httplib_content = format!(
            r#"{{
    "meta": {{
        "version": 1.0
    }},
    "name": "{}",
    "description": "{}",
    "source": {{
        "type": "REST_API",
        "api_version": "v1",
        "root_url": "http://127.0.0.1:{port}",
        "token": "{token}"
    }}
}}"#,
            lib.name, desc_str
        );
        std::fs::write(&httplib_path, &httplib_content).map_err(|e| e.to_string())?;

        let clean_httplib = httplib_path
            .to_string_lossy()
            .strip_prefix(r"\\?\")
            .unwrap_or(&httplib_path.to_string_lossy())
            .to_string();
        httplib_paths.push(clean_httplib);

        // Insert into runtime registry
        state.registry.insert(&token, library);
        count += 1;
    }

    persisted.save(&registry_path).map_err(|e| e.to_string())?;

    Ok(AddProjectResult {
        registered_count: count,
        httplib_paths,
    })
}

#[tauri::command]
pub fn add_git_library(
    project_path: String,
    git_url: String,
    name: String,
    target_dir: String,
) -> Result<String, String> {
    let project_dir = PathBuf::from(&project_path);
    let target = PathBuf::from(&target_dir).join(&name);

    if target.exists() {
        return Err(format!("Directory already exists: {}", target.display()));
    }

    let is_git_repo = project_dir.join(".git").exists();

    // Compute path relative to project for git submodule
    let rel_target = pathdiff::diff_paths(&target, &project_dir)
        .unwrap_or_else(|| target.clone());

    let output = if is_git_repo {
        std::process::Command::new("git")
            .args(["submodule", "add", &git_url, &rel_target.to_string_lossy()])
            .current_dir(&project_dir)
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?
    } else {
        std::process::Command::new("git")
            .args(["clone", &git_url, &target.to_string_lossy()])
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Git command failed: {}", stderr));
    }

    Ok(target.to_string_lossy().to_string())
}

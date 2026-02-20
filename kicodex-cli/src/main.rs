use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "kicodex",
    about = "KiCad HTTP Library server backed by CSV files"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose (debug) logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP server for a library directory (single-project mode)
    /// or serve all registered projects (no path argument)
    Serve {
        /// Path to the library directory (containing library.yaml).
        /// If omitted, serves all registered projects from the persistent registry.
        path: Option<PathBuf>,

        /// Port to listen on
        #[arg(long, default_value_t = 18734)]
        port: u16,

        /// Host address to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },

    /// Initialize a KiCad project directory for use with KiCodex.
    /// Reads kicodex.yaml, generates auth tokens, registers libraries
    /// with the persistent registry, and writes .kicad_httplib files.
    Init {
        /// Path to the KiCad project directory (containing kicodex.yaml).
        /// Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Port the KiCodex server listens on (used in .kicad_httplib)
        #[arg(long, default_value_t = 18734)]
        port: u16,
    },

    /// Scaffold a new library or add a part table to an existing library
    New {
        /// Name for the library
        name: String,

        /// Parent directory where the library will be created
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Name for the part table (defaults to library name for new libraries)
        #[arg(long, alias = "table", alias = "component-type")]
        part_table: Option<String>,
    },

    /// Scan for libraries and generate/update kicodex.yaml
    Scan {
        /// Directory to scan (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Validate library data against templates
    Validate {
        /// Path to library directory (containing library.yaml)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// KiCad project directory (for project-local lib tables).
        /// Auto-detected from kicodex.yaml or .kicad_pro location if omitted.
        #[arg(long)]
        project: Option<PathBuf>,

        /// Output results as JSON (for CI)
        #[arg(long)]
        json: bool,
    },

    /// List all registered projects and libraries
    List,

    /// Remove a project from the registry and delete its .kicad_httplib files
    Remove {
        /// Path to the project directory to remove (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = if cli.verbose {
        "debug"
    } else if cli.quiet {
        "warn"
    } else {
        "info"
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    match cli.command {
        Commands::Serve { path, port, host } => match path {
            Some(path) => {
                let path = path.canonicalize().unwrap_or(path);
                run_serve(&path, port, &host).await?;
            }
            None => {
                let cwd = std::env::current_dir()?;
                if cwd.join("kicodex.yaml").exists() {
                    run_serve(&cwd, port, &host).await?;
                } else {
                    run_serve_all(port, &host).await?;
                }
            }
        },
        Commands::Init { path, port } => {
            let path = path.canonicalize().unwrap_or(path);
            run_init(&path, port)?;
        }
        Commands::New {
            name,
            path,
            part_table,
        } => {
            run_new(&name, &path, part_table.as_deref())?;
        }
        Commands::Scan { path } => {
            run_scan(&path)?;
        }
        Commands::Validate {
            path,
            project,
            json,
        } => {
            let path = path.canonicalize().unwrap_or(path);
            let code = run_validate(&path, project.as_deref(), json)?;
            if code != 0 {
                std::process::exit(code);
            }
        }
        Commands::List => {
            run_list()?;
        }
        Commands::Remove { path } => {
            run_remove(&path)?;
        }
    }

    Ok(())
}

/// Serve from a path: if it has kicodex.yaml, load all libraries from it;
/// if it has library.yaml, serve that single library.
async fn run_serve(path: &std::path::Path, port: u16, host: &str) -> anyhow::Result<()> {
    if path.join("kicodex.yaml").exists() {
        let config = kicodex_core::data::project::load_project_config(path)?;
        if config.libraries.is_empty() {
            anyhow::bail!("kicodex.yaml has no libraries listed");
        }

        let registry = kicodex_core::registry::ProjectRegistry::new();
        for lib_ref in &config.libraries {
            let lib_path = path.join(&lib_ref.path);
            let lib_path = lib_path.canonicalize().unwrap_or(lib_path);
            let library = kicodex_core::server::load_library(&lib_path)?;
            tracing::info!(
                "Loaded library '{}' with {} part table(s)",
                library.name,
                library.part_tables.len()
            );
            for ct in &library.part_tables {
                tracing::info!("  {} ({} parts)", ct.name, ct.components.len());
            }
            let token = uuid::Uuid::new_v4().to_string();
            registry.insert(&token, library);
        }

        let registry = Arc::new(registry);
        kicodex_core::server::run_server_with_registry(registry, port, host).await?;
    } else if path.join("library.yaml").exists() {
        kicodex_core::server::run_server(path, port, host).await?;
    } else {
        anyhow::bail!(
            "No library.yaml or kicodex.yaml found in {}. \
             Run `kicodex scan` to generate kicodex.yaml, or \
             `kicodex serve <library-dir>` to serve a single library.",
            path.display()
        );
    }

    Ok(())
}

/// Serve all registered projects from the persistent registry.
async fn run_serve_all(port: u16, host: &str) -> anyhow::Result<()> {
    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    let persisted = kicodex_core::registry::PersistedRegistry::load(&registry_path)?;

    if persisted.projects.is_empty() {
        anyhow::bail!(
            "No projects registered. Run `kicodex init` in a project directory first, \
             or use `kicodex serve <path>` for single-library mode."
        );
    }

    tracing::info!(
        "Loading {} registered project(s) from {}",
        persisted.projects.len(),
        registry_path.display()
    );

    let registry = kicodex_core::registry::ProjectRegistry::from_persisted(&persisted)?;
    let registry = Arc::new(registry);

    // Start file watcher for hot-reload
    if let Err(e) = kicodex_core::watcher::start_watching(&persisted, registry.clone()) {
        tracing::warn!("Failed to start file watcher: {}", e);
    }

    kicodex_core::server::run_server_with_registry(registry, port, host).await?;

    Ok(())
}

/// Initialize a KiCad project: read kicodex.yaml, register each library,
/// and write .kicad_httplib files.
fn run_init(project_dir: &std::path::Path, port: u16) -> anyhow::Result<()> {
    let config = kicodex_core::data::project::load_project_config(project_dir)?;

    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    let mut persisted = kicodex_core::registry::PersistedRegistry::load(&registry_path)?;

    for lib_ref in &config.libraries {
        let library_path = project_dir.join(&lib_ref.path);
        let library_path = library_path
            .canonicalize()
            .unwrap_or_else(|_| library_path.clone());

        // Validate that the library can be loaded
        let library = kicodex_core::server::load_library(&library_path)?;
        tracing::info!(
            "Validated library '{}' at {}",
            library.name,
            library_path.display()
        );

        let token = uuid::Uuid::new_v4().to_string();
        let description = library.description.clone();
        let fallback = format!("KiCodex HTTP Library for {}", lib_ref.name);
        let desc_str = description.as_deref().unwrap_or(&fallback);

        persisted.upsert(kicodex_core::registry::ProjectEntry {
            token: token.clone(),
            project_path: Some(project_dir.to_string_lossy().to_string()),
            library_path: library_path.to_string_lossy().to_string(),
            name: lib_ref.name.clone(),
            description: description.clone(),
        });

        // Write .kicad_httplib file in the project directory
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

        std::fs::write(&httplib_path, &httplib_content)?;
        tracing::info!("Wrote {}", httplib_path.display());
    }

    persisted.save(&registry_path)?;
    tracing::info!("Registry saved to {}", registry_path.display());

    println!(
        "Initialized {} library/libraries. Run `kicodex serve` to start the server.",
        config.libraries.len()
    );

    Ok(())
}

/// Scaffold a new library or add a part table to an existing library.
fn run_new(
    name: &str,
    parent_dir: &std::path::Path,
    part_table: Option<&str>,
) -> anyhow::Result<()> {
    let lib_dir = parent_dir.join(name);
    let manifest_path = lib_dir.join("library.yaml");

    if manifest_path.exists() {
        // Scenario B: library exists, add a new part table
        let ct_name = part_table.ok_or_else(|| {
            anyhow::anyhow!(
                "Library '{}' already exists. Use --part-table <name> to add a new part table.",
                name
            )
        })?;

        let mut manifest = kicodex_core::data::library::load_library_manifest(&lib_dir)?;

        // Check for duplicate part table
        if manifest.part_tables.iter().any(|t| t.template == ct_name) {
            anyhow::bail!(
                "Part table '{}' already exists in library '{}'",
                ct_name,
                name
            );
        }

        // Create template file
        let templates_dir = lib_dir.join(&manifest.templates_path);
        std::fs::create_dir_all(&templates_dir)?;
        let template_path = templates_dir.join(format!("{}.yaml", ct_name));
        std::fs::write(&template_path, schema_template())?;

        // Create CSV file
        let csv_path = lib_dir.join(format!("{}.csv", ct_name));
        std::fs::write(&csv_path, csv_template())?;

        // Append part table to manifest
        manifest
            .part_tables
            .push(kicodex_core::data::library::PartTableDef {
                name: capitalize(ct_name),
                file: format!("{}.csv", ct_name),
                template: ct_name.to_string(),
            });

        let yaml = serde_yml::to_string(&manifest)?;
        std::fs::write(&manifest_path, yaml)?;

        println!(
            "Added part table '{}' to library '{}':",
            ct_name, name
        );
        println!(
            "  - {}/{}/{}.yaml",
            name, manifest.templates_path, ct_name
        );
        println!("  - {}/{}.csv", name, ct_name);
        println!("  - Updated library.yaml");
    } else {
        // Scenario A: create new library
        std::fs::create_dir_all(&lib_dir)?;
        let templates_dir = lib_dir.join("templates");
        std::fs::create_dir_all(&templates_dir)?;

        // Build part tables list — only if --part-table was explicitly provided
        let mut part_tables = Vec::new();
        if let Some(ct_name) = part_table {
            // Write template
            let template_path = templates_dir.join(format!("{}.yaml", ct_name));
            std::fs::write(&template_path, schema_template())?;

            // Write CSV
            let csv_path = lib_dir.join(format!("{}.csv", ct_name));
            std::fs::write(&csv_path, csv_template())?;

            part_tables.push(kicodex_core::data::library::PartTableDef {
                name: capitalize(ct_name),
                file: format!("{}.csv", ct_name),
                template: ct_name.to_string(),
            });

            println!("Created library '{}' at {}/", name, lib_dir.display());
            println!("  - library.yaml");
            println!("  - templates/{}.yaml", ct_name);
            println!("  - {}.csv", ct_name);
        } else {
            println!("Created library '{}' at {}/", name, lib_dir.display());
            println!("  - library.yaml");
            println!("  - templates/");
            println!(
                "Add part tables with: kicodex new {} --part-table <name>",
                name,
            );
        }

        // Write library.yaml
        let manifest = kicodex_core::data::library::LibraryManifest {
            name: name.to_string(),
            description: Some(format!("KiCodex library: {}", name)),
            templates_path: "templates".to_string(),
            part_tables,
        };
        let yaml = serde_yml::to_string(&manifest)?;
        std::fs::write(&manifest_path, yaml)?;

        // Auto-register as standalone library in persisted registry
        register_standalone_library(&lib_dir, name)?;
    }

    Ok(())
}

/// Scan for library.yaml files and generate/update kicodex.yaml.
fn run_scan(scan_dir: &std::path::Path) -> anyhow::Result<()> {
    let pattern = format!("{}/**/library.yaml", scan_dir.display());
    let entries: Vec<_> = glob::glob(&pattern)?.filter_map(|e| e.ok()).collect();

    if entries.is_empty() {
        println!("No library.yaml files found under {}", scan_dir.display());
        return Ok(());
    }

    println!("Scanning for libraries...");

    // Load existing kicodex.yaml if present
    let config_path = scan_dir.join("kicodex.yaml");
    let mut existing_config = if config_path.exists() {
        kicodex_core::data::project::load_project_config(scan_dir)?
    } else {
        kicodex_core::data::project::ProjectConfig {
            libraries: Vec::new(),
        }
    };

    let existing_names: std::collections::HashSet<String> = existing_config
        .libraries
        .iter()
        .map(|l| l.name.clone())
        .collect();

    let mut new_count = 0u32;

    println!("Found {} libraries:", entries.len());

    for entry in &entries {
        let lib_dir = entry.parent().unwrap();

        // Validate by loading the manifest
        let manifest = match kicodex_core::data::library::load_library_manifest(lib_dir) {
            Ok(m) => m,
            Err(e) => {
                println!("  ! {} (invalid: {})", lib_dir.display(), e);
                continue;
            }
        };

        // Compute relative path from scan_dir to lib_dir
        let rel_path =
            pathdiff::diff_paths(lib_dir, scan_dir).unwrap_or_else(|| lib_dir.to_path_buf());
        let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");

        if existing_names.contains(&manifest.name) {
            println!(
                "  = {} ({})     [already in kicodex.yaml]",
                manifest.name, rel_path_str
            );
        } else {
            println!("  + {} ({})     [NEW]", manifest.name, rel_path_str);
            existing_config
                .libraries
                .push(kicodex_core::data::project::LibraryRef {
                    name: manifest.name.clone(),
                    path: rel_path_str,
                });
            new_count += 1;
        }
    }

    // Write kicodex.yaml
    let yaml = serde_yml::to_string(&existing_config)?;
    std::fs::write(&config_path, yaml)?;

    if new_count > 0 {
        println!(
            "Updated kicodex.yaml ({} new {} added)",
            new_count,
            if new_count == 1 {
                "library"
            } else {
                "libraries"
            }
        );
    } else {
        println!("kicodex.yaml is up to date (no new libraries found)");
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Error,
    Warn,
}

struct ValidationIssue {
    severity: Severity,
    part_table: String,
    file: String,
    row: Option<usize>,
    id: Option<String>,
    message: String,
}

/// Validate library data against templates and report issues.
/// Returns exit code: 0 if no errors, 1 if any errors.
///
/// If `path` contains a `kicodex.yaml`, validates all libraries referenced in it.
/// If `path` contains a `library.yaml`, validates that single library.
fn run_validate(
    path: &std::path::Path,
    project: Option<&std::path::Path>,
    json_output: bool,
) -> anyhow::Result<i32> {
    // Determine library paths to validate
    let library_roots: Vec<std::path::PathBuf> = if path.join("kicodex.yaml").exists() {
        let config = kicodex_core::data::project::load_project_config(path)?;
        if config.libraries.is_empty() {
            anyhow::bail!("kicodex.yaml has no libraries listed");
        }
        config
            .libraries
            .iter()
            .map(|lib_ref| {
                let lib_path = path.join(&lib_ref.path);
                lib_path.canonicalize().unwrap_or(lib_path)
            })
            .collect()
    } else if path.join("library.yaml").exists() {
        vec![path.to_path_buf()]
    } else {
        anyhow::bail!(
            "No library.yaml or kicodex.yaml found in {}",
            path.display()
        );
    };

    // Determine project directory for KiCad lib tables
    let project_dir = project
        .map(|p| p.to_path_buf())
        .or_else(|| find_project_dir(path));

    // Try to load KiCad libraries (warn and skip if unavailable)
    let kicad_libs =
        match kicodex_core::data::kicad_libs::KicadLibraries::load(project_dir.as_deref()) {
            Ok(libs) => Some(libs),
            Err(e) => {
                tracing::warn!("Could not load KiCad library tables: {}", e);
                None
            }
        };

    let mut total_exit_code = 0;
    for library_root in &library_roots {
        let code = validate_library(library_root, kicad_libs.as_ref(), json_output)?;
        if code != 0 {
            total_exit_code = code;
        }
    }
    Ok(total_exit_code)
}

/// Walk up from `path` to find a directory containing `kicodex.yaml` or `.kicad_pro`.
/// Limited to 10 levels to avoid scanning large directory trees.
fn find_project_dir(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut current = if path.is_file() {
        path.parent()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    for _ in 0..10 {
        if current.join("kicodex.yaml").exists() || current.join("sym-lib-table").exists() {
            return Some(current);
        }
        // Check for .kicad_pro only in directories likely to be project dirs
        if std::fs::read_dir(&current)
            .ok()
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.ends_with(".kicad_pro"))
                })
            })
            .unwrap_or(false)
        {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
    None
}

/// Validate a single library directory.
fn validate_library(
    library_root: &std::path::Path,
    kicad_libs: Option<&kicodex_core::data::kicad_libs::KicadLibraries>,
    json_output: bool,
) -> anyhow::Result<i32> {
    let library = kicodex_core::server::load_library(library_root)?;
    let manifest = kicodex_core::data::library::load_library_manifest(library_root)?;

    // Build a map from part table name -> csv file path
    let ct_files: std::collections::HashMap<String, String> = manifest
        .part_tables
        .iter()
        .map(|t| (t.name.clone(), t.file.clone()))
        .collect();

    let mut issues: Vec<ValidationIssue> = Vec::new();

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

        // Check 1: Required fields present as CSV columns
        for (field_name, field_def) in &ct.template.fields {
            if field_def.required && !csv_headers.contains(field_name) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    part_table: ct.name.clone(),
                    file: csv_file.clone(),
                    row: None,
                    id: None,
                    message: format!(
                        "required field '{}' is missing from CSV columns",
                        field_name
                    ),
                });
            }
        }

        // Check 2-6: Per-row checks
        let mut seen_ids: HashSet<String> = HashSet::new();

        for (row_idx, row) in ct.components.iter().enumerate() {
            let row_num = row_idx + 1; // 1-based
            let row_id = row.get("id").cloned().unwrap_or_default();

            // Check 3: Duplicate IDs
            if !row_id.is_empty() && !seen_ids.insert(row_id.clone()) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    part_table: ct.name.clone(),
                    file: csv_file.clone(),
                    row: Some(row_num),
                    id: Some(row_id.clone()),
                    message: format!("duplicate id '{}'", row_id),
                });
            }

            for (field_name, field_def) in &ct.template.fields {
                let value = row.get(field_name).map(|s| s.as_str()).unwrap_or("");
                let field_type = field_def.field_type.as_deref();

                // Check 2: Required fields non-empty
                if field_def.required && value.is_empty() {
                    if csv_headers.contains(field_name) {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            part_table: ct.name.clone(),
                            file: csv_file.clone(),
                            row: Some(row_num),
                            id: Some(row_id.clone()),
                            message: format!(
                                "required field '{}' is empty",
                                field_def.display_name
                            ),
                        });
                    }
                    continue;
                }

                if value.is_empty() {
                    // Warn on empty optional typed fields
                    if let Some(ft) = field_type {
                        issues.push(ValidationIssue {
                            severity: Severity::Warn,
                            part_table: ct.name.clone(),
                            file: csv_file.clone(),
                            row: Some(row_num),
                            id: Some(row_id.clone()),
                            message: format!(
                                "field '{}' is empty ({} field)",
                                field_def.display_name, ft
                            ),
                        });
                    }
                    continue;
                }

                // Check 4 & 5: kicad_symbol / kicad_footprint format
                if matches!(field_type, Some("kicad_symbol") | Some("kicad_footprint")) {
                    let colon_count = value.chars().filter(|&c| c == ':').count();
                    if colon_count != 1 {
                        let sev = if field_def.required {
                            Severity::Error
                        } else {
                            Severity::Warn
                        };
                        issues.push(ValidationIssue {
                            severity: sev,
                            part_table: ct.name.clone(),
                            file: csv_file.clone(),
                            row: Some(row_num),
                            id: Some(row_id.clone()),
                            message: format!(
                                "field '{}' has invalid {} format '{}' (expected 'Library:Name')",
                                field_def.display_name,
                                field_type.unwrap(),
                                value
                            ),
                        });
                    } else if let Some(klibs) = kicad_libs {
                        // Deep validation: check against KiCad library tables
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
                                issues.push(ValidationIssue {
                                    severity: Severity::Warn,
                                    part_table: ct.name.clone(),
                                    file: csv_file.clone(),
                                    row: Some(row_num),
                                    id: Some(row_id.clone()),
                                    message: format!(
                                        "{} library '{}' not found in lib tables",
                                        kind, lib
                                    ),
                                });
                            }
                            LibLookup::EntryNotFound(lib, entry) => {
                                issues.push(ValidationIssue {
                                    severity: Severity::Warn,
                                    part_table: ct.name.clone(),
                                    file: csv_file.clone(),
                                    row: Some(row_num),
                                    id: Some(row_id.clone()),
                                    message: format!(
                                        "{} '{}' not found in library '{}'",
                                        kind, entry, lib
                                    ),
                                });
                            }
                            LibLookup::LibraryUnreadable(_) => {
                                // Skip silently — library might be on a network path
                                tracing::debug!(
                                    "library for {} '{}' is unreadable, skipping check",
                                    kind,
                                    value
                                );
                            }
                        }
                    }
                }

                // Check 6: URL format
                if field_type == Some("url")
                    && !value.starts_with("http://")
                    && !value.starts_with("https://")
                {
                    let sev = if field_def.required {
                        Severity::Error
                    } else {
                        Severity::Warn
                    };
                    issues.push(ValidationIssue {
                        severity: sev,
                        part_table: ct.name.clone(),
                        file: csv_file.clone(),
                        row: Some(row_num),
                        id: Some(row_id.clone()),
                        message: format!(
                            "field '{}' has invalid URL '{}' (must start with http:// or https://)",
                            field_def.display_name, value
                        ),
                    });
                }
            }
        }
    }

    let error_count = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    let warning_count = issues
        .iter()
        .filter(|i| i.severity == Severity::Warn)
        .count();

    if json_output {
        let ct_json: Vec<serde_json::Value> = library
            .part_tables
            .iter()
            .map(|ct| {
                let csv_file = ct_files
                    .get(&ct.name)
                    .cloned()
                    .unwrap_or_else(|| ct.template_name.clone());
                let errors: Vec<_> = issues
                    .iter()
                    .filter(|i| i.part_table == ct.name && i.severity == Severity::Error)
                    .map(issue_to_json)
                    .collect();
                let warnings: Vec<_> = issues
                    .iter()
                    .filter(|i| i.part_table == ct.name && i.severity == Severity::Warn)
                    .map(issue_to_json)
                    .collect();
                json!({
                    "name": ct.name,
                    "file": csv_file,
                    "errors": errors,
                    "warnings": warnings,
                })
            })
            .collect();

        let output = json!({
            "library": library.name,
            "part_tables": ct_json,
            "error_count": error_count,
            "warning_count": warning_count,
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Validating library '{}'...\n", library.name);

        if issues.is_empty() {
            println!(
                "No issues found across {} part table(s).",
                library.part_tables.len()
            );
        } else {
            let mut current_part_table = String::new();
            for issue in &issues {
                if issue.part_table != current_part_table {
                    current_part_table = issue.part_table.clone();
                    println!(
                        "Part table '{}' ({}):",
                        current_part_table, issue.file
                    );
                }

                let severity_tag = match issue.severity {
                    Severity::Error => "[ERROR]",
                    Severity::Warn => "[WARN]",
                };

                match (&issue.row, &issue.id) {
                    (Some(row), Some(id)) if !id.is_empty() => {
                        println!(
                            "  {} Row {} (id={}): {}",
                            severity_tag, row, id, issue.message
                        );
                    }
                    (Some(row), _) => {
                        println!("  {} Row {}: {}", severity_tag, row, issue.message);
                    }
                    _ => {
                        println!("  {} {}", severity_tag, issue.message);
                    }
                }
            }

            println!(
                "\nSummary: {} error(s), {} warning(s) across {} part table(s)",
                error_count,
                warning_count,
                library.part_tables.len()
            );
        }
    }

    Ok(if error_count > 0 { 1 } else { 0 })
}

fn issue_to_json(issue: &ValidationIssue) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(row) = issue.row {
        obj.insert("row".into(), json!(row));
    }
    if let Some(ref id) = issue.id {
        obj.insert("id".into(), json!(id));
    }
    obj.insert("message".into(), json!(issue.message));
    serde_json::Value::Object(obj)
}

/// List all registered projects and their libraries.
fn run_list() -> anyhow::Result<()> {
    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    let persisted = kicodex_core::registry::PersistedRegistry::load(&registry_path)?;

    if persisted.projects.is_empty() {
        println!("No projects registered.");
        println!("Run `kicodex init` in a project directory to register it.");
        return Ok(());
    }

    // Separate standalone libraries from project-attached entries
    let mut standalone: Vec<&kicodex_core::registry::ProjectEntry> = Vec::new();
    let mut by_project: std::collections::BTreeMap<&str, Vec<&kicodex_core::registry::ProjectEntry>> =
        std::collections::BTreeMap::new();
    for entry in &persisted.projects {
        match &entry.project_path {
            Some(pp) => by_project.entry(pp.as_str()).or_default().push(entry),
            None => standalone.push(entry),
        }
    }

    if !standalone.is_empty() {
        println!("Standalone Libraries:\n");
        for entry in &standalone {
            let short_token = if entry.token.len() > 8 {
                format!("{}...", &entry.token[..8])
            } else {
                entry.token.clone()
            };
            let ct_count = match kicodex_core::server::load_library(std::path::Path::new(
                &entry.library_path,
            )) {
                Ok(lib) => format!("{} part table(s)", lib.part_tables.len()),
                Err(_) => "unable to load".to_string(),
            };
            println!(
                "  {} [{}] (token: {})",
                entry.name, ct_count, short_token
            );
            println!("    {}", entry.library_path);
        }
        println!();
    }

    if !by_project.is_empty() {
        println!("{} project(s) registered:\n", by_project.len());

        for (project_path, entries) in &by_project {
            println!("  {}", project_path);
            for entry in entries {
                let short_token = if entry.token.len() > 8 {
                    format!("{}...", &entry.token[..8])
                } else {
                    entry.token.clone()
                };
                let ct_count = match kicodex_core::server::load_library(std::path::Path::new(
                    &entry.library_path,
                )) {
                    Ok(lib) => format!("{} part table(s)", lib.part_tables.len()),
                    Err(_) => "unable to load".to_string(),
                };
                println!(
                    "    {} [{}] (token: {})",
                    entry.name, ct_count, short_token
                );
            }
            println!();
        }
    }

    Ok(())
}

/// Remove a project or standalone library from the registry.
/// If the path contains library.yaml (not kicodex.yaml), treats it as standalone library removal.
fn run_remove(path: &std::path::Path) -> anyhow::Result<()> {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let path_str = path.to_string_lossy().to_string();

    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    let mut persisted = kicodex_core::registry::PersistedRegistry::load(&registry_path)?;

    // Detect if this is a standalone library (has library.yaml but no kicodex.yaml)
    let is_library = path.join("library.yaml").exists() && !path.join("kicodex.yaml").exists();

    if is_library {
        // Try standalone removal first
        let standalone_match: Vec<_> = persisted
            .projects
            .iter()
            .filter(|p| p.project_path.is_none() && p.library_path == path_str)
            .cloned()
            .collect();

        if standalone_match.is_empty() {
            println!("No standalone library registered at {}", path.display());
            return Ok(());
        }

        persisted.remove_by_library_path(&path_str);
        persisted.save(&registry_path)?;

        println!(
            "Removed standalone library at {}",
            path.display()
        );
    } else {
        // Project removal
        let matching: Vec<_> = persisted
            .projects
            .iter()
            .filter(|p| p.project_path.as_deref() == Some(path_str.as_str()))
            .cloned()
            .collect();

        if matching.is_empty() {
            println!("No registered project found at {}", path.display());
            return Ok(());
        }

        // Delete .kicad_httplib files
        for entry in &matching {
            let httplib_path = path.join(format!("{}.kicad_httplib", entry.name));
            if httplib_path.exists() {
                std::fs::remove_file(&httplib_path)?;
                println!("Deleted {}", httplib_path.display());
            }
        }

        // Remove from registry
        persisted.remove_by_path(&path_str);
        persisted.save(&registry_path)?;

        println!(
            "Removed {} library/libraries for project at {}",
            matching.len(),
            path.display()
        );
    }

    Ok(())
}

/// Register a library as standalone in the persisted registry.
fn register_standalone_library(lib_dir: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    let mut persisted = kicodex_core::registry::PersistedRegistry::load(&registry_path)?;
    let lib_path = lib_dir
        .canonicalize()
        .unwrap_or_else(|_| lib_dir.to_path_buf());
    let lib_path_str = lib_path.to_string_lossy().to_string();

    // Check if already registered
    if persisted
        .projects
        .iter()
        .any(|p| p.project_path.is_none() && p.library_path == lib_path_str)
    {
        return Ok(());
    }

    let token = uuid::Uuid::new_v4().to_string();
    persisted.upsert(kicodex_core::registry::ProjectEntry {
        token,
        project_path: None,
        library_path: lib_path_str,
        name: name.to_string(),
        description: Some(format!("KiCodex library: {}", name)),
    });

    persisted.save(&registry_path)?;
    println!("Registered as standalone library");
    Ok(())
}

fn schema_template() -> &'static str {
    "fields:\n  value:\n    display_name: Value\n    visible: true\n  description:\n    display_name: Description\n    visible: true\n  footprint:\n    display_name: Footprint\n    visible: false\n    type: kicad_footprint\n  symbol:\n    display_name: Symbol\n    visible: false\n    type: kicad_symbol\n  datasheet:\n    display_name: Datasheet\n    required: false\n    type: url\n"
}

fn csv_template() -> &'static str {
    "id,mpn,value,description,footprint,symbol,datasheet\n"
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

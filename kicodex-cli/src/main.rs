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

    /// Scaffold a new library or add a component type to an existing library
    New {
        /// Name for the library
        name: String,

        /// Parent directory where the library will be created
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Name for the component type (defaults to library name for new libraries)
        #[arg(long, alias = "table")]
        component_type: Option<String>,
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
            component_type,
        } => {
            run_new(&name, &path, component_type.as_deref())?;
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
                "Loaded library '{}' with {} component type(s)",
                library.name,
                library.component_types.len()
            );
            for ct in &library.component_types {
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
            project_path: project_dir.to_string_lossy().to_string(),
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

/// Scaffold a new library or add a component type to an existing library.
fn run_new(
    name: &str,
    parent_dir: &std::path::Path,
    component_type: Option<&str>,
) -> anyhow::Result<()> {
    let lib_dir = parent_dir.join(name);
    let manifest_path = lib_dir.join("library.yaml");

    if manifest_path.exists() {
        // Scenario B: library exists, add a new component type
        let ct_name = component_type.ok_or_else(|| {
            anyhow::anyhow!(
                "Library '{}' already exists. Use --component-type <name> to add a new component type.",
                name
            )
        })?;

        let mut manifest = kicodex_core::data::library::load_library_manifest(&lib_dir)?;

        // Check for duplicate component type
        if manifest.component_types.iter().any(|t| t.template == ct_name) {
            anyhow::bail!(
                "Component type '{}' already exists in library '{}'",
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

        // Append component type to manifest
        manifest
            .component_types
            .push(kicodex_core::data::library::ComponentTypeDef {
                name: capitalize(ct_name),
                file: format!("{}.csv", ct_name),
                template: ct_name.to_string(),
            });

        let yaml = serde_yml::to_string(&manifest)?;
        std::fs::write(&manifest_path, yaml)?;

        println!(
            "Added component type '{}' to library '{}':",
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

        // Build component types list — only if --component-type was explicitly provided
        let mut component_types = Vec::new();
        if let Some(ct_name) = component_type {
            // Write template
            let template_path = templates_dir.join(format!("{}.yaml", ct_name));
            std::fs::write(&template_path, schema_template())?;

            // Write CSV
            let csv_path = lib_dir.join(format!("{}.csv", ct_name));
            std::fs::write(&csv_path, csv_template())?;

            component_types.push(kicodex_core::data::library::ComponentTypeDef {
                name: capitalize(ct_name),
                file: format!("{}.csv", ct_name),
                template: ct_name.to_string(),
            });

            println!("Created library '{}' at {}/", name, lib_dir.display());
            println!("  - library.yaml");
            println!("  - templates/{}.yaml", ct_name);
            println!("  - {}.csv", ct_name);
            println!(
                "Add your parts to {}.csv, then run `kicodex scan` to generate kicodex.yaml",
                ct_name
            );
        } else {
            println!("Created library '{}' at {}/", name, lib_dir.display());
            println!("  - library.yaml");
            println!("  - templates/");
            println!(
                "Add component types with: kicodex new {} --component-type <name>",
                name,
            );
        }

        // Write library.yaml
        let manifest = kicodex_core::data::library::LibraryManifest {
            name: name.to_string(),
            description: Some(format!("KiCodex library: {}", name)),
            templates_path: "templates".to_string(),
            component_types,
        };
        let yaml = serde_yml::to_string(&manifest)?;
        std::fs::write(&manifest_path, yaml)?;
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
    component_type: String,
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

    // Build a map from component type name -> csv file path
    let ct_files: std::collections::HashMap<String, String> = manifest
        .component_types
        .iter()
        .map(|t| (t.name.clone(), t.file.clone()))
        .collect();

    let mut issues: Vec<ValidationIssue> = Vec::new();

    for ct in &library.component_types {
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
                    component_type: ct.name.clone(),
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
                    component_type: ct.name.clone(),
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
                            component_type: ct.name.clone(),
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
                            component_type: ct.name.clone(),
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
                            component_type: ct.name.clone(),
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
                                    component_type: ct.name.clone(),
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
                                    component_type: ct.name.clone(),
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
                        component_type: ct.name.clone(),
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
            .component_types
            .iter()
            .map(|ct| {
                let csv_file = ct_files
                    .get(&ct.name)
                    .cloned()
                    .unwrap_or_else(|| ct.template_name.clone());
                let errors: Vec<_> = issues
                    .iter()
                    .filter(|i| i.component_type == ct.name && i.severity == Severity::Error)
                    .map(issue_to_json)
                    .collect();
                let warnings: Vec<_> = issues
                    .iter()
                    .filter(|i| i.component_type == ct.name && i.severity == Severity::Warn)
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
            "component_types": ct_json,
            "error_count": error_count,
            "warning_count": warning_count,
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Validating library '{}'...\n", library.name);

        if issues.is_empty() {
            println!(
                "No issues found across {} component type(s).",
                library.component_types.len()
            );
        } else {
            let mut current_component_type = String::new();
            for issue in &issues {
                if issue.component_type != current_component_type {
                    current_component_type = issue.component_type.clone();
                    println!(
                        "Component type '{}' ({}):",
                        current_component_type, issue.file
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
                "\nSummary: {} error(s), {} warning(s) across {} component type(s)",
                error_count,
                warning_count,
                library.component_types.len()
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

    // Group entries by project_path
    let mut by_project: std::collections::BTreeMap<&str, Vec<&kicodex_core::registry::ProjectEntry>> =
        std::collections::BTreeMap::new();
    for entry in &persisted.projects {
        by_project
            .entry(&entry.project_path)
            .or_default()
            .push(entry);
    }

    println!("{} project(s) registered:\n", by_project.len());

    for (project_path, entries) in &by_project {
        println!("  {}", project_path);
        for entry in entries {
            let short_token = if entry.token.len() > 8 {
                format!("{}...", &entry.token[..8])
            } else {
                entry.token.clone()
            };
            // Count component types by loading the library
            let ct_count = match kicodex_core::server::load_library(std::path::Path::new(
                &entry.library_path,
            )) {
                Ok(lib) => format!("{} component type(s)", lib.component_types.len()),
                Err(_) => "unable to load".to_string(),
            };
            println!(
                "    {} [{}] (token: {})",
                entry.name, ct_count, short_token
            );
        }
        println!();
    }

    Ok(())
}

/// Remove a project from the registry and delete its .kicad_httplib files.
fn run_remove(path: &std::path::Path) -> anyhow::Result<()> {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let path_str = path.to_string_lossy().to_string();

    let registry_path = kicodex_core::registry::PersistedRegistry::default_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    let mut persisted = kicodex_core::registry::PersistedRegistry::load(&registry_path)?;

    // Find matching entries before removing
    let matching: Vec<_> = persisted
        .projects
        .iter()
        .filter(|p| p.project_path == path_str)
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

    Ok(())
}

fn schema_template() -> &'static str {
    "fields:\n  value:\n    display_name: Value\n    visible: true\n  description:\n    display_name: Description\n    visible: true\n  footprint:\n    display_name: Footprint\n    visible: true\n    type: kicad_footprint\n  symbol:\n    display_name: Symbol\n    visible: true\n    type: kicad_symbol\n"
}

fn csv_template() -> &'static str {
    "id,mpn,value,description,footprint,symbol\n"
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

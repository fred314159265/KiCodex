use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "kicodex",
    about = "KiCad HTTP Library server backed by CSV files"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

    /// Scaffold a new library or add a table to an existing library
    New {
        /// Name for the library
        name: String,

        /// Parent directory for the library (default: libs)
        #[arg(long, default_value = "libs")]
        path: PathBuf,

        /// Name for the table (defaults to library name for new libraries)
        #[arg(long)]
        table: Option<String>,
    },

    /// Scan for libraries and generate/update kicodex.yaml
    Scan {
        /// Directory to scan (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { path, port } => match path {
            Some(path) => {
                let path = path.canonicalize().unwrap_or(path);
                kicodex_core::server::run_server(&path, port).await?;
            }
            None => {
                run_serve_all(port).await?;
            }
        },
        Commands::Init { path, port } => {
            let path = path.canonicalize().unwrap_or(path);
            run_init(&path, port)?;
        }
        Commands::New { name, path, table } => {
            run_new(&name, &path, table.as_deref())?;
        }
        Commands::Scan { path } => {
            run_scan(&path)?;
        }
    }

    Ok(())
}

/// Serve all registered projects from the persistent registry.
async fn run_serve_all(port: u16) -> anyhow::Result<()> {
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

    kicodex_core::server::run_server_with_registry(registry, port).await?;

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

/// Scaffold a new library or add a table to an existing library.
fn run_new(name: &str, parent_dir: &std::path::Path, table: Option<&str>) -> anyhow::Result<()> {
    let lib_dir = parent_dir.join(name);
    let manifest_path = lib_dir.join("library.yaml");

    if manifest_path.exists() {
        // Scenario B: library exists, add a new table
        let table_name = table.ok_or_else(|| {
            anyhow::anyhow!(
                "Library '{}' already exists. Use --table <name> to add a new table.",
                name
            )
        })?;

        let mut manifest =
            kicodex_core::data::library::load_library_manifest(&lib_dir)?;

        // Check for duplicate table (compare against schema key, which is the lowercase table identifier)
        if manifest.tables.iter().any(|t| t.schema == table_name) {
            anyhow::bail!(
                "Table '{}' already exists in library '{}'",
                table_name,
                name
            );
        }

        // Create schema file
        let schemas_dir = lib_dir.join(&manifest.schemas_path);
        std::fs::create_dir_all(&schemas_dir)?;
        let schema_path = schemas_dir.join(format!("{}.yaml", table_name));
        std::fs::write(&schema_path, schema_template())?;

        // Create CSV file
        let csv_path = lib_dir.join(format!("{}.csv", table_name));
        std::fs::write(&csv_path, csv_template())?;

        // Append table to manifest
        manifest.tables.push(kicodex_core::data::library::TableDef {
            name: capitalize(table_name),
            file: format!("{}.csv", table_name),
            schema: table_name.to_string(),
        });

        let yaml = serde_yml::to_string(&manifest)?;
        std::fs::write(&manifest_path, yaml)?;

        println!("Added table '{}' to library '{}':", table_name, name);
        println!("  - {}/schemas/{}.yaml", name, table_name);
        println!("  - {}/{}.csv", name, table_name);
        println!("  - Updated library.yaml");
    } else {
        // Scenario A: create new library
        let table_name = table.unwrap_or(name);

        std::fs::create_dir_all(&lib_dir)?;
        let schemas_dir = lib_dir.join("schemas");
        std::fs::create_dir_all(&schemas_dir)?;

        // Write schema
        let schema_path = schemas_dir.join(format!("{}.yaml", table_name));
        std::fs::write(&schema_path, schema_template())?;

        // Write CSV
        let csv_path = lib_dir.join(format!("{}.csv", table_name));
        std::fs::write(&csv_path, csv_template())?;

        // Write library.yaml
        let manifest = kicodex_core::data::library::LibraryManifest {
            name: name.to_string(),
            description: Some(format!("KiCodex library: {}", name)),
            schemas_path: "schemas".to_string(),
            tables: vec![kicodex_core::data::library::TableDef {
                name: capitalize(table_name),
                file: format!("{}.csv", table_name),
                schema: table_name.to_string(),
            }],
        };
        let yaml = serde_yml::to_string(&manifest)?;
        std::fs::write(&manifest_path, yaml)?;

        println!(
            "Created library '{}' at {}/",
            name,
            lib_dir.display()
        );
        println!("  - library.yaml");
        println!("  - schemas/{}.yaml", table_name);
        println!("  - {}.csv", table_name);
        println!(
            "Add your parts to {}.csv, then run `kicodex scan` to generate kicodex.yaml",
            table_name
        );
    }

    Ok(())
}

/// Scan for library.yaml files and generate/update kicodex.yaml.
fn run_scan(scan_dir: &std::path::Path) -> anyhow::Result<()> {
    let pattern = format!("{}/**/library.yaml", scan_dir.display());
    let entries: Vec<_> = glob::glob(&pattern)?
        .filter_map(|e| e.ok())
        .collect();

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
        let rel_path = pathdiff::diff_paths(lib_dir, scan_dir)
            .unwrap_or_else(|| lib_dir.to_path_buf());
        let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");

        if existing_names.contains(&manifest.name) {
            println!("  = {} ({})     [already in kicodex.yaml]", manifest.name, rel_path_str);
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
            if new_count == 1 { "library" } else { "libraries" }
        );
    } else {
        println!("kicodex.yaml is up to date (no new libraries found)");
    }

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

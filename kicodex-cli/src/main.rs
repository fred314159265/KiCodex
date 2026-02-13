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

        persisted.upsert(kicodex_core::registry::ProjectEntry {
            token: token.clone(),
            project_path: project_dir.to_string_lossy().to_string(),
            library_path: library_path.to_string_lossy().to_string(),
            name: lib_ref.name.clone(),
        });

        // Write .kicad_httplib file in the project directory
        let httplib_path = project_dir.join(format!("{}.kicad_httplib", lib_ref.name));
        let httplib_content = format!(
            r#"{{
    "meta": {{
        "version": 1.0
    }},
    "name": "{}",
    "description": "KiCodex HTTP Library for {}",
    "source": {{
        "type": "REST_API",
        "api_version": "v1",
        "root_url": "http://127.0.0.1:{port}/v1",
        "token": "{token}"
    }}
}}"#,
            lib_ref.name, lib_ref.name
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

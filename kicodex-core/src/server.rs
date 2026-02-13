use std::path::Path;
use std::sync::Arc;

use axum::Router;
use thiserror::Error;
use tower_http::trace::TraceLayer;

use crate::data::csv_loader::{self, CsvRow};
use crate::data::library::{self, LibraryManifest};
use crate::data::schema;
use crate::middleware;
use crate::registry::ProjectRegistry;
use crate::routes;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("library error: {0}")]
    Library(#[from] library::LibraryError),
    #[error("schema error: {0}")]
    Schema(#[from] schema::SchemaError),
    #[error("CSV error: {0}")]
    Csv(#[from] csv_loader::CsvError),
    #[error("server error: {0}")]
    Io(#[from] std::io::Error),
}

/// A loaded table with its data and metadata.
#[derive(Debug, Clone)]
pub struct LoadedTable {
    pub name: String,
    pub schema_name: String,
    pub rows: Vec<CsvRow>,
    pub schema: schema::ResolvedSchema,
}

/// The full loaded library state.
#[derive(Debug, Clone)]
pub struct LoadedLibrary {
    pub name: String,
    pub tables: Vec<LoadedTable>,
}

/// Load a library from disk into memory.
pub fn load_library(library_root: &Path) -> Result<LoadedLibrary, ServerError> {
    let manifest: LibraryManifest = library::load_library_manifest(library_root)?;
    let schemas_dir = library_root.join(&manifest.schemas_path);

    let mut tables = Vec::new();
    for table_def in &manifest.tables {
        let resolved_schema = schema::load_schema(&schemas_dir, &table_def.schema)?;
        let csv_path = library_root.join(&table_def.file);
        let rows = csv_loader::load_csv_with_ids(&csv_path)?;

        tables.push(LoadedTable {
            name: table_def.name.clone(),
            schema_name: table_def.schema.clone(),
            rows,
            schema: resolved_schema,
        });
    }

    Ok(LoadedLibrary {
        name: manifest.name,
        tables,
    })
}

/// Build the Axum router with auth middleware and all routes.
pub fn build_router(registry: Arc<ProjectRegistry>) -> Router {
    Router::new()
        .route("/v1/", axum::routing::get(routes::root::get_root))
        .route(
            "/v1/categories.json",
            axum::routing::get(routes::categories::get_categories),
        )
        .route(
            "/v1/parts/category/{categoryId}",
            axum::routing::get(routes::parts::get_parts_by_category),
        )
        .route(
            "/v1/parts/{partId}",
            axum::routing::get(routes::parts::get_part_detail),
        )
        .layer(axum::middleware::from_fn_with_state(
            registry.clone(),
            middleware::auth_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(registry)
}

/// Start the server in single-library mode (Phase 1 compatible).
pub async fn run_server(library_root: &Path, port: u16) -> Result<(), ServerError> {
    let library = load_library(library_root)?;
    tracing::info!(
        "Loaded library '{}' with {} tables",
        library.name,
        library.tables.len()
    );
    for table in &library.tables {
        tracing::info!("  {} ({} parts)", table.name, table.rows.len());
    }

    let registry = ProjectRegistry::new();
    let token = uuid::Uuid::new_v4().to_string();
    registry.insert(&token, library);

    let app = build_router(Arc::new(registry));
    let addr = format!("127.0.0.1:{port}");
    tracing::info!("Starting KiCodex server on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Start the server with a pre-built registry (multi-project mode).
pub async fn run_server_with_registry(
    registry: Arc<ProjectRegistry>,
    port: u16,
) -> Result<(), ServerError> {
    let app = build_router(registry);
    let addr = format!("127.0.0.1:{port}");
    tracing::info!("Starting KiCodex server on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

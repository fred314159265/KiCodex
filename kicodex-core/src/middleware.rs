use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use crate::registry::ProjectRegistry;
use crate::server::LoadedLibrary;

/// Extension inserted into the request by the auth middleware.
/// Routes extract this to get the library for the authenticated project.
#[derive(Clone)]
pub struct AuthenticatedLibrary(pub Arc<LoadedLibrary>);

/// Middleware that extracts the `Authorization: Token <value>` header,
/// looks up the project in the registry, and inserts an `AuthenticatedLibrary`
/// extension. Returns 401 if the token is missing or unknown.
///
/// When the registry contains exactly one project, the token check is skipped
/// (single-library mode for Phase 1 backwards compatibility).
pub async fn auth_middleware(
    State(registry): State<Arc<ProjectRegistry>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let tokens = registry.tokens();

    // Single-project mode: skip auth, use the only registered library
    if tokens.len() == 1 {
        if let Some(library) = registry.get(&tokens[0]) {
            req.extensions_mut().insert(AuthenticatedLibrary(library));
            return Ok(next.run(req).await);
        }
    }

    // Multi-project mode: require Authorization header
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Token ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let library = registry.get(token).ok_or(StatusCode::UNAUTHORIZED)?;
    req.extensions_mut().insert(AuthenticatedLibrary(library));

    Ok(next.run(req).await)
}

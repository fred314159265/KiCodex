use axum::Extension;
use axum::Json;

use crate::middleware::AuthenticatedLibrary;
use crate::models::Category;

pub async fn get_categories(
    Extension(AuthenticatedLibrary(library)): Extension<AuthenticatedLibrary>,
) -> Json<Vec<Category>> {
    let categories: Vec<Category> = library
        .component_types
        .iter()
        .enumerate()
        .map(|(i, ct)| Category {
            id: (i + 1).to_string(),
            name: ct.name.clone(),
            description: String::new(),
        })
        .collect();
    Json(categories)
}

use axum::Extension;
use axum::Json;

use crate::middleware::AuthenticatedLibrary;
use crate::models::Category;

pub async fn get_categories(
    Extension(AuthenticatedLibrary(library)): Extension<AuthenticatedLibrary>,
) -> Json<Vec<Category>> {
    let categories: Vec<Category> = library
        .tables
        .iter()
        .enumerate()
        .map(|(i, table)| Category {
            id: (i + 1).to_string(),
            name: table.name.clone(),
            description: String::new(),
        })
        .collect();
    Json(categories)
}

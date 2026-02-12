use axum::extract::State;
use axum::Json;

use crate::models::Category;
use crate::server::AppState;

pub async fn get_categories(State(state): State<AppState>) -> Json<Vec<Category>> {
    let categories: Vec<Category> = state
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

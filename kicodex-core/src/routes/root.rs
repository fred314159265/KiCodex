use axum::Json;

use crate::models::RootResponse;

pub async fn get_root() -> Json<RootResponse> {
    Json(RootResponse {
        categories: "categories.json".to_string(),
        parts: "parts/".to_string(),
    })
}

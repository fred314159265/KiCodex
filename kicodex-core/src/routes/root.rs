use axum::Json;

use crate::models::RootResponse;

pub async fn get_root() -> Json<RootResponse> {
    Json(RootResponse {
        categories: String::new(),
        parts: String::new(),
    })
}

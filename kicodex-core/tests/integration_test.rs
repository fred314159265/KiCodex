use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample-library")
}

#[tokio::test]
async fn test_root_endpoint() {
    let library = kicodex_core::server::load_library(&fixture_path()).unwrap();
    let app = kicodex_core::server::build_router(library);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let resp: serde_json::Value = client
        .get(format!("{url}/v1/"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp["categories"], "");
    assert_eq!(resp["parts"], "");
}

#[tokio::test]
async fn test_categories_endpoint() {
    let library = kicodex_core::server::load_library(&fixture_path()).unwrap();
    let app = kicodex_core::server::build_router(library);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let resp: Vec<serde_json::Value> = client
        .get(format!("{url}/v1/categories.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp.len(), 1);
    assert_eq!(resp[0]["id"], "1");
    assert_eq!(resp[0]["name"], "Resistors");
}

#[tokio::test]
async fn test_parts_by_category_endpoint() {
    let library = kicodex_core::server::load_library(&fixture_path()).unwrap();
    let app = kicodex_core::server::build_router(library);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let resp: Vec<serde_json::Value> = client
        .get(format!("{url}/v1/parts/category/1.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp.len(), 3);
    assert_eq!(resp[0]["id"], "1");
    assert_eq!(resp[0]["name"], "RC0603FR-0710KL");
    assert!(resp[0]["description"].as_str().unwrap().contains("10K"));
}

#[tokio::test]
async fn test_part_detail_endpoint() {
    let library = kicodex_core::server::load_library(&fixture_path()).unwrap();
    let app = kicodex_core::server::build_router(library);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let resp: serde_json::Value = client
        .get(format!("{url}/v1/parts/1.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp["id"], "1");
    assert_eq!(resp["name"], "RC0603FR-0710KL");
    assert_eq!(resp["symbolIdStr"], "Device:R");
    assert_eq!(resp["exclude_from_bom"], "False");
    assert_eq!(resp["exclude_from_board"], "False");
    assert_eq!(resp["exclude_from_sim"], "False");

    // Check fields — keys are schema display_name values
    let fields = &resp["fields"];
    assert_eq!(
        fields["Footprint"]["value"],
        "Resistor_SMD:R_0603_1608Metric"
    );
    assert_eq!(fields["Footprint"]["visible"], "False");
    assert_eq!(fields["Value"]["value"], "10K");
    assert!(fields["Value"]["visible"].is_null()); // visible by default
    assert_eq!(fields["reference"]["value"], "R");
    assert!(fields["reference"]["visible"].is_null()); // visible by default
    assert_eq!(fields["Description"]["value"], "RES 10K OHM 1% 1/10W 0603");
    assert_eq!(fields["Description"]["visible"], "False");
    assert_eq!(fields["Manufacturer"]["value"], "Yageo");
    assert_eq!(fields["Manufacturer"]["visible"], "False");
    assert_eq!(fields["MPN"]["value"], "RC0603FR-0710KL");
    assert_eq!(fields["MPN"]["visible"], "False");
    assert_eq!(fields["Datasheet"]["visible"], "False");
    // Resistor-specific fields are also hidden by default
    assert_eq!(fields["Resistance"]["visible"], "False");
    assert_eq!(fields["Tolerance"]["visible"], "False");
    assert_eq!(fields["Power Rating"]["visible"], "False");
    assert_eq!(fields["Package"]["visible"], "False");
}

#[tokio::test]
async fn test_nonexistent_category_returns_404() {
    let library = kicodex_core::server::load_library(&fixture_path()).unwrap();
    let app = kicodex_core::server::build_router(library);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{url}/v1/parts/category/99.json"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_nonexistent_part_returns_404() {
    let library = kicodex_core::server::load_library(&fixture_path()).unwrap();
    let app = kicodex_core::server::build_router(library);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{url}/v1/parts/999.json"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

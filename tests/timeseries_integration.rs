//! Integration tests for the Timeseries endpoints

use reqwest::Client;

#[tokio::test]
async fn test_timeseries_write() {
    let client = Client::new();
    let payload = serde_json::json!({
        "key": "ts-key",
        "points": [
            {"timestamp": 1700000000, "value": 42.0},
            {"timestamp": 1700000060, "value": 43.0}
        ]
    });
    let resp = client
        .post("http://localhost:8000/ts/write")
        .body(serde_json::to_string(&payload).unwrap())
        .header("Content-Type", "application/json")
        .send()
        .await
        .expect("Failed to write timeseries data");
    assert!(resp.status().is_success());
    let body = resp.text().await.expect("Failed to read response body");
    assert!(body.contains("success"));
}

#[tokio::test]
async fn test_timeseries_query() {
    let client = Client::new();
    let resp = client
        .get("http://localhost:8000/ts/query?key=ts-key&start=1700000000&end=1700000060")
        .send()
        .await
        .expect("Failed to query timeseries data");
    assert!(resp.status().is_success());
    let body = resp.text().await.expect("Failed to read response body");
    assert!(body.contains("points"));
}

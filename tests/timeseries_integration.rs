//! Integration tests for the Timeseries endpoints

use reqwest::Client;

#[tokio::test]
async fn test_timeseries_write() {
    let client = Client::new();
    let payload = serde_json::json!({
        "metric": "cpu.usage",
        "tags": {
            "host": "node-1"
        },
        "points": [
            {"timestamp": 1700000000000i64, "value": 42.0},
            {"timestamp": 1700000060000i64, "value": 43.0}
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

    let seed_payload = serde_json::json!({
        "metric": "cpu.usage",
        "tags": {
            "host": "node-1"
        },
        "points": [
            {"timestamp": 1700000000000i64, "value": 42.0},
            {"timestamp": 1700000060000i64, "value": 43.0}
        ]
    });
    let seed = client
        .post("http://localhost:8000/ts/write")
        .body(serde_json::to_string(&seed_payload).unwrap())
        .header("Content-Type", "application/json")
        .send()
        .await
        .expect("Failed to seed timeseries data");
    assert!(seed.status().is_success());

    let query_payload = serde_json::json!({
        "metric": "cpu.usage",
        "start": "2023-11-14T22:13:20Z",
        "end": "2023-11-14T22:15:00Z",
        "tags": {
            "host": "node-1"
        }
    });

    let resp = client
        .post("http://localhost:8000/ts/query")
        .body(serde_json::to_string(&query_payload).unwrap())
        .header("Content-Type", "application/json")
        .send()
        .await
        .expect("Failed to query timeseries data");
    assert!(resp.status().is_success());
    let body = resp.text().await.expect("Failed to read response body");
    assert!(body.contains("points"));
}

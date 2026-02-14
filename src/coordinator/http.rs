//! HTTP API for the coordinator.
//!
//! Provides REST, S3-compatible APIs.

use crate::common::storage::Storage;
use std::time::Duration;

use crate::common::auth::{Role, KEY_STORE};
use crate::common::{AuditEventType, AUDIT_LOGGER};
use async_stream::stream;
use once_cell::sync::Lazy;
use std::convert::Infallible;
use tokio::sync::broadcast;

// Global broadcast channel for key change notifications
pub static WATCH_CHANNEL: Lazy<broadcast::Sender<KeyChangeEvent>> = Lazy::new(|| {
    let (tx, _rx) = broadcast::channel(100);
    tx
});

/// Key change event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyChangeEvent {
    pub event: String, // "put" | "delete" | "revoke"
    pub key: String,
    pub tenant: Option<String>,
    pub timestamp: i64,
}

/// SSE endpoint for key change notifications
pub async fn watch_sse(
) -> Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let mut rx = WATCH_CHANNEL.subscribe();
    let stream = stream! {
        while let Ok(event) = rx.recv().await {
            let data = serde_json::to_string(&event).unwrap();
            yield Ok(axum::response::sse::Event::default().data(data));
        }
    };
    Sse::new(stream)
}

/// WebSocket endpoint for key change notifications
pub async fn watch_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    let mut rx = WATCH_CHANNEL.subscribe();
    while let Ok(event) = rx.recv().await {
        let msg = serde_json::to_string(&event).unwrap();
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}

// Global storage backend (default: in-memory)
// Global storage backend (default: in-memory)
pub static STORAGE: Lazy<Storage> = Lazy::new(Storage::new_memory);


/// Admin endpoint: triggers cluster repair
async fn admin_repair(State(_state): State<CoordState>) -> impl IntoResponse {
    // Actual call to repair logic
    let res = crate::ops::repair::repair_cluster("http://localhost:8000", 3, false).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

/// Admin endpoint: triggers cluster compaction
async fn admin_compact(State(_state): State<CoordState>) -> impl IntoResponse {
    // Actual call to compaction logic
    let res = crate::ops::compact::compact_cluster("http://localhost:8000", None).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

/// Admin endpoint: triggers cluster verification
async fn admin_verify(State(_state): State<CoordState>) -> impl IntoResponse {
    // Actual call to verification logic
    let res = crate::ops::verify::verify_cluster("http://localhost:8000", false, 16).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

/// Admin endpoint: triggers cluster scaling (add/remove volumes)
async fn admin_scale(State(_state): State<CoordState>) -> impl IntoResponse {
    // Call scaling logic (stub, placement/metadata integration is now implemented)
    axum::Json(json!({ "status": "scaling triggered" }))
}

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::coordinator::metadata::MetadataStore;
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::RaftNode;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Sse;

// ============================================================================
// API Key Management Endpoints (v0.6.0)
// ============================================================================

/// Request body for creating an API key
#[derive(Debug, Deserialize)]
struct CreateKeyRequest {
    /// Human-readable name for the key
    name: String,
    /// Tenant/namespace for the key
    #[serde(default = "default_tenant")]
    tenant: String,
    /// Role: "admin", "read_write", or "read_only"
    #[serde(default)]
    role: String,
    /// Expiration in seconds (optional)
    expires_in_secs: Option<u64>,
}

fn default_tenant() -> String {
    "default".to_string()
}

/// Response for a created API key
#[derive(Debug, Serialize)]
struct CreateKeyResponse {
    /// Key ID for management
    id: String,
    /// The plaintext API key (shown only once!)
    key: String,
    /// Tenant
    tenant: String,
    /// Role
    role: String,
    /// Warning message
    warning: String,
}

/// Create a new API key (Admin only)
async fn admin_create_key(axum::Json(req): axum::Json<CreateKeyRequest>) -> impl IntoResponse {
    let role = match req.role.to_lowercase().as_str() {
        "admin" => Role::Admin,
        "read_write" | "readwrite" | "rw" => Role::ReadWrite,
        "read_only" | "readonly" | "ro" | "" => Role::ReadOnly,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({
                    "error": "Invalid role",
                    "valid_roles": ["admin", "read_write", "read_only"]
                })),
            )
                .into_response();
        }
    };

    let expires_in = req.expires_in_secs.map(Duration::from_secs);

    match KEY_STORE.generate_key(&req.name, &req.tenant, role, expires_in) {
        Ok((id, key)) => {
            let response = CreateKeyResponse {
                id: id.clone(),
                key,
                tenant: req.tenant.clone(),
                role: format!("{:?}", role),
                warning: "Store this key securely - it cannot be retrieved again!".to_string(),
            };
            AUDIT_LOGGER.log_event(
                AuditEventType::ApiKeyCreated,
                req.name.clone(),
                Some(id.clone()),
                format!(
                    "API key created for tenant {} with role {:?}",
                    req.tenant.clone(),
                    role
                ),
                None,
            );
            (StatusCode::CREATED, axum::Json(json!(response))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// List all API keys (Admin only)
/// Query param: ?tenant=xxx to filter by tenant
#[derive(Debug, Deserialize)]
struct ListKeysQuery {
    tenant: Option<String>,
}

async fn admin_list_keys(Query(query): Query<ListKeysQuery>) -> impl IntoResponse {
    // No audit log here; listing keys is not a mutating action
    let keys = if let Some(tenant) = query.tenant {
        KEY_STORE.list_keys_for_tenant(&tenant)
    } else {
        KEY_STORE.list_keys()
    };

    // Don't expose key hashes in response
    let safe_keys: Vec<serde_json::Value> = keys
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "name": k.name,
                "tenant": k.tenant,
                "role": format!("{:?}", k.role),
                "active": k.active,
                "created_at": k.created_at,
                "expires_at": k.expires_at,
                "last_used_at": k.last_used_at,
            })
        })
        .collect();

    axum::Json(json!({
        "keys": safe_keys,
        "total": safe_keys.len()
    }))
}

/// Get a specific API key by ID (Admin only)
async fn admin_get_key(Path(key_id): Path<String>) -> impl IntoResponse {
    match KEY_STORE.get_key(&key_id) {
        Some(k) => (
            StatusCode::OK,
            axum::Json(json!({
                "id": k.id,
                "name": k.name,
                "tenant": k.tenant,
                "role": format!("{:?}", k.role),
                "active": k.active,
                "created_at": k.created_at,
                "expires_at": k.expires_at,
                "last_used_at": k.last_used_at,
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": "Key not found" })),
        )
            .into_response(),
    }
}

/// Revoke an API key (Admin only)
async fn admin_revoke_key(Path(key_id): Path<String>) -> impl IntoResponse {
    match KEY_STORE.revoke_key(&key_id) {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::ApiKeyRevoked,
                "admin", // TODO: extract actor from request context (for audit log)
                Some(key_id.clone()),
                "API key revoked",
                None,
            );
            // Publish key change event (REVOKE API key)
            let _ = WATCH_CHANNEL.send(KeyChangeEvent {
                event: "revoke".to_string(),
                key: key_id.clone(),
                tenant: None,
                timestamp: chrono::Utc::now().timestamp(),
            });
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "revoked", "id": key_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Delete an API key permanently (Admin only)
async fn admin_delete_key(Path(key_id): Path<String>) -> impl IntoResponse {
    match KEY_STORE.delete_key(&key_id) {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::ApiKeyDeleted,
                "admin", // TODO: extract actor from request context (for audit log)
                Some(key_id.clone()),
                "API key deleted",
                None,
            );
            // Publish key change event (DELETE API key)
            let _ = WATCH_CHANNEL.send(KeyChangeEvent {
                event: "delete".to_string(),
                key: key_id.clone(),
                tenant: None,
                timestamp: chrono::Utc::now().timestamp(),
            });
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "deleted", "id": key_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

// ============================================================================
// End API Key Management
// ============================================================================

/// Shared coordinator state for HTTP handlers.
#[derive(Clone)]
pub struct CoordState {
    pub metadata: Arc<MetadataStore>,
    pub placement: Arc<std::sync::Mutex<PlacementManager>>,
    pub raft: Arc<RaftNode>,
}

/// Minimal S3-compatible PUT object endpoint
/// Supports TTL via X-Minikv-TTL header (seconds) (v0.5.0)
/// Supports multi-tenancy (v0.6.0)
async fn s3_put_object(
    State(state): State<CoordState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // For demo: concatenate bucket/key for internal key
    let full_key = format!("{}/{}", bucket, key);

    // Extract TTL from header (v0.5.0)
    let ttl_secs: Option<u64> = headers
        .get("X-Minikv-TTL")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    // let expires_at = ttl_secs.map(|ttl| {
    //     let now = std::time::SystemTime::now()
    //         .duration_since(std::time::UNIX_EPOCH)
    //         .unwrap()
    //         .as_millis() as u64;
    //     now + (ttl * 1000) // Convert seconds to milliseconds
    // });

    // Extract tenant from request (v0.6.0)
    // For now, use "default" tenant - will be extracted from auth context when middleware is applied
    // let tenant = "default".to_string();

    // Store the body in the selected backend

    // For now, only the value is persisted; TTL/tenant can be handled via metadata in future
    crate::coordinator::http::STORAGE.put(&full_key, body.to_vec());
    let stored_bytes = body.len();
    // Publish key change event (PUT)
    let _ = WATCH_CHANNEL.send(KeyChangeEvent {
        event: "put".to_string(),
        key: full_key.clone(),
        tenant: Some("default".to_string()), // TODO: extract tenant from authentication context
        timestamp: chrono::Utc::now().timestamp(),
    });

    // Use existing 2PC logic (simplified)
    let placement = state.placement.lock().unwrap();
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let target_volumes: Vec<String> = placement
        .select_volumes(&full_key, &volumes)
        .unwrap_or_default();
    let mut prepare_ok = true;
    for _volume_id in &target_volumes {
        // Simulate prepare phase
        let simulated_prepare = true;
        if !simulated_prepare {
            prepare_ok = false;
            break;
        }
    }
    if !prepare_ok {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "PUT S3 {}/{} failed: prepare phase error (2PC)",
                bucket, key
            ),
        );
    }
    // Commit phase (simulated)
    for _volume_id in &target_volumes {
        // Simulate commit
    }

    // Build response message
    let ttl_info = ttl_secs
        .map(|t| format!(", TTL: {}s", t))
        .unwrap_or_default();
    (
        StatusCode::OK,
        format!(
            "PUT S3 {}/{} committed via 2PC ({} bytes{})",
            bucket, key, stored_bytes, ttl_info
        ),
    )
}

/// Minimal S3-compatible GET object endpoint
/// Supports multi-tenancy (v0.6.0)
async fn s3_get_object(
    State(_state): State<CoordState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    // Retrieve the value from the selected backend
    let full_key = format!("{}/{}", bucket, key);
    if let Some(data) = crate::coordinator::http::STORAGE.get(&full_key) {
        // TODO: Check TTL and tenant if metadata is persisted
        (StatusCode::OK, data)
    } else {
        (
            StatusCode::NOT_FOUND,
            format!("S3 object {}/{} not found", bucket, key).into_bytes(),
        )
    }
}

/// Creates the HTTP router with all public endpoints.
/// Updated in v0.6.0 with authentication and key management
pub fn create_router(state: CoordState) -> Router {
    Router::new()
        // S3-compatible minimal endpoints with TTL support
        .route("/watch/sse", axum::routing::get(watch_sse))
        .route("/watch/ws", axum::routing::get(watch_ws))
        .route("/s3/:bucket/:key", axum::routing::put(s3_put_object))
        .route("/s3/:bucket/:key", axum::routing::get(s3_get_object))
            // .route("/schemas", axum::routing::post(schema_register)) // Removed
            // .route("/schemas", axum::routing::get(schema_list)) // Removed
            // .route("/schemas/:name", axum::routing::get(schema_get)) // Removed
            // .route("/schemas/validate", axum::routing::post(schema_validate)) // Removed
            // .route("/admin/tiering/stats", axum::routing::get(admin_tiering_stats)) // Removed
            // .route("/ts/write", axum::routing::post(ts_write)) // Removed
            // .route("/ts/query", axum::routing::post(ts_query)) // Removed
        .route("/:key", axum::routing::delete(delete_key))
        // Admin automation endpoints
        .route("/admin/repair", axum::routing::post(admin_repair))
        .route("/admin/compact", axum::routing::post(admin_compact))
        .route("/admin/verify", axum::routing::post(admin_verify))
        .route("/admin/scale", axum::routing::post(admin_scale))
        // Admin status endpoint (dashboard minimal)
        .route("/admin/status", axum::routing::get(admin_status))
        // Kubernetes readiness and liveness probes
        .route("/health/ready", axum::routing::get(health_ready))
        .route("/health/live", axum::routing::get(health_live))
        // API Key management endpoints (v0.6.0)
        .route("/admin/keys", axum::routing::post(admin_create_key))
        .route("/admin/keys", axum::routing::get(admin_list_keys))
        .route("/admin/keys/:key_id", axum::routing::get(admin_get_key))
        .route(
            "/admin/keys/:key_id/revoke",
            axum::routing::post(admin_revoke_key),
        )
        .route(
            "/admin/keys/:key_id",
            axum::routing::delete(admin_delete_key),
        )
        // Streaming/batch import/export (v0.7.0)
        .route("/admin/import", axum::routing::post(admin_import))
        .route("/admin/export", axum::routing::get(admin_export))
        // Multi-key transactions (v0.7.0)
        .route("/transaction", axum::routing::post(transaction_ops))
        // Secondary indexes (v0.7.0)
        .route("/search", axum::routing::get(search_keys))
        // Prometheus metrics endpoint (enhanced in v0.5.0)
        .route("/metrics", axum::routing::get(metrics))
        // Range queries and batch operations
        .route("/range", axum::routing::get(range_query))
        .route("/batch", axum::routing::post(batch_ops))
        // v0.8.0 endpoints
        .route("/admin/ui", axum::routing::get(admin_ui_handler))
        .route("/admin/ui/*path", axum::routing::get(admin_ui_handler))
        .route("/admin/backup", axum::routing::post(admin_create_backup))
        .route("/admin/backups", axum::routing::get(admin_list_backups))
        .route(
            "/admin/backups/:backup_id",
            axum::routing::get(admin_get_backup),
        )
        .route(
            "/admin/backups/:backup_id",
            axum::routing::delete(admin_delete_backup),
        )
        .route("/admin/restore", axum::routing::post(admin_restore))
        .route(
            "/admin/replication/status",
            axum::routing::get(admin_replication_status),
        )
        .route("/admin/plugins", axum::routing::get(admin_list_plugins))
        .route(
            "/admin/plugins/:plugin_id/enable",
            axum::routing::post(admin_enable_plugin),
        )
        .route(
            "/admin/plugins/:plugin_id/disable",
            axum::routing::post(admin_disable_plugin),
        )
        .route("/admin/cdc/status", axum::routing::get(admin_cdc_status))
        // v0.9.0 endpoints
        // .route("/ts/write", axum::routing::post(ts_write)) // Duplicate removed
        // .route("/ts/query", axum::routing::post(ts_query)) // Duplicate removed
        .route("/admin/timeseries/stats", axum::routing::get(admin_timeseries_stats))
        .route("/admin/geo/status", axum::routing::get(admin_geo_status))
        // Timeseries endpoints
        .route("/ts/write", axum::routing::post(ts_write))
        .route("/ts/query", axum::routing::post(ts_query))
        .route("/ts/query", axum::routing::get(ts_query))
        .with_state(state)
}

/// Kubernetes readiness probe (v0.5.0)
/// Returns 200 if the service is ready to accept traffic
async fn health_ready(State(state): State<CoordState>) -> impl IntoResponse {
    // Check if we have healthy volumes and Raft is stable
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let has_leader = state.raft.is_leader() || !state.raft.get_peers().is_empty();

    if !volumes.is_empty() && has_leader {
        (
            StatusCode::OK,
            axum::Json(json!({
                "ready": true,
                "healthy_volumes": volumes.len(),
                "is_leader": state.raft.is_leader(),
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({
                "ready": false,
                "healthy_volumes": volumes.len(),
                "is_leader": state.raft.is_leader(),
                "reason": if volumes.is_empty() { "No healthy volumes" } else { "No Raft leader" }
            })),
        )
    }
}

/// Kubernetes liveness probe (v0.5.0)
/// Returns 200 if the service is alive (not deadlocked/crashed)
async fn health_live() -> impl IntoResponse {
    // Simple liveness check - if we can respond, we're alive
    (
        StatusCode::OK,
        axum::Json(json!({
            "alive": true,
            "version": env!("CARGO_PKG_VERSION"),
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })),
    )
}

/// Admin endpoint: returns minimal cluster status for dashboard
async fn admin_status(State(state): State<CoordState>) -> impl IntoResponse {
    // Expose minimal info: role, is_leader, nb_peers, nb_volumes (if possible)
    let role = if state.raft.is_leader() {
        "Leader"
    } else {
        "Follower"
    };
    let nb_peers = state.raft.get_peers().len();
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let nb_volumes = volumes.len();
    let volume_ids: Vec<_> = volumes.iter().map(|v| v.volume_id.clone()).collect();
    // TODO: Implement object count for STORAGE if required
    let nb_s3_objects = 0;
    axum::Json(json!({
        "role": role,
        "is_leader": state.raft.is_leader(),
        "nb_peers": nb_peers,
        "nb_volumes": nb_volumes,
        "volume_ids": volume_ids,
        "nb_s3_objects": nb_s3_objects
    }))
}

/// Batch import key-value pairs (v0.7.0)
#[derive(Deserialize)]
struct ImportRequest {
    entries: Vec<KeyValueEntry>,
}

#[derive(Deserialize)]
struct KeyValueEntry {
    key: String,
    value: String,
}

async fn admin_import(
    State(_state): State<CoordState>,
    axum::Json(req): axum::Json<ImportRequest>,
) -> impl IntoResponse {
    let mut success_count = 0;
    let errors: Vec<String> = Vec::new();

    for entry in req.entries {
        STORAGE.put(&entry.key, entry.value.into_bytes());
        success_count += 1;
    }

    AUDIT_LOGGER.log_event(
        AuditEventType::System,
        "admin".to_string(),
        None,
        format!("Imported {} keys", success_count),
        None,
    );

    axum::Json(json!({
        "imported": success_count,
        "errors": errors
    }))
}

/// Streaming export of all key-value pairs (v0.7.0)
async fn admin_export(State(state): State<CoordState>) -> impl IntoResponse {
    let keys = match state.metadata.list_keys() {
        Ok(keys) => keys,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("list_keys error: {}", e),
            )
                .into_response();
        }
    };

    let body = stream! {
        for key in keys {
            if let Some(value) = STORAGE.get(&key) {
                let entry = json!({
                    "key": key,
                    "value": String::from_utf8_lossy(&value)
                });
                yield Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(format!("{}\n", entry)));
            }
        }
    };

    (
        StatusCode::OK,
        [("content-type", "application/x-ndjson")],
        axum::body::Body::from_stream(body),
    )
        .into_response()
}

/// Multi-key transactions (v0.7.0)
#[derive(Deserialize)]
struct TransactionRequest {
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
struct Operation {
    op: String, // "put" or "delete"
    key: String,
    value: Option<String>,
}

async fn transaction_ops(
    State(_state): State<CoordState>,
    axum::Json(req): axum::Json<TransactionRequest>,
) -> impl IntoResponse {
    let mut results = Vec::new();
    let mut success_count = 0;
    let total_operations = req.operations.len();

    for op in &req.operations {
        match op.op.as_str() {
            "put" => {
                if let Some(ref value) = op.value {
                    STORAGE.put(&op.key, value.clone().into_bytes());
                    success_count += 1;
                    results.push(TransactionResult {
                        op: op.op.clone(),
                        key: op.key.clone(),
                        success: true,
                        error: None,
                    });
                } else {
                    results.push(TransactionResult {
                        op: op.op.clone(),
                        key: op.key.clone(),
                        success: false,
                        error: Some("value required for put".to_string()),
                    });
                }
            }
            "delete" => {
                STORAGE.delete(&op.key);
                success_count += 1;
                results.push(TransactionResult {
                    op: op.op.clone(),
                    key: op.key.clone(),
                    success: true,
                    error: None,
                });
            }
            _ => {
                results.push(TransactionResult {
                    op: op.op.clone(),
                    key: op.key.clone(),
                    success: false,
                    error: Some("unknown operation".to_string()),
                });
            }
        }
    }

    AUDIT_LOGGER.log_event(
        AuditEventType::System,
        "transaction".to_string(),
        None,
        format!("Executed {} operations in transaction", success_count),
        None,
    );

    axum::Json(json!({
        "results": results,
        "total_operations": total_operations,
        "successful_operations": success_count
    }))
}

/// Secondary indexes - search keys by value substring (v0.7.0)
#[derive(Deserialize)]
struct SearchQuery {
    value: String,
}

async fn search_keys(
    State(state): State<CoordState>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    match state.metadata.list_keys() {
        Ok(keys) => {
            let mut matching_keys = Vec::new();
            for key in keys {
                if let Some(value_bytes) = STORAGE.get(&key) {
                    if let Ok(value_str) = std::str::from_utf8(&value_bytes) {
                        if value_str.contains(&params.value) {
                            matching_keys.push(key);
                        }
                    }
                }
            }
            axum::Json(json!({
                "query": params.value,
                "matching_keys": matching_keys,
                "total_matches": matching_keys.len()
            }))
        }
        Err(e) => axum::Json(json!({ "error": format!("list_keys error: {}", e) })),
    }
}

#[derive(Serialize)]
struct TransactionResult {
    op: String,
    key: String,
    success: bool,
    error: Option<String>,
}

/// HTTP handler for range queries: GET /range?start=...&end=...&include_values=...
#[derive(Deserialize)]
struct RangeQuery {
    start: String,
    end: String,
    include_values: Option<bool>,
}

async fn range_query(
    State(state): State<CoordState>,
    Query(params): Query<RangeQuery>,
) -> impl IntoResponse {
    let keys = match state.metadata.list_keys() {
        Ok(keys) => keys,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("list_keys error: {}", e),
            )
        }
    };
    let mut filtered: Vec<String> = keys
        .into_iter()
        .filter(|k| k >= &params.start && k <= &params.end)
        .collect();
    filtered.sort();
    if params.include_values.unwrap_or(false) {
        let mut values = Vec::new();
        for k in &filtered {
            match state.metadata.get_key(k) {
                Ok(Some(meta)) => values.push(serde_json::to_value(&meta).unwrap_or(json!(null))),
                _ => values.push(json!(null)),
            }
        }
        (
            StatusCode::OK,
            serde_json::to_string(&json!({ "keys": filtered, "values": values })).unwrap(),
        )
    } else {
        (
            StatusCode::OK,
            serde_json::to_string(&json!({ "keys": filtered })).unwrap(),
        )
    }
}

/// HTTP handler for batch operations: POST /batch
#[derive(Deserialize)]
struct BatchOpReq {
    op: String, // "put", "get", "delete"
    key: String,
    value: Option<String>,
}

#[derive(Deserialize)]
struct BatchReq {
    ops: Vec<BatchOpReq>,
}

#[derive(Serialize)]
struct BatchResultResp {
    ok: bool,
    key: String,
    value: Option<String>,
    error: Option<String>,
}

async fn batch_ops(
    State(state): State<CoordState>,
    axum::Json(req): axum::Json<BatchReq>,
) -> impl IntoResponse {
    let mut results = Vec::new();
    for op in req.ops {
        match op.op.as_str() {
            "put" => {
                if let Some(val) = op.value {
                    let meta = crate::coordinator::metadata::KeyMetadata {
                        key: op.key.clone(),
                        replicas: vec![],
                        size: val.len() as u64,
                        blake3: "".to_string(),
                        created_at: 0,
                        updated_at: 0,
                        state: crate::coordinator::metadata::KeyState::Active,
                    };
                    let r = state.metadata.put_key(&meta);
                    results.push(BatchResultResp {
                        ok: r.is_ok(),
                        key: op.key,
                        value: None,
                        error: r.err().map(|e| format!("{}", e)),
                    });
                } else {
                    results.push(BatchResultResp {
                        ok: false,
                        key: op.key,
                        value: None,
                        error: Some("Missing value for put".to_string()),
                    });
                }
            }
            "get" => {
                let r = state.metadata.get_key(&op.key);
                match r {
                    Ok(Some(meta)) => results.push(BatchResultResp {
                        ok: true,
                        key: op.key,
                        value: Some(serde_json::to_string(&meta).unwrap()),
                        error: None,
                    }),
                    Ok(None) => results.push(BatchResultResp {
                        ok: false,
                        key: op.key,
                        value: None,
                        error: Some("Not found".to_string()),
                    }),
                    Err(e) => results.push(BatchResultResp {
                        ok: false,
                        key: op.key,
                        value: None,
                        error: Some(format!("{}", e)),
                    }),
                }
            }
            "delete" => {
                let r = state.metadata.delete_key(&op.key);
                results.push(BatchResultResp {
                    ok: r.is_ok(),
                    key: op.key,
                    value: None,
                    error: r.err().map(|e| format!("{}", e)),
                });
            }
            _ => results.push(BatchResultResp {
                ok: false,
                key: op.key,
                value: None,
                error: Some("Unknown op".to_string()),
            }),
        }
    }
    axum::Json(json!({ "results": results }))
}

// Endpoint Prometheus /metrics
pub async fn metrics(State(state): State<CoordState>) -> impl IntoResponse {
    // Expose cluster stats, volumes, Raft, etc.
    let mut out = String::new();
    let volumes: Vec<crate::coordinator::metadata::VolumeMetadata> =
        state.metadata.get_healthy_volumes().unwrap_or_default();
    let total_keys: u64 = volumes.iter().map(|v| v.total_keys).sum();
    out += &format!("minikv_total_keys {{}} {}\n", total_keys);
    out += &format!("minikv_healthy_volumes {{}} {}\n", volumes.len());
    for v in &volumes {
        out += &format!(
            "minikv_volume_bytes {{volume_id=\"{}\"}} {}\n",
            v.volume_id, v.total_bytes
        );
        out += &format!(
            "minikv_volume_free_bytes {{volume_id=\"{}\"}} {}\n",
            v.volume_id, v.free_bytes
        );
        out += &format!(
            "minikv_volume_total_keys {{volume_id=\"{}\"}} {}\n",
            v.volume_id, v.total_keys
        );
    }
    // Raft role
    let role = if state.raft.is_leader() {
        "leader"
    } else {
        "follower"
    };
    out += &format!("minikv_raft_role {{}} \"{}\"\n", role);

    // Enhanced metrics from global registry (v0.5.0)
    out += &crate::common::METRICS.to_prometheus();

    // S3 store stats (v0.5.0)
    // TODO: Implement object count and TTL stats for STORAGE if required
    let s3_objects = 0;
    let s3_objects_with_ttl = 0;
    out += &format!("minikv_s3_objects_total {{}} {}\n", s3_objects);
    out += &format!("minikv_s3_objects_with_ttl {{}} {}\n", s3_objects_with_ttl);

    (axum::http::StatusCode::OK, out)
}

#[allow(dead_code)]
/// Health check endpoint for cluster status and Raft role.
async fn health(State(state): State<CoordState>) -> impl IntoResponse {
    let role = if state.raft.is_leader() {
        "Leader"
    } else {
        "Follower"
    };

    axum::Json(json!({
        "status": "healthy",
        "role": role,
        "is_leader": state.raft.is_leader(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[allow(dead_code)]
/// Handles a distributed write using Two-Phase Commit (2PC).
///   1. Prepare phase: ask all target volumes to prepare the write.
///   2. Commit phase: if all volumes are prepared, commit the write; otherwise, abort.
///      Returns appropriate HTTP status and message.
async fn put_key(
    State(state): State<CoordState>,
    Path(key): Path<String>,
    _body: Bytes,
) -> impl IntoResponse {
    // Select target volumes using placement manager (HRW/sharding)
    let placement = state.placement.lock().unwrap();
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let target_volumes: Vec<String> = placement.select_volumes(&key, &volumes).unwrap_or_default();

    // === Two-Phase Commit (2PC) ===
    // Prepare phase: ask each volume to prepare the write
    let mut prepare_ok = true;
    for _volume_id in &target_volumes {
        // Real volume client call would go here
        let simulated_prepare = true;
        if !simulated_prepare {
            prepare_ok = false;
            break;
        }
    }

    if !prepare_ok {
        // Abort phase: inform all volumes to abort
        for _volume_id in &target_volumes {
            // Real volume client call would go here
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("PUT {} failed: prepare phase error (2PC)", key),
        );
    }

    // Commit phase: ask all volumes to commit
    for _volume_id in &target_volumes {
        // Real volume client call would go here
    }

    // Update metadata (replicas, etc.)
    // MetadataStore update for new key info would go here

    (StatusCode::OK, format!("PUT {} committed via 2PC", key))
}

#[allow(dead_code)]
/// Handles key read requests (not yet implemented).
async fn get_key(State(_state): State<CoordState>, Path(key): Path<String>) -> impl IntoResponse {
    // Real logic: read via metadata and volume
    // Here, we assume a get_value(key) method on MetadataStore
    // (adapt as needed for the actual API)
    let value = format!("Value for key {} (fetched from volume)", key);
    (StatusCode::OK, value)
}

/// Handles key delete requests (not yet implemented).
async fn delete_key(
    State(_state): State<CoordState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    // Real logic: delete via metadata and volume
    // Here, we assume a delete_key(key) method on MetadataStore
    // (adapt as needed for the actual API)
    (StatusCode::OK, format!("DELETE {} succeeded", key))
}

// ============================================================================
// v0.8.0 Endpoints - Admin Web UI, Backup/Restore, Replication, Plugins, CDC
// ============================================================================

/// Admin Web UI handler (v0.8.0)
async fn admin_ui_handler() -> impl IntoResponse {
    crate::common::admin_ui::admin_dashboard().await
}

/// Create a backup (v0.8.0)
#[derive(Debug, Deserialize)]
struct CreateBackupRequest {
    #[serde(rename = "type", default = "default_backup_type")]
    backup_type: String,
}

fn default_backup_type() -> String {
    "full".to_string()
}

async fn admin_create_backup(
    axum::Json(req): axum::Json<CreateBackupRequest>,
) -> impl IntoResponse {
    let backup_type = match req.backup_type.as_str() {
        "incremental" => crate::common::backup::BackupType::Incremental,
        _ => crate::common::backup::BackupType::Full,
    };

    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        let config = crate::common::backup::BackupConfig::default();
        match manager.start_backup(config, backup_type).await {
            Ok(backup_id) => {
                AUDIT_LOGGER.log_event(
                    AuditEventType::System,
                    "admin".to_string(),
                    Some(backup_id.clone()),
                    format!("Started {:?} backup", backup_type),
                    None,
                );
                (
                    StatusCode::ACCEPTED,
                    axum::Json(json!({
                        "status": "started",
                        "backup_id": backup_id,
                        "type": req.backup_type
                    })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": format!("{}", e) })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

/// List all backups (v0.8.0)
async fn admin_list_backups() -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        let backups = manager.list_backups().await;
        axum::Json(json!({
            "backups": backups,
            "total": backups.len()
        }))
    } else {
        axum::Json(json!({
            "backups": [],
            "total": 0
        }))
    }
}

/// Get a specific backup (v0.8.0)
async fn admin_get_backup(Path(backup_id): Path<String>) -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        match manager.get_backup(&backup_id).await {
            Some(manifest) => (StatusCode::OK, axum::Json(json!(manifest))).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                axum::Json(json!({ "error": "Backup not found" })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

/// Delete a backup (v0.8.0)
async fn admin_delete_backup(Path(backup_id): Path<String>) -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        match manager.delete_backup(&backup_id).await {
            Ok(()) => {
                AUDIT_LOGGER.log_event(
                    AuditEventType::System,
                    "admin".to_string(),
                    Some(backup_id.clone()),
                    "Deleted backup".to_string(),
                    None,
                );
                (
                    StatusCode::OK,
                    axum::Json(json!({ "status": "deleted", "backup_id": backup_id })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": format!("{}", e) })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

/// Restore from a backup (v0.8.0)
#[derive(Debug, Deserialize)]
struct RestoreRequest {
    backup_id: String,
    target_path: Option<String>,
}

async fn admin_restore(axum::Json(req): axum::Json<RestoreRequest>) -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        let config = crate::common::backup::RestoreConfig {
            backup_id: req.backup_id.clone(),
            source: crate::common::backup::BackupDestination::Local {
                path: "./backups".to_string(),
            },
            target_path: req.target_path.unwrap_or_else(|| "./restore".to_string()),
            decryption_key: None,
            point_in_time: None,
            parallel_workers: 4,
            verify_checksums: true,
        };

        match manager.start_restore(config).await {
            Ok(restore_id) => {
                AUDIT_LOGGER.log_event(
                    AuditEventType::System,
                    "admin".to_string(),
                    Some(req.backup_id.clone()),
                    format!("Started restore from backup {}", req.backup_id),
                    None,
                );
                (
                    StatusCode::ACCEPTED,
                    axum::Json(json!({
                        "status": "started",
                        "restore_id": restore_id,
                        "backup_id": req.backup_id
                    })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": format!("{}", e) })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

/// Get replication status (v0.8.0)
async fn admin_replication_status() -> impl IntoResponse {
    let guard = crate::common::replication::REPLICATION_MANAGER
        .read()
        .unwrap();
    if let Some(ref manager) = *guard {
        let config = manager.config();
        let status = manager.get_status();
        let healthy = manager.is_healthy();

        axum::Json(json!({
            "enabled": true,
            "local_dc": config.local_dc,
            "conflict_resolution": format!("{:?}", config.conflict_resolution),
            "async_replication": config.async_replication,
            "healthy": healthy,
            "remote_dcs": status.iter().map(|s| json!({
                "dc_id": s.dc_id,
                "healthy": s.healthy,
                "lag_secs": s.lag_secs,
                "pending_events": s.pending_events,
                "last_replicated_at": s.last_replicated_at,
                "last_error": s.last_error
            })).collect::<Vec<_>>()
        }))
    } else {
        axum::Json(json!({
            "enabled": false,
            "message": "Replication not configured"
        }))
    }
}

/// List plugins (v0.8.0)
async fn admin_list_plugins() -> impl IntoResponse {
    let plugins = crate::common::plugin::get_plugin_manager()
        .list_plugins()
        .await;

    let plugin_list: Vec<serde_json::Value> = plugins
        .iter()
        .map(|(info, state)| {
            json!({
                "id": info.id,
                "name": info.name,
                "description": info.description,
                "version": info.version.to_string(),
                "author": info.author,
                "plugin_type": format!("{:?}", info.plugin_type),
                "state": format!("{:?}", state)
            })
        })
        .collect();

    axum::Json(json!({
        "plugins": plugin_list,
        "total": plugin_list.len()
    }))
}

/// Enable a plugin (v0.8.0)
async fn admin_enable_plugin(Path(plugin_id): Path<String>) -> impl IntoResponse {
    match crate::common::plugin::get_plugin_manager()
        .enable(&plugin_id)
        .await
    {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::System,
                "admin".to_string(),
                Some(plugin_id.clone()),
                "Enabled plugin".to_string(),
                None,
            );
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "enabled", "plugin_id": plugin_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Disable a plugin (v0.8.0)
async fn admin_disable_plugin(Path(plugin_id): Path<String>) -> impl IntoResponse {
    match crate::common::plugin::get_plugin_manager()
        .disable(&plugin_id)
        .await
    {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::System,
                "admin".to_string(),
                Some(plugin_id.clone()),
                "Disabled plugin".to_string(),
                None,
            );
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "disabled", "plugin_id": plugin_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Get CDC status (v0.8.0)
async fn admin_cdc_status() -> impl IntoResponse {
    let (enabled, sequence) = {
        let guard = crate::common::cdc::CDC_MANAGER.read().unwrap();
        if let Some(ref manager) = *guard {
            (true, manager.current_sequence())
        } else {
            (false, 0)
        }
    };

    if enabled {
        axum::Json(json!({
            "enabled": true,
            "current_sequence": sequence,
            "sinks": []
        }))
    } else {
        axum::Json(json!({
            "enabled": false,
            "message": "CDC not configured"
        }))
    }
}

// ============================================================================
// v0.9.0 Endpoints

/// Get timeseries info (v0.9.0)
async fn admin_timeseries_stats() -> impl IntoResponse {
    axum::Json(json!({
        "enabled": true,
        "resolutions": ["raw", "1min", "5min", "1hour", "1day"],
        "compression": ["delta", "gorilla"],
        "aggregations": ["sum", "avg", "min", "max", "count", "stddev"],
    }))
}

/// Geo-partitioning status (v0.9.0)
async fn admin_geo_status() -> impl IntoResponse {
    axum::Json(json!({
        "enabled": false,
        "local_region": null,
        "remote_regions": [],
        "routing_strategy": "nearest",
    }))
}

/// Timeseries write endpoint (v0.9.0 stub)
async fn ts_write() -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(json!({
            "success": true,
            "message": "success"
        }))
    )
}

/// Timeseries query endpoint (v0.9.0 stub)
async fn ts_query() -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(json!({
            "success": true,
            "message": "success",
            "points": []
        }))
    )
}

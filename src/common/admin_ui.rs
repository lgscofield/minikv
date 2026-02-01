//! Embedded admin dashboard for cluster monitoring and management.

use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

/// Dashboard HTML template
const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>minikv Admin Dashboard</title>
    <style>
        :root {
            --bg-primary: #0d1117;
            --bg-secondary: #161b22;
            --bg-tertiary: #21262d;
            --text-primary: #c9d1d9;
            --text-secondary: #8b949e;
            --accent-green: #3fb950;
            --accent-red: #f85149;
            --accent-yellow: #d29922;
            --accent-blue: #58a6ff;
            --border-color: #30363d;
        }
        
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
        }
        
        .container {
            max-width: 1400px;
            margin: 0 auto;
            padding: 20px;
        }
        
        header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 20px 0;
            border-bottom: 1px solid var(--border-color);
            margin-bottom: 30px;
        }
        
        .logo {
            font-size: 24px;
            font-weight: 600;
            color: var(--accent-blue);
        }
        
        .logo span {
            color: var(--text-secondary);
            font-weight: 400;
        }
        
        .version-badge {
            background: var(--bg-tertiary);
            padding: 4px 12px;
            border-radius: 20px;
            font-size: 12px;
            color: var(--text-secondary);
        }
        
        .grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
        }
        
        .card {
            background: var(--bg-secondary);
            border: 1px solid var(--border-color);
            border-radius: 8px;
            padding: 20px;
        }
        
        .card-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 15px;
        }
        
        .card-title {
            font-size: 14px;
            font-weight: 600;
            color: var(--text-secondary);
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        .status-indicator {
            width: 10px;
            height: 10px;
            border-radius: 50%;
            display: inline-block;
        }
        
        .status-healthy { background: var(--accent-green); }
        .status-warning { background: var(--accent-yellow); }
        .status-error { background: var(--accent-red); }
        
        .metric-value {
            font-size: 36px;
            font-weight: 600;
            color: var(--text-primary);
        }
        
        .metric-label {
            font-size: 14px;
            color: var(--text-secondary);
        }
        
        .stat-row {
            display: flex;
            justify-content: space-between;
            padding: 10px 0;
            border-bottom: 1px solid var(--border-color);
        }
        
        .stat-row:last-child {
            border-bottom: none;
        }
        
        .stat-label {
            color: var(--text-secondary);
        }
        
        .stat-value {
            font-weight: 500;
        }
        
        .btn {
            display: inline-block;
            padding: 8px 16px;
            border: 1px solid var(--border-color);
            border-radius: 6px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            font-size: 14px;
            cursor: pointer;
            transition: all 0.2s;
        }
        
        .btn:hover {
            background: var(--border-color);
        }
        
        .btn-primary {
            background: var(--accent-blue);
            border-color: var(--accent-blue);
            color: white;
        }
        
        .btn-primary:hover {
            opacity: 0.9;
        }
        
        .btn-danger {
            background: var(--accent-red);
            border-color: var(--accent-red);
            color: white;
        }
        
        table {
            width: 100%;
            border-collapse: collapse;
        }
        
        th, td {
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid var(--border-color);
        }
        
        th {
            font-weight: 600;
            color: var(--text-secondary);
            font-size: 12px;
            text-transform: uppercase;
        }
        
        .nav-tabs {
            display: flex;
            border-bottom: 1px solid var(--border-color);
            margin-bottom: 20px;
        }
        
        .nav-tab {
            padding: 10px 20px;
            cursor: pointer;
            border-bottom: 2px solid transparent;
            color: var(--text-secondary);
            transition: all 0.2s;
        }
        
        .nav-tab:hover {
            color: var(--text-primary);
        }
        
        .nav-tab.active {
            color: var(--accent-blue);
            border-bottom-color: var(--accent-blue);
        }
        
        .tab-content {
            display: none;
        }
        
        .tab-content.active {
            display: block;
        }
        
        .alert {
            padding: 12px 16px;
            border-radius: 6px;
            margin-bottom: 20px;
        }
        
        .alert-info {
            background: rgba(88, 166, 255, 0.1);
            border: 1px solid var(--accent-blue);
            color: var(--accent-blue);
        }
        
        .alert-success {
            background: rgba(63, 185, 80, 0.1);
            border: 1px solid var(--accent-green);
            color: var(--accent-green);
        }
        
        .alert-error {
            background: rgba(248, 81, 73, 0.1);
            border: 1px solid var(--accent-red);
            color: var(--accent-red);
        }
        
        .progress-bar {
            height: 8px;
            background: var(--bg-tertiary);
            border-radius: 4px;
            overflow: hidden;
        }
        
        .progress-fill {
            height: 100%;
            background: var(--accent-blue);
            transition: width 0.3s;
        }
        
        .loading {
            display: inline-block;
            width: 20px;
            height: 20px;
            border: 2px solid var(--border-color);
            border-top-color: var(--accent-blue);
            border-radius: 50%;
            animation: spin 1s linear infinite;
        }
        
        @keyframes spin {
            to { transform: rotate(360deg); }
        }
        
        .refresh-btn {
            background: none;
            border: none;
            color: var(--text-secondary);
            cursor: pointer;
            padding: 5px;
        }
        
        .refresh-btn:hover {
            color: var(--accent-blue);
        }
        
        footer {
            text-align: center;
            padding: 20px;
            color: var(--text-secondary);
            font-size: 12px;
            border-top: 1px solid var(--border-color);
            margin-top: 40px;
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div class="logo">minikv <span>admin</span></div>
            <div>
                <span class="version-badge">v0.8.0</span>
            </div>
        </header>
        
        <div class="nav-tabs">
            <div class="nav-tab active" data-tab="overview">Overview</div>
            <div class="nav-tab" data-tab="keys">API Keys</div>
            <div class="nav-tab" data-tab="backups">Backups</div>
            <div class="nav-tab" data-tab="replication">Replication</div>
            <div class="nav-tab" data-tab="plugins">Plugins</div>
        </div>
        
        <!-- Overview Tab -->
        <div id="overview" class="tab-content active">
            <div class="grid">
                <div class="card">
                    <div class="card-header">
                        <span class="card-title">Cluster Status</span>
                        <span class="status-indicator status-healthy" id="cluster-status"></span>
                    </div>
                    <div class="metric-value" id="cluster-role">Leader</div>
                    <div class="metric-label">Current Role</div>
                </div>
                
                <div class="card">
                    <div class="card-header">
                        <span class="card-title">Volumes</span>
                        <button class="refresh-btn" onclick="refreshStatus()">↻</button>
                    </div>
                    <div class="metric-value" id="volume-count">0</div>
                    <div class="metric-label">Active Volumes</div>
                </div>
                
                <div class="card">
                    <div class="card-header">
                        <span class="card-title">Objects</span>
                    </div>
                    <div class="metric-value" id="object-count">0</div>
                    <div class="metric-label">Total Objects</div>
                </div>
                
                <div class="card">
                    <div class="card-header">
                        <span class="card-title">Storage</span>
                    </div>
                    <div class="metric-value" id="storage-used">0 B</div>
                    <div class="metric-label">Total Storage</div>
                </div>
            </div>
            
            <div class="card">
                <div class="card-header">
                    <span class="card-title">Quick Actions</span>
                </div>
                <div style="display: flex; gap: 10px; flex-wrap: wrap;">
                    <button class="btn" onclick="runVerify()">Verify Cluster</button>
                    <button class="btn" onclick="runCompact()">Run Compaction</button>
                    <button class="btn" onclick="runRepair()">Run Repair</button>
                    <button class="btn btn-primary" onclick="createBackup()">Create Backup</button>
                </div>
            </div>
            
            <div class="card" style="margin-top: 20px;">
                <div class="card-header">
                    <span class="card-title">System Information</span>
                </div>
                <div class="stat-row">
                    <span class="stat-label">Node ID</span>
                    <span class="stat-value" id="node-id">-</span>
                </div>
                <div class="stat-row">
                    <span class="stat-label">Raft Term</span>
                    <span class="stat-value" id="raft-term">-</span>
                </div>
                <div class="stat-row">
                    <span class="stat-label">Leader</span>
                    <span class="stat-value" id="leader-id">-</span>
                </div>
                <div class="stat-row">
                    <span class="stat-label">Uptime</span>
                    <span class="stat-value" id="uptime">-</span>
                </div>
            </div>
        </div>
        
        <!-- API Keys Tab -->
        <div id="keys" class="tab-content">
            <div class="card">
                <div class="card-header">
                    <span class="card-title">API Keys</span>
                    <button class="btn btn-primary" onclick="showCreateKeyModal()">Create Key</button>
                </div>
                <table>
                    <thead>
                        <tr>
                            <th>Name</th>
                            <th>Tenant</th>
                            <th>Role</th>
                            <th>Status</th>
                            <th>Created</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody id="keys-table">
                        <tr>
                            <td colspan="6" style="text-align: center; color: var(--text-secondary);">
                                Loading...
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
        
        <!-- Backups Tab -->
        <div id="backups" class="tab-content">
            <div class="card">
                <div class="card-header">
                    <span class="card-title">Backups</span>
                    <div>
                        <button class="btn" onclick="createBackup('incremental')">Incremental Backup</button>
                        <button class="btn btn-primary" onclick="createBackup('full')">Full Backup</button>
                    </div>
                </div>
                <table>
                    <thead>
                        <tr>
                            <th>ID</th>
                            <th>Type</th>
                            <th>Status</th>
                            <th>Size</th>
                            <th>Keys</th>
                            <th>Created</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody id="backups-table">
                        <tr>
                            <td colspan="7" style="text-align: center; color: var(--text-secondary);">
                                No backups found
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
        
        <!-- Replication Tab -->
        <div id="replication" class="tab-content">
            <div class="card">
                <div class="card-header">
                    <span class="card-title">Cross-Datacenter Replication</span>
                </div>
                <div id="replication-status">
                    <div class="alert alert-info">
                        Cross-datacenter replication is available. Configure remote datacenters in config.toml.
                    </div>
                </div>
                <table>
                    <thead>
                        <tr>
                            <th>Datacenter</th>
                            <th>Status</th>
                            <th>Lag</th>
                            <th>Pending Events</th>
                            <th>Last Sync</th>
                        </tr>
                    </thead>
                    <tbody id="replication-table">
                        <tr>
                            <td colspan="5" style="text-align: center; color: var(--text-secondary);">
                                No remote datacenters configured
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
        
        <!-- Plugins Tab -->
        <div id="plugins" class="tab-content">
            <div class="card">
                <div class="card-header">
                    <span class="card-title">Plugins</span>
                </div>
                <table>
                    <thead>
                        <tr>
                            <th>Name</th>
                            <th>Type</th>
                            <th>Version</th>
                            <th>Status</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody id="plugins-table">
                        <tr>
                            <td colspan="5" style="text-align: center; color: var(--text-secondary);">
                                No plugins installed
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
        
        <footer>
            minikv v0.8.0 &mdash; A production-grade distributed KV store
        </footer>
    </div>
    
    <script>
        // Tab switching
        document.querySelectorAll('.nav-tab').forEach(tab => {
            tab.addEventListener('click', () => {
                document.querySelectorAll('.nav-tab').forEach(t => t.classList.remove('active'));
                document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
                tab.classList.add('active');
                document.getElementById(tab.dataset.tab).classList.add('active');
            });
        });
        
        // Format bytes
        function formatBytes(bytes) {
            if (bytes === 0) return '0 B';
            const k = 1024;
            const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
            const i = Math.floor(Math.log(bytes) / Math.log(k));
            return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
        }
        
        // Format date
        function formatDate(dateStr) {
            if (!dateStr) return '-';
            const date = new Date(dateStr);
            return date.toLocaleDateString() + ' ' + date.toLocaleTimeString();
        }
        
        // Fetch cluster status
        async function refreshStatus() {
            try {
                const response = await fetch('/admin/status');
                const data = await response.json();
                
                document.getElementById('cluster-role').textContent = data.role || 'Unknown';
                document.getElementById('volume-count').textContent = data.volumes?.length || 0;
                document.getElementById('object-count').textContent = data.s3_object_count || 0;
                document.getElementById('node-id').textContent = data.node_id || '-';
                document.getElementById('raft-term').textContent = data.raft_term || '-';
                document.getElementById('leader-id').textContent = data.leader || '-';
                
                const statusIndicator = document.getElementById('cluster-status');
                statusIndicator.className = 'status-indicator ' + 
                    (data.role === 'Leader' ? 'status-healthy' : 'status-warning');
            } catch (error) {
                console.error('Failed to fetch status:', error);
            }
        }
        
        // Fetch API keys
        async function refreshKeys() {
            try {
                const response = await fetch('/admin/keys');
                const data = await response.json();
                
                const tbody = document.getElementById('keys-table');
                if (data.keys && data.keys.length > 0) {
                    tbody.innerHTML = data.keys.map(key => `
                        <tr>
                            <td>${key.name}</td>
                            <td>${key.tenant}</td>
                            <td>${key.role}</td>
                            <td>
                                <span class="status-indicator ${key.active ? 'status-healthy' : 'status-error'}"></span>
                                ${key.active ? 'Active' : 'Revoked'}
                            </td>
                            <td>${formatDate(key.created_at)}</td>
                            <td>
                                ${key.active ? 
                                    `<button class="btn btn-danger" onclick="revokeKey('${key.id}')">Revoke</button>` : 
                                    ''}
                            </td>
                        </tr>
                    `).join('');
                } else {
                    tbody.innerHTML = '<tr><td colspan="6" style="text-align: center;">No API keys found</td></tr>';
                }
            } catch (error) {
                console.error('Failed to fetch keys:', error);
            }
        }
        
        // Quick actions
        async function runVerify() {
            try {
                const response = await fetch('/admin/verify', { method: 'POST' });
                const data = await response.json();
                alert(JSON.stringify(data, null, 2));
            } catch (error) {
                alert('Verify failed: ' + error.message);
            }
        }
        
        async function runCompact() {
            try {
                const response = await fetch('/admin/compact', { method: 'POST' });
                const data = await response.json();
                alert(JSON.stringify(data, null, 2));
            } catch (error) {
                alert('Compact failed: ' + error.message);
            }
        }
        
        async function runRepair() {
            try {
                const response = await fetch('/admin/repair', { method: 'POST' });
                const data = await response.json();
                alert(JSON.stringify(data, null, 2));
            } catch (error) {
                alert('Repair failed: ' + error.message);
            }
        }
        
        async function createBackup(type = 'full') {
            try {
                const response = await fetch('/admin/backup', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ type: type })
                });
                const data = await response.json();
                alert('Backup started: ' + data.backup_id);
                refreshBackups();
            } catch (error) {
                alert('Backup failed: ' + error.message);
            }
        }
        
        async function revokeKey(keyId) {
            if (!confirm('Are you sure you want to revoke this API key?')) return;
            try {
                await fetch(`/admin/keys/${keyId}/revoke`, { method: 'POST' });
                refreshKeys();
            } catch (error) {
                alert('Revoke failed: ' + error.message);
            }
        }
        
        async function refreshBackups() {
            try {
                const response = await fetch('/admin/backups');
                const data = await response.json();
                
                const tbody = document.getElementById('backups-table');
                if (data.backups && data.backups.length > 0) {
                    tbody.innerHTML = data.backups.map(backup => `
                        <tr>
                            <td>${backup.id}</td>
                            <td>${backup.backup_type}</td>
                            <td>
                                <span class="status-indicator ${
                                    backup.status === 'completed' ? 'status-healthy' : 
                                    backup.status === 'failed' ? 'status-error' : 'status-warning'
                                }"></span>
                                ${backup.status}
                            </td>
                            <td>${formatBytes(backup.size_bytes)}</td>
                            <td>${backup.key_count}</td>
                            <td>${formatDate(backup.started_at)}</td>
                            <td>
                                <button class="btn" onclick="restoreBackup('${backup.id}')">Restore</button>
                                <button class="btn btn-danger" onclick="deleteBackup('${backup.id}')">Delete</button>
                            </td>
                        </tr>
                    `).join('');
                }
            } catch (error) {
                console.error('Failed to fetch backups:', error);
            }
        }
        
        // Initial load
        refreshStatus();
        refreshKeys();
        refreshBackups();
        
        // Auto-refresh every 30 seconds
        setInterval(refreshStatus, 30000);
    </script>
</body>
</html>
"#;

/// Serve the admin dashboard HTML
pub async fn admin_dashboard() -> impl IntoResponse {
    Html(DASHBOARD_HTML)
}

/// Create the admin UI router
pub fn create_admin_ui_router() -> Router {
    Router::new()
        .route("/", get(admin_dashboard))
        .route("/dashboard", get(admin_dashboard))
}

/// Admin dashboard API response types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardStats {
    pub cluster_status: String,
    pub role: String,
    pub volume_count: u32,
    pub object_count: u64,
    pub storage_bytes: u64,
    pub node_id: String,
    pub raft_term: u64,
    pub leader: Option<String>,
    pub uptime_secs: u64,
}

/// Backup list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupListResponse {
    pub backups: Vec<crate::common::backup::BackupManifest>,
}

/// Plugin list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginListResponse {
    pub plugins: Vec<PluginSummary>,
}

/// Plugin summary for UI
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub plugin_type: String,
    pub version: String,
    pub status: String,
}

/// Replication status response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplicationStatusResponse {
    pub enabled: bool,
    pub local_dc: String,
    pub remote_dcs: Vec<RemoteDCStatus>,
}

/// Remote datacenter status for UI
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoteDCStatus {
    pub id: String,
    pub name: String,
    pub healthy: bool,
    pub lag_secs: u64,
    pub pending_events: usize,
    pub last_sync: Option<String>,
}

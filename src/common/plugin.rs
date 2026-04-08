//! Plugin system for extending minikv with custom storage, auth, and hooks.

use crate::common::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PluginVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn is_compatible(&self, other: &PluginVersion) -> bool {
        self.major == other.major
    }
}

impl std::fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,

    pub name: String,

    pub description: String,

    pub version: PluginVersion,

    pub author: String,

    pub homepage: Option<String>,

    pub license: Option<String>,

    pub plugin_type: PluginType,

    pub required_version: PluginVersion,

    pub dependencies: Vec<PluginDependency>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Storage,
    Auth,
    Hook,
    Middleware,
    Endpoint,
    General,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    pub plugin_id: String,
    pub min_version: PluginVersion,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginState {
    Loaded,
    Initialized,
    Enabled,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfig {
    pub settings: HashMap<String, serde_json::Value>,
}

impl PluginConfig {
    pub fn new() -> Self {
        Self {
            settings: HashMap::new(),
        }
    }

    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.settings
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn set<T: Serialize>(&mut self, key: &str, value: T) {
        if let Ok(v) = serde_json::to_value(value) {
            self.settings.insert(key.to_string(), v);
        }
    }
}

pub struct PluginContext {
    pub config: PluginConfig,
    pub shared_data: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
    pub logger: PluginLogger,
}

impl PluginContext {
    pub fn new(config: PluginConfig) -> Self {
        Self {
            config,
            shared_data: Arc::new(RwLock::new(HashMap::new())),
            logger: PluginLogger::new("plugin"),
        }
    }

    pub async fn get_shared<T: 'static + Send + Sync + Clone>(&self, key: &str) -> Option<T> {
        self.shared_data
            .read()
            .await
            .get(key)
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
    }

    pub async fn set_shared<T: 'static + Send + Sync>(&self, key: &str, value: T) {
        self.shared_data
            .write()
            .await
            .insert(key.to_string(), Box::new(value));
    }
}

pub struct PluginLogger {
    prefix: String,
}

impl PluginLogger {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    pub fn info(&self, message: &str) {
        tracing::info!("[{}] {}", self.prefix, message);
    }

    pub fn warn(&self, message: &str) {
        tracing::warn!("[{}] {}", self.prefix, message);
    }

    pub fn error(&self, message: &str) {
        tracing::error!("[{}] {}", self.prefix, message);
    }

    pub fn debug(&self, message: &str) {
        tracing::debug!("[{}] {}", self.prefix, message);
    }
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn info(&self) -> &PluginInfo;

    async fn initialize(&mut self, ctx: &PluginContext) -> Result<()>;

    async fn enable(&mut self, ctx: &PluginContext) -> Result<()>;

    async fn disable(&mut self, ctx: &PluginContext) -> Result<()>;

    async fn shutdown(&mut self, ctx: &PluginContext) -> Result<()>;

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[async_trait]
pub trait StoragePlugin: Plugin {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<()>;

    async fn delete(&self, key: &str) -> Result<()>;

    async fn exists(&self, key: &str) -> Result<bool>;

    async fn list_keys(&self, prefix: Option<&str>) -> Result<Vec<String>>;

    async fn stats(&self) -> Result<StorageStats>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_keys: u64,
    pub total_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    pub delete_ops: u64,
}

#[async_trait]
pub trait AuthPlugin: Plugin {
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthResult>;

    async fn authorize(
        &self,
        identity: &AuthIdentity,
        action: &str,
        resource: &str,
    ) -> Result<bool>;

    async fn refresh_token(&self, token: &str) -> Result<Option<String>>;
}

#[derive(Debug, Clone)]
pub enum AuthCredentials {
    ApiKey(String),
    Basic { username: String, password: String },
    Bearer(String),
    Custom(HashMap<String, String>),
}

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub authenticated: bool,
    pub identity: Option<AuthIdentity>,
    pub error: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthIdentity {
    pub id: String,
    pub tenant: Option<String>,
    pub roles: Vec<String>,
    pub claims: HashMap<String, String>,
}

#[async_trait]
pub trait HookPlugin: Plugin {
    async fn before_put(&self, key: &str, value: &[u8]) -> Result<Option<Vec<u8>>> {
        let _ = (key, value);
        Ok(None) // Return None to use original value, Some to modify
    }

    async fn after_put(&self, key: &str, value: &[u8]) -> Result<()> {
        let _ = (key, value);
        Ok(())
    }

    async fn before_get(&self, key: &str) -> Result<()> {
        let _ = key;
        Ok(())
    }

    async fn after_get(&self, key: &str, value: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
        let _ = (key, value);
        Ok(None) // Return None to use original value, Some to modify
    }

    async fn before_delete(&self, key: &str) -> Result<bool> {
        let _ = key;
        Ok(true) // Return false to prevent deletion
    }

    async fn after_delete(&self, key: &str) -> Result<()> {
        let _ = key;
        Ok(())
    }
}

pub struct RegisteredPlugin {
    pub plugin: Box<dyn Plugin>,
    pub state: PluginState,
    pub context: PluginContext,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
    pub error: Option<String>,
}

pub struct PluginManager {
    plugins: Arc<RwLock<HashMap<String, RegisteredPlugin>>>,
    load_order: Arc<RwLock<Vec<String>>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            load_order: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn register(&self, mut plugin: Box<dyn Plugin>, config: PluginConfig) -> Result<()> {
        let info = plugin.info().clone();
        let id = info.id.clone();

        if self.plugins.read().await.contains_key(&id) {
            return Err(Error::Other(format!("Plugin {} is already registered", id)));
        }

        for dep in &info.dependencies {
            if !dep.optional && !self.plugins.read().await.contains_key(&dep.plugin_id) {
                return Err(Error::Other(format!(
                    "Plugin {} requires {} which is not loaded",
                    id, dep.plugin_id
                )));
            }
        }

        let context = PluginContext::new(config);

        plugin.initialize(&context).await?;

        let registered = RegisteredPlugin {
            plugin,
            state: PluginState::Initialized,
            context,
            loaded_at: chrono::Utc::now(),
            error: None,
        };

        self.plugins.write().await.insert(id.clone(), registered);
        self.load_order.write().await.push(id);

        Ok(())
    }

    pub async fn enable(&self, plugin_id: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        let registered = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| Error::Other(format!("Plugin {} not found", plugin_id)))?;

        if registered.state == PluginState::Enabled {
            return Ok(());
        }

        registered.plugin.enable(&registered.context).await?;
        registered.state = PluginState::Enabled;

        Ok(())
    }

    pub async fn disable(&self, plugin_id: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        let registered = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| Error::Other(format!("Plugin {} not found", plugin_id)))?;

        if registered.state != PluginState::Enabled {
            return Ok(());
        }

        registered.plugin.disable(&registered.context).await?;
        registered.state = PluginState::Disabled;

        Ok(())
    }

    pub async fn unregister(&self, plugin_id: &str) -> Result<()> {
        self.disable(plugin_id).await.ok();

        let mut plugins = self.plugins.write().await;
        if let Some(mut registered) = plugins.remove(plugin_id) {
            registered.plugin.shutdown(&registered.context).await?;
        }

        self.load_order.write().await.retain(|id| id != plugin_id);

        Ok(())
    }

    pub async fn get_info(&self, plugin_id: &str) -> Option<PluginInfo> {
        self.plugins
            .read()
            .await
            .get(plugin_id)
            .map(|p| p.plugin.info().clone())
    }

    pub async fn get_state(&self, plugin_id: &str) -> Option<PluginState> {
        self.plugins.read().await.get(plugin_id).map(|p| p.state)
    }

    pub async fn list_plugins(&self) -> Vec<(PluginInfo, PluginState)> {
        self.plugins
            .read()
            .await
            .values()
            .map(|p| (p.plugin.info().clone(), p.state))
            .collect()
    }

    pub async fn enable_all(&self) -> Result<()> {
        let order = self.load_order.read().await.clone();
        for plugin_id in order {
            self.enable(&plugin_id).await?;
        }
        Ok(())
    }

    pub async fn disable_all(&self) -> Result<()> {
        let mut order = self.load_order.read().await.clone();
        order.reverse();
        for plugin_id in order {
            self.disable(&plugin_id).await?;
        }
        Ok(())
    }

    pub async fn shutdown_all(&self) -> Result<()> {
        self.disable_all().await?;

        let mut order = self.load_order.read().await.clone();
        order.reverse();
        for plugin_id in order {
            self.unregister(&plugin_id).await?;
        }

        Ok(())
    }

    pub async fn health_check_all(&self) -> HashMap<String, bool> {
        let plugins = self.plugins.read().await;
        let mut results = HashMap::new();

        for (id, registered) in plugins.iter() {
            let healthy = registered.plugin.health_check().await.unwrap_or(false);
            results.insert(id.clone(), healthy);
        }

        results
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

pub static PLUGIN_MANAGER: once_cell::sync::Lazy<PluginManager> =
    once_cell::sync::Lazy::new(PluginManager::new);

pub fn get_plugin_manager() -> &'static PluginManager {
    &PLUGIN_MANAGER
}

pub struct LoggingHookPlugin {
    info: PluginInfo,
}

impl LoggingHookPlugin {
    pub fn new() -> Self {
        Self {
            info: PluginInfo {
                id: "builtin.logging-hook".to_string(),
                name: "Logging Hook".to_string(),
                description: "Logs all data operations".to_string(),
                version: PluginVersion::new(1, 0, 0),
                author: "minikv".to_string(),
                homepage: None,
                license: Some("MIT".to_string()),
                plugin_type: PluginType::Hook,
                required_version: PluginVersion::new(0, 8, 0),
                dependencies: vec![],
            },
        }
    }
}

impl Default for LoggingHookPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for LoggingHookPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    async fn initialize(&mut self, ctx: &PluginContext) -> Result<()> {
        ctx.logger.info("Logging hook plugin initialized");
        Ok(())
    }

    async fn enable(&mut self, ctx: &PluginContext) -> Result<()> {
        ctx.logger.info("Logging hook plugin enabled");
        Ok(())
    }

    async fn disable(&mut self, ctx: &PluginContext) -> Result<()> {
        ctx.logger.info("Logging hook plugin disabled");
        Ok(())
    }

    async fn shutdown(&mut self, ctx: &PluginContext) -> Result<()> {
        ctx.logger.info("Logging hook plugin shutdown");
        Ok(())
    }
}

#[async_trait]
impl HookPlugin for LoggingHookPlugin {
    async fn after_put(&self, key: &str, value: &[u8]) -> Result<()> {
        tracing::info!("PUT {} ({} bytes)", key, value.len());
        Ok(())
    }

    async fn after_get(&self, key: &str, value: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
        match value {
            Some(v) => tracing::info!("GET {} ({} bytes)", key, v.len()),
            None => tracing::info!("GET {} (not found)", key),
        }
        Ok(None)
    }

    async fn after_delete(&self, key: &str) -> Result<()> {
        tracing::info!("DELETE {}", key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_version_compatibility() {
        let v1 = PluginVersion::new(1, 0, 0);
        let v2 = PluginVersion::new(1, 2, 0);
        let v3 = PluginVersion::new(2, 0, 0);

        assert!(v1.is_compatible(&v2));
        assert!(!v1.is_compatible(&v3));
    }

    #[test]
    fn test_plugin_config() {
        let mut config = PluginConfig::new();
        config.set("timeout", 30);
        config.set("enabled", true);
        config.set("name", "test");

        assert_eq!(config.get::<i32>("timeout"), Some(30));
        assert_eq!(config.get::<bool>("enabled"), Some(true));
        assert_eq!(config.get::<String>("name"), Some("test".to_string()));
        assert_eq!(config.get::<i32>("missing"), None);
    }

    #[tokio::test]
    async fn test_plugin_manager() {
        let manager = PluginManager::new();

        let plugin = LoggingHookPlugin::new();
        let config = PluginConfig::new();

        manager.register(Box::new(plugin), config).await.unwrap();

        let plugins = manager.list_plugins().await;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].0.id, "builtin.logging-hook");
        assert_eq!(plugins[0].1, PluginState::Initialized);

        manager.enable("builtin.logging-hook").await.unwrap();
        assert_eq!(
            manager.get_state("builtin.logging-hook").await,
            Some(PluginState::Enabled)
        );

        manager.disable("builtin.logging-hook").await.unwrap();
        assert_eq!(
            manager.get_state("builtin.logging-hook").await,
            Some(PluginState::Disabled)
        );
    }
}

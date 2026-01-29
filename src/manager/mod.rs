use crate::core::{CompareResult, RouteInfo, SaveEntry, SaveResult, SaveStatus};
use crate::error::{Result, SaveError};
use crate::git::Git2Core;
use std::path::Path;

pub struct SaveManager {
    core: Git2Core,
}

impl SaveManager {
    pub fn new(core: Git2Core) -> Self {
        Self { core }
    }

    pub fn into_core(self) -> Git2Core {
        self.core
    }

    pub fn save(&mut self, message: &str) -> Result<SaveResult> {
        if message.is_empty() {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            return self.core.commit(&timestamp);
        }
        self.core.commit(message)
    }

    pub fn load(&mut self, target: &str, preview: bool) -> Result<()> {
        let status = self.core.get_status()?;
        if status.has_uncommitted_changes && !preview {
            return Err(SaveError::UncommittedChanges);
        }
        if preview {
            return Ok(());
        }
        self.core.checkout(target)?;
        Ok(())
    }

    pub fn list_saves(&self) -> Result<Vec<SaveEntry>> {
        self.core.get_history()
    }

    pub fn get_status(&self) -> Result<SaveStatus> {
        self.core.get_status()
    }

    pub fn get_history(&self) -> Result<Vec<SaveEntry>> {
        self.core.get_history()
    }

    pub fn compare(&self, save1: &str, save2: &str) -> Result<CompareResult> {
        self.core.compare_saves(save1, save2)
    }

    pub fn list_tags(&self) -> Result<Vec<String>> {
        self.core.list_tags()
    }

    pub fn create_tag(&self, name: &str, message: &str) -> Result<()> {
        self.core.create_tag(name, message)
    }

    pub fn delete_tag(&self, name: &str) -> Result<()> {
        self.core.delete_tag(name)
    }

    pub fn load_by_tag(&mut self, tag_name: &str, force: bool) -> Result<()> {
        let status = self.core.get_status()?;
        if status.has_uncommitted_changes && !force {
            return Err(SaveError::UncommittedChanges);
        }
        if !force && status.has_uncommitted_changes {
            return Err(SaveError::UncommittedChanges);
        }
        if status.has_uncommitted_changes && !force {
            return Err(SaveError::UncommittedChanges);
        }
        self.core.checkout_by_tag(tag_name)?;
        Ok(())
    }

    pub fn should_auto_save(&self) -> bool {
        let config = ConfigManager::new(self.core.workdir()).load_auto_save_config();
        if !config.enabled {
            return false;
        }

        let now = chrono::Local::now().timestamp();
        if let Some(last_save) = config.last_save_time {
            if now - last_save >= config.interval as i64 {
                return true;
            }
        } else {
            return true;
        }
        false
    }

    pub fn update_last_save_time(&self) {
        let mut config = ConfigManager::new(self.core.workdir()).load_auto_save_config();
        config.last_save_time = Some(chrono::Local::now().timestamp());
        let _ = ConfigManager::new(self.core.workdir()).save_auto_save_config(&config);
    }
}

pub struct RouteManager {
    core: Git2Core,
}

impl RouteManager {
    pub fn new(core: Git2Core) -> Self {
        Self { core }
    }

    pub fn into_core(self) -> Git2Core {
        self.core
    }

    pub fn list_routes(&self) -> Result<Vec<RouteInfo>> {
        self.core.list_routes()
    }

    pub fn create_route(&mut self, name: &str) -> Result<()> {
        self.core.create_route(name)
    }

    pub fn switch_route(&mut self, name: &str) -> Result<()> {
        self.core.switch_route(name)
    }

    pub fn switch_create_route(&mut self, name: &str) -> Result<()> {
        self.core.switch_create_route(name)
    }

    pub fn delete_route(&mut self, name: &str) -> Result<()> {
        self.core.delete_route(name)
    }

    pub fn rename_route(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.core.rename_route(old_name, new_name)
    }

    pub fn get_current_route(&self) -> Result<String> {
        let routes = self.core.list_routes()?;
        Ok(routes
            .into_iter()
            .find(|r| r.is_current)
            .map(|r| r.name)
            .unwrap_or_else(|| "unknown".to_string()))
    }
}

pub struct ConfigManager {
    config_path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct AutoSaveConfig {
    pub enabled: bool,
    pub interval: u64,
    pub max_count: u32,
    pub last_save_time: Option<i64>,
}

impl Default for AutoSaveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: 300,
            max_count: 10,
            last_save_time: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IgnorePatterns {
    pub patterns: Vec<String>,
}

impl Default for IgnorePatterns {
    fn default() -> Self {
        Self {
            patterns: vec![
                "*.tmp".to_string(),
                "*.bak".to_string(),
                "*.backup".to_string(),
                "**/temp/**".to_string(),
                "**/cache/**".to_string(),
            ],
        }
    }
}

impl ConfigManager {
    pub fn new(save_dir: &Path) -> Self {
        let config_path = save_dir.join("gitsave.toml");
        Self { config_path }
    }

    pub fn load(&self) -> Result<toml::Value> {
        if !self.config_path.exists() {
            return Ok(toml::Value::Table(toml::Table::new()));
        }
        let content = std::fs::read_to_string(&self.config_path)
            .map_err(|e| SaveError::Config(e.to_string()))?;
        toml::from_str(&content).map_err(|e| SaveError::Config(e.to_string()))
    }

    pub fn save(&self, config: &toml::Value) -> Result<()> {
        let content =
            toml::to_string_pretty(config).map_err(|e| SaveError::Config(e.to_string()))?;
        std::fs::write(&self.config_path, content).map_err(|e| SaveError::Config(e.to_string()))
    }

    pub fn load_auto_save_config(&self) -> AutoSaveConfig {
        let config = self
            .load()
            .unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));

        let auto_save = config.get("auto_save").and_then(|v| v.as_table()).cloned();
        let enabled = auto_save
            .as_ref()
            .and_then(|t| t.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let interval = auto_save
            .as_ref()
            .and_then(|t| t.get("interval"))
            .and_then(|v| v.as_integer())
            .unwrap_or(300) as u64;
        let max_count = auto_save
            .as_ref()
            .and_then(|t| t.get("max_count"))
            .and_then(|v| v.as_integer())
            .unwrap_or(10) as u32;

        AutoSaveConfig {
            enabled,
            interval,
            max_count,
            last_save_time: None,
        }
    }

    pub fn save_auto_save_config(&self, config: &AutoSaveConfig) -> Result<()> {
        let mut toml_config = self
            .load()
            .unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));

        let mut auto_save_table = toml::Table::new();
        auto_save_table.insert("enabled".to_string(), toml::Value::Boolean(config.enabled));
        auto_save_table.insert(
            "interval".to_string(),
            toml::Value::Integer(config.interval as i64),
        );
        auto_save_table.insert(
            "max_count".to_string(),
            toml::Value::Integer(config.max_count as i64),
        );

        if let toml::Value::Table(ref mut table) = toml_config {
            table.insert("auto_save".to_string(), toml::Value::Table(auto_save_table));
        }

        self.save(&toml_config)
    }

    pub fn load_ignore_patterns(&self) -> IgnorePatterns {
        let config = self
            .load()
            .unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));

        let ignore = config.get("ignore").and_then(|v| v.as_table()).cloned();
        let patterns = ignore
            .as_ref()
            .and_then(|t| t.get("patterns"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    "*.tmp".to_string(),
                    "*.bak".to_string(),
                    "*.backup".to_string(),
                    "**/temp/**".to_string(),
                    "**/cache/**".to_string(),
                ]
            });

        IgnorePatterns { patterns }
    }

    pub fn save_ignore_patterns(&self, patterns: &IgnorePatterns) -> Result<()> {
        let mut toml_config = self
            .load()
            .unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));

        let patterns_array: toml::Value = toml::Value::Array(
            patterns
                .patterns
                .iter()
                .map(|s| toml::Value::String(s.clone()))
                .collect(),
        );

        let mut ignore_table = toml::Table::new();
        ignore_table.insert("patterns".to_string(), patterns_array);

        if let toml::Value::Table(ref mut table) = toml_config {
            table.insert("ignore".to_string(), toml::Value::Table(ignore_table));
        }

        self.save(&toml_config)
    }
}

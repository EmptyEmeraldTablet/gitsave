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
}

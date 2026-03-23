use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_RECENT_PATHS: usize = 10;

#[derive(Debug, Serialize, Deserialize, Default)]
struct RecentPathsFile {
    paths: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct RecentPathEntry {
    pub path: String,
    pub last_used: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RecentPathsFileV2 {
    entries: Vec<RecentPathEntry>,
}

pub struct RecentPathCache {
    path: PathBuf,
}

impl RecentPathCache {
    pub fn new() -> Self {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        let path = base.join("gitsave").join("recent_paths.toml");
        Self { path }
    }

    pub fn load_entries(&self) -> Vec<RecentPathEntry> {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(_) => return Vec::new(),
        };

        if let Ok(v2) = toml::from_str::<RecentPathsFileV2>(&content) {
            return v2.entries;
        }

        toml::from_str::<RecentPathsFile>(&content)
            .unwrap_or_default()
            .paths
            .into_iter()
            .map(|path| RecentPathEntry {
                path,
                last_used: 0,
            })
            .collect()
    }

    pub fn load_paths(&self) -> Vec<PathBuf> {
        self.load_entries()
            .into_iter()
            .map(|entry| PathBuf::from(entry.path))
            .collect()
    }

    pub fn add_path(&self, path: &Path) {
        let mut entries = self.load_entries();
        entries.retain(|entry| PathBuf::from(&entry.path) != path);
        entries.insert(
            0,
            RecentPathEntry {
                path: path.to_string_lossy().to_string(),
                last_used: Local::now().timestamp(),
            },
        );
        if entries.len() > MAX_RECENT_PATHS {
            entries.truncate(MAX_RECENT_PATHS);
        }
        let file = RecentPathsFileV2 { entries };
        let content = match toml::to_string_pretty(&file) {
            Ok(content) => content,
            Err(_) => return,
        };
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.path, content);
    }
}

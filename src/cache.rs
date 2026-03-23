use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_RECENT_PATHS: usize = 10;

#[derive(Debug, Serialize, Deserialize, Default)]
struct RecentPathsFile {
    paths: Vec<String>,
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

    pub fn load_paths(&self) -> Vec<PathBuf> {
        match fs::read_to_string(&self.path) {
            Ok(content) => toml::from_str::<RecentPathsFile>(&content)
                .unwrap_or_default()
                .paths
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn add_path(&self, path: &Path) {
        let mut paths = self.load_paths();
        paths.retain(|p| p != path);
        paths.insert(0, path.to_path_buf());
        if paths.len() > MAX_RECENT_PATHS {
            paths.truncate(MAX_RECENT_PATHS);
        }
        let file = RecentPathsFile {
            paths: paths
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
        };
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

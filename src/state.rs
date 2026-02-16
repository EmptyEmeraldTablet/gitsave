use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
struct ForkState {
    pending_base: Option<String>,
    counters: HashMap<String, u32>,
}

pub struct ForkStateManager {
    path: PathBuf,
}

impl ForkStateManager {
    pub fn new(workdir: &Path) -> Self {
        let path = workdir.join(".git").join("gitsave_state.toml");
        Self { path }
    }

    fn load(&self) -> ForkState {
        if let Ok(content) = fs::read_to_string(&self.path) {
            toml::from_str(&content).unwrap_or_default()
        } else {
            ForkState::default()
        }
    }

    fn save(&self, state: &ForkState) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(state) {
            let _ = fs::write(&self.path, content);
        }
    }

    pub fn set_pending_base(&self, base: Option<String>) {
        let mut state = self.load();
        state.pending_base = base;
        self.save(&state);
    }

    pub fn take_pending_base(&self) -> Option<String> {
        let mut state = self.load();
        let base = state.pending_base.take();
        self.save(&state);
        base
    }

    pub fn next_branch_name(&self, base: &str) -> String {
        let mut state = self.load();
        let counter = state
            .counters
            .entry(base.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
        let ts = Local::now().format("%Y%m%d-%H%M%S");
        let name = format!("gitsave/{}/{}-{:03}", base, ts, counter);
        self.save(&state);
        name
    }
}

pub fn root_branch_name(branch: &str) -> String {
    if let Some(stripped) = branch.strip_prefix("gitsave/") {
        stripped.split('/').next().unwrap_or(branch).to_string()
    } else {
        branch.to_string()
    }
}

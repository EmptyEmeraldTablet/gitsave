use chrono::{DateTime, Utc};
#[derive(Debug, Clone)]
pub struct SaveEntry {
    pub id: String,
    pub short_id: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub route: String,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub name: String,
    pub is_current: bool,
    pub latest_save: Option<SaveEntry>,
    pub save_count: usize,
}

#[derive(Debug, Clone)]
pub struct SaveStatus {
    pub current_route: String,
    pub last_save: Option<SaveEntry>,
    pub pending_changes: Vec<PendingChange>,
    pub has_uncommitted_changes: bool,
}

#[derive(Debug, Clone)]
pub struct PendingChange {
    pub path: String,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct SaveResult {
    pub oid: String,
    pub short_oid: String,
    pub message: String,
    pub changed_files: usize,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CompareResult {
    pub from: SaveEntry,
    pub to: SaveEntry,
    pub additions: usize,
    pub deletions: usize,
    pub changed_files: Vec<ChangedFile>,
}

#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
}

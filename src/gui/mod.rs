use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[cfg(feature = "gui")]
use rfd::AsyncFileDialog;

use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, vertical_space,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Size, Task, Theme, window};

use crate::cache::{RecentPathCache, RecentPathEntry};
use crate::core::{ChangeStatus, RouteInfo, SaveEntry, SaveStatus};
use crate::error::SaveError;
use crate::git::Git2Core;
use crate::manager::{RouteManager, SaveManager, is_recovery_branch_name};

// ─── Colour palette ──────────────────────────────────────────────────────────

const C_BG: Color = Color { r: 0.09, g: 0.09, b: 0.11, a: 1.0 };
const C_SURFACE: Color = Color { r: 0.14, g: 0.14, b: 0.18, a: 1.0 };
const C_BORDER: Color = Color { r: 0.25, g: 0.25, b: 0.35, a: 1.0 };
const C_ACCENT: Color = Color { r: 0.28, g: 0.62, b: 1.00, a: 1.0 };
const C_SUCCESS: Color = Color { r: 0.25, g: 0.78, b: 0.42, a: 1.0 };
const C_WARN: Color = Color { r: 0.95, g: 0.73, b: 0.25, a: 1.0 };
const C_ERROR: Color = Color { r: 0.92, g: 0.28, b: 0.25, a: 1.0 };
const C_TEXT: Color = Color { r: 0.85, g: 0.85, b: 0.90, a: 1.0 };
const C_DIM: Color = Color { r: 0.50, g: 0.50, b: 0.58, a: 1.0 };
const C_SEL: Color = Color { r: 0.28, g: 0.62, b: 1.00, a: 0.20 };
const C_RECOVERY: Color = Color { r: 0.92, g: 0.55, b: 0.10, a: 1.0 };
const MAX_INIT_PREVIEW_ITEMS: usize = 20;
const DEFAULT_COMPRESSION: i32 = 6;

// ─── Entry point ─────────────────────────────────────────────────────────────

pub fn run(save_dir: &Path) -> anyhow::Result<()> {
    let path = save_dir.to_path_buf();
    iced::application("Gitsave", GitsaveApp::update, GitsaveApp::view)
        .window(window::Settings {
            size: Size::new(980.0, 660.0),
            min_size: Some(Size::new(700.0, 460.0)),
            ..Default::default()
        })
        .theme(GitsaveApp::theme)
        .run_with(move || GitsaveApp::new(path.clone()))
        .map_err(|e| anyhow::anyhow!("GUI error: {e}"))
}

// ─── State ───────────────────────────────────────────────────────────────────

struct GitsaveApp {
    screen: Screen,
}

enum Screen {
    Picker(PickerState),
    Init(InitState),
    Main(MainState),
}

// ── Picker ───────────────────────────────────────────────────────────────────

struct PickerState {
    input: String,
    recent: Vec<RecentPathEntry>,
    error: Option<String>,
    manage: Option<PickerManageState>,
}

struct PickerManageState {
    target: PathBuf,
    last_used: Option<i64>,
    info: ManageInfo,
    export_name: String,
    cleanup_confirm: String,
    message: Option<String>,
}

struct ManageInfo {
    has_git: bool,
    has_gitsave: bool,
    repo_size: Option<u64>,
}

// ── Init ─────────────────────────────────────────────────────────────────────

struct InitState {
    dir: PathBuf,
    error: Option<String>,
    entries: Vec<DirEntryInfo>,
    entry_summary: String,
    mode: InitMode,
    author_name: String,
    author_email: String,
}

enum InitMode {
    Confirm,
    AuthorInput,
}

struct DirEntryInfo {
    name: String,
    is_dir: bool,
}

// ── Main ─────────────────────────────────────────────────────────────────────

struct MainState {
    dir: PathBuf,
    routes: Vec<RouteInfo>,
    history: Vec<SaveEntry>,
    all_history: Vec<SaveEntry>,
    status: Option<SaveStatus>,
    sel_route: usize,
    sel_hist: usize,
    save_msg: String,
    modal: Option<Modal>,
    notif: Option<Notif>,
    is_recovery: bool,
    busy: bool,
    route_history_ids: HashSet<String>,
    route_history_ready: bool,
}

impl MainState {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            routes: vec![],
            history: vec![],
            all_history: vec![],
            status: None,
            sel_route: 0,
            sel_hist: 0,
            save_msg: String::new(),
            modal: None,
            notif: None,
            is_recovery: false,
            busy: false,
            route_history_ids: HashSet::new(),
            route_history_ready: false,
        }
    }

    fn selected_route(&self) -> Option<&RouteInfo> {
        self.routes.get(self.sel_route)
    }

    fn selected_route_name(&self) -> Option<String> {
        self.selected_route().map(|r| r.name.clone())
    }

    fn selected_history_entry(&self) -> Option<&SaveEntry> {
        self.history.get(self.sel_hist)
    }

    fn is_dirty(&self) -> bool {
        self.status
            .as_ref()
            .map(|s| s.has_uncommitted_changes)
            .unwrap_or(false)
    }

    fn notify_ok(&mut self, msg: impl Into<String>) {
        self.notif = Some(Notif { text: msg.into(), kind: NotifKind::Ok });
    }

    fn notify_err(&mut self, msg: impl Into<String>) {
        self.notif = Some(Notif { text: msg.into(), kind: NotifKind::Err });
    }

    fn apply_history_filter(&mut self) {
        if self.is_recovery {
            return;
        }

        let filtered = if self.route_history_ready {
            self.all_history
                .iter()
                .filter(|entry| self.route_history_ids.contains(&entry.id))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            self.all_history.clone()
        };

        self.history = filtered;
        if self.history.is_empty() {
            self.sel_hist = 0;
            return;
        }
        if let Some(status) = &self.status {
            if let Some(current) = status.last_save.as_ref() {
                if let Some(idx) = self
                    .history
                    .iter()
                    .position(|entry| entry.short_id == current.short_id)
                {
                    self.sel_hist = idx;
                    return;
                }
            }
        }
        if self.sel_hist >= self.history.len() {
            self.sel_hist = self.history.len() - 1;
        }
    }
}

// ── Modal ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Modal {
    Confirm { prompt: String, action: ConfirmAction },
    TextInput { prompt: String, value: String, action: TextAction },
    ResolveDirty { prompt: String, action: PendingAction },
    ResolveUnstable { prompt: String, request: SaveRequest },
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    SwitchRoute { name: String },
    DiscardChanges,
}

#[derive(Debug, Clone)]
enum TextAction {
    RollbackNewRoute { target_id: String },
    CreateRoute,
    CreateSwitchRoute,
    RenameRoute { old_name: String },
    AmendMessage,
    RecoverRoute { old_name: String },
}

#[derive(Debug, Clone)]
enum PendingAction {
    RollbackSave { target_id: String, label: String },
    CreateRoute { name: String, switch: bool },
    SwitchRoute { name: String },
    RecoverRoute { old_name: String, new_name: String },
}

#[derive(Debug, Clone)]
struct SaveRequest {
    message: String,
    after: Option<PendingAction>,
}

#[derive(Debug, Clone)]
enum SaveOutcome {
    Saved(String),
    Unstable(u32),
    Failed(String),
}

#[derive(Debug, Clone)]
struct SaveResponse {
    outcome: SaveOutcome,
    request: SaveRequest,
}

// ── Notification ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Notif {
    text: String,
    kind: NotifKind,
}

#[derive(Debug, Clone, PartialEq)]
enum NotifKind {
    Ok,
    Err,
}

// ── Refresh payload ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RefreshData {
    routes: Vec<RouteInfo>,
    history: Vec<SaveEntry>,
    all_history: Vec<SaveEntry>,
    status: SaveStatus,
}

// ─── Messages ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    // Picker
    PickerInput(String),
    PickerSubmit,
    PickerBrowse,
    PickerBrowseResult(Option<PathBuf>),
    PickerOpenRecent(PathBuf),
    PickerSelectManage(PathBuf),
    PickerManageOpen,
    PickerManageExportNameChanged(String),
    PickerManageExport,
    PickerManageCleanupChanged(String),
    PickerManageCleanup,
    // Init
    InitYes,
    InitNo,
    InitDone(Result<InitOutcome, String>),
    InitAuthorNameChanged(String),
    InitAuthorEmailChanged(String),
    InitAuthorConfirm,
    InitAuthorSkip,
    InitAuthorDone(Result<(), String>),
    // Main – data
    Refresh,
    Refreshed(Result<RefreshData, String>),
    RouteHistoryLoaded { route: String, result: Result<HashSet<String>, String> },
    // Main – selection
    SelectRoute(usize),
    SelectHistory(usize),
    // Main – save
    SaveMsgChanged(String),
    TrySave,
    ForceSave,
    SaveDone(SaveResponse),
    // Main – rollback
    BeginRollback,
    RollbackDone(Result<(), String>),
    // Main – route management
    BeginCreateRoute,
    BeginCreateSwitchRoute,
    BeginSwitchRoute,
    BeginRenameRoute,
    BeginRecoverRoute,
    // Main – misc
    BeginDiscard,
    BeginAmend,
    ToggleRecovery,
    // Modal
    ModalInput(String),
    ModalOk,
    ModalCancel,
    ResolveDirtySave,
    ResolveDirtyDiscard,
    ResolveDirtyCancel,
    ResolveUnstableForce,
    ResolveUnstableRetry,
    ResolveUnstableCancel,
    // Generic action result
    ActionDone(Result<(), String>),
    DiscardThenActionDone(Result<(), String>, PendingAction),
    // Navigation
    BackToPicker,
}

#[derive(Debug, Clone)]
enum InitOutcome {
    Ready,
    NeedsAuthor,
}

// ─── App ─────────────────────────────────────────────────────────────────────

impl GitsaveApp {
    fn new(save_dir: PathBuf) -> (Self, Task<Message>) {
        let cache = RecentPathCache::new();
        let recent = cache.load_entries();
        let is_gitsave =
            Git2Core::open(&save_dir).is_ok() && save_dir.join("gitsave.toml").exists();

        if is_gitsave {
            cache.add_path(&save_dir);
            let dir = save_dir.clone();
            let app = Self { screen: Screen::Main(MainState::new(save_dir)) };
            let task =
                Task::perform(async move { do_refresh(dir, false) }, Message::Refreshed);
            (app, task)
        } else {
            let app = Self {
                screen: Screen::Picker(PickerState {
                    input: save_dir.to_string_lossy().to_string(),
                    recent,
                    error: None,
                    manage: None,
                }),
            };
            (app, Task::none())
        }
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Picker ──────────────────────────────────────────────────────
            Message::PickerInput(v) => {
                if let Screen::Picker(p) = &mut self.screen {
                    p.input = v;
                }
                Task::none()
            }

            Message::PickerSubmit => {
                let path = match &self.screen {
                    Screen::Picker(p) => PathBuf::from(p.input.trim()),
                    _ => return Task::none(),
                };
                self.open_path(path)
            }

            Message::PickerOpenRecent(path) => self.open_path(path),

            Message::PickerSelectManage(path) => {
                if let Screen::Picker(p) = &mut self.screen {
                    let path_label = path.to_string_lossy().to_string();
                    let last_used = p
                        .recent
                        .iter()
                        .find(|entry| entry.path == path_label)
                        .and_then(|entry| if entry.last_used > 0 { Some(entry.last_used) } else { None });
                    p.manage = Some(build_manage_state(path, last_used));
                    p.error = None;
                }
                Task::none()
            }

            Message::PickerManageOpen => {
                let path = match &self.screen {
                    Screen::Picker(p) => p.manage.as_ref().map(|m| m.target.clone()),
                    _ => None,
                };
                if let Some(path) = path {
                    self.open_path(path)
                } else {
                    Task::none()
                }
            }

            Message::PickerManageExportNameChanged(value) => {
                if let Screen::Picker(p) = &mut self.screen {
                    if let Some(m) = &mut p.manage {
                        m.export_name = value;
                    }
                }
                Task::none()
            }

            Message::PickerManageExport => {
                if let Screen::Picker(p) = &mut self.screen {
                    if let Some(m) = &mut p.manage {
                        m.message = None;
                        let export_dir = export_base_dir(&m.target);
                        match validate_export_name(&m.export_name) {
                            Ok(file_name) => {
                                let output = export_dir.join(file_name);
                                match export_archive(&m.target, &output) {
                                    Ok(()) => {
                                        m.message = Some("Export complete".to_string());
                                    }
                                    Err(err) => {
                                        m.message = Some(err);
                                    }
                                }
                            }
                            Err(err) => {
                                m.message = Some(err);
                            }
                        }
                    }
                }
                Task::none()
            }

            Message::PickerManageCleanupChanged(value) => {
                if let Screen::Picker(p) = &mut self.screen {
                    if let Some(m) = &mut p.manage {
                        m.cleanup_confirm = value;
                    }
                }
                Task::none()
            }

            Message::PickerManageCleanup => {
                if let Screen::Picker(p) = &mut self.screen {
                    if let Some(m) = &mut p.manage {
                        m.message = None;
                        let expected = m.target.display().to_string();
                        if !paths_match(&m.cleanup_confirm, &expected) {
                            m.message = Some("Path does not match".to_string());
                            return Task::none();
                        }
                        match cleanup_repo(&m.target) {
                            Ok(()) => {
                                m.message = Some("Cleanup complete".to_string());
                                m.cleanup_confirm.clear();
                                m.info = build_manage_info(&m.target);
                            }
                            Err(err) => {
                                m.message = Some(err);
                            }
                        }
                    }
                }
                Task::none()
            }

            Message::PickerBrowse => Task::perform(
                async {
                    AsyncFileDialog::new()
                        .set_title("Select Save Directory / 选择存档目录")
                        .pick_folder()
                        .await
                        .map(|h| h.path().to_path_buf())
                },
                Message::PickerBrowseResult,
            ),

            Message::PickerBrowseResult(maybe_path) => {
                if let (Screen::Picker(p), Some(path)) = (&mut self.screen, maybe_path) {
                    p.input = path.to_string_lossy().to_string();
                    p.error = None;
                }
                Task::none()
            }

            // ── Init ────────────────────────────────────────────────────────
            Message::InitYes => {
                let dir = match &self.screen {
                    Screen::Init(s) => s.dir.clone(),
                    _ => return Task::none(),
                };
                Task::perform(async move { init_repo_prepare(dir) }, Message::InitDone)
            }

            Message::InitNo => {
                self.to_picker();
                Task::none()
            }

            Message::InitDone(Ok(InitOutcome::Ready)) => {
                let dir = match &self.screen {
                    Screen::Init(s) => s.dir.clone(),
                    _ => return Task::none(),
                };
                self.enter_main(dir)
            }

            Message::InitDone(Ok(InitOutcome::NeedsAuthor)) => {
                if let Screen::Init(s) = &mut self.screen {
                    s.mode = InitMode::AuthorInput;
                    s.error = None;
                }
                Task::none()
            }

            Message::InitDone(Err(e)) => {
                if let Screen::Init(s) = &mut self.screen {
                    s.error = Some(e);
                }
                Task::none()
            }

            Message::InitAuthorNameChanged(value) => {
                if let Screen::Init(s) = &mut self.screen {
                    s.author_name = value;
                }
                Task::none()
            }

            Message::InitAuthorEmailChanged(value) => {
                if let Screen::Init(s) = &mut self.screen {
                    s.author_email = value;
                }
                Task::none()
            }

            Message::InitAuthorConfirm => {
                let (dir, name, email) = match &self.screen {
                    Screen::Init(s) => (
                        s.dir.clone(),
                        s.author_name.trim().to_string(),
                        s.author_email.trim().to_string(),
                    ),
                    _ => return Task::none(),
                };
                Task::perform(async move { init_repo_finalize(dir, &name, &email) }, Message::InitAuthorDone)
            }

            Message::InitAuthorSkip => {
                let dir = match &self.screen {
                    Screen::Init(s) => s.dir.clone(),
                    _ => return Task::none(),
                };
                Task::perform(async move { init_repo_finalize(dir, "", "") }, Message::InitAuthorDone)
            }

            Message::InitAuthorDone(Ok(())) => {
                let dir = match &self.screen {
                    Screen::Init(s) => s.dir.clone(),
                    _ => return Task::none(),
                };
                self.enter_main(dir)
            }

            Message::InitAuthorDone(Err(e)) => {
                if let Screen::Init(s) = &mut self.screen {
                    s.error = Some(e);
                }
                Task::none()
            }

            // ── Main – data ──────────────────────────────────────────────────
            Message::Refresh => self.trigger_refresh(),

            Message::Refreshed(result) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.busy = false;
                    match result {
                        Ok(data) => {
                            let prev_name =
                                s.routes.get(s.sel_route).map(|r| r.name.clone());
                            s.routes = data.routes;
                            s.all_history = data.all_history;
                            s.history = data.history;
                            s.status = Some(data.status);
                            s.sel_route = prev_name
                                .and_then(|n| s.routes.iter().position(|r| r.name == n))
                                .or_else(|| s.routes.iter().position(|r| r.is_current))
                                .unwrap_or(0);
                            s.route_history_ready = false;
                            s.route_history_ids.clear();
                            if s.sel_hist >= s.history.len() {
                                s.sel_hist = 0;
                            }
                            if !s.is_recovery {
                                if let Some(route) = s.selected_route_name() {
                                    return self.start_route_history_refresh(route);
                                }
                            }
                        }
                        Err(e) => s.notify_err(format!("Refresh error: {e}")),
                    }
                }
                Task::none()
            }

            Message::RouteHistoryLoaded { route, result } => {
                if let Screen::Main(s) = &mut self.screen {
                    if s.selected_route_name().as_deref() == Some(route.as_str()) {
                        match result {
                            Ok(ids) => {
                                s.route_history_ids = ids;
                                s.route_history_ready = true;
                            }
                            Err(err) => {
                                s.route_history_ready = false;
                                s.notify_err(format!("History filter error: {err}"));
                            }
                        }
                        s.apply_history_filter();
                    }
                }
                Task::none()
            }

            // ── Main – selection ─────────────────────────────────────────────
            Message::SelectRoute(i) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.sel_route = i;
                    s.sel_hist = 0;
                    s.route_history_ready = false;
                    s.route_history_ids.clear();
                    s.apply_history_filter();
                    if !s.is_recovery {
                        if let Some(route) = s.selected_route_name() {
                            return self.start_route_history_refresh(route);
                        }
                    }
                }
                Task::none()
            }

            Message::SelectHistory(i) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.sel_hist = i;
                }
                Task::none()
            }

            // ── Main – save ──────────────────────────────────────────────────
            Message::SaveMsgChanged(v) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.save_msg = v;
                }
                Task::none()
            }

            Message::TrySave => {
                let (dir, msg) = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.notif = None;
                        (s.dir.clone(), s.save_msg.trim().to_string())
                    }
                    _ => return Task::none(),
                };
                let request = SaveRequest { message: msg, after: None };
                Task::perform(
                    async move { do_save_request(dir, request, false) },
                    Message::SaveDone,
                )
            }

            Message::ForceSave => {
                let (dir, msg) = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.notif = None;
                        (s.dir.clone(), s.save_msg.trim().to_string())
                    }
                    _ => return Task::none(),
                };
                let request = SaveRequest { message: msg, after: None };
                Task::perform(
                    async move { do_save_request(dir, request, true) },
                    Message::SaveDone,
                )
            }

            Message::SaveDone(response) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.busy = false;
                    match response.outcome {
                        SaveOutcome::Saved(label) => {
                            s.notify_ok(format!("Saved: {label}"));
                            s.save_msg.clear();
                            if let Some(action) = response.request.after {
                                return self.execute_pending_action(action);
                            }
                        }
                        SaveOutcome::Unstable(attempts) => {
                            s.modal = Some(Modal::ResolveUnstable {
                                prompt: format!(
                                    "Save files still changing after {} checks. Force save?",
                                    attempts
                                ),
                                request: response.request,
                            });
                        }
                        SaveOutcome::Failed(e) => s.notify_err(format!("Save failed: {e}")),
                    }
                }
                self.trigger_refresh()
            }

            // ── Main – rollback ──────────────────────────────────────────────
            Message::BeginRollback => {
                if let Screen::Main(s) = &mut self.screen {
                    if let Some(entry) = s.selected_history_entry().cloned() {
                        if s.is_dirty() {
                            s.modal = Some(Modal::ResolveDirty {
                                prompt: format!(
                                    "Roll back to:\n  [{}] {}\n\nSave changes first?",
                                    entry.short_id, entry.message
                                ),
                                action: PendingAction::RollbackSave {
                                    target_id: entry.id,
                                    label: entry.message,
                                },
                            });
                        } else {
                            s.modal = Some(Modal::TextInput {
                                prompt: format!(
                                    "Roll back to:\n  [{}] {}\n\nEnter a name for the new route:",
                                    entry.short_id, entry.message
                                ),
                                value: String::new(),
                                action: TextAction::RollbackNewRoute { target_id: entry.id },
                            });
                        }
                    } else {
                        s.notify_err("No save selected");
                    }
                }
                Task::none()
            }

            Message::RollbackDone(result) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.busy = false;
                    match result {
                        Ok(()) => s.notify_ok("Rolled back successfully"),
                        Err(e) => s.notify_err(format!("Rollback failed: {e}")),
                    }
                }
                self.trigger_refresh()
            }

            // ── Main – route management ──────────────────────────────────────
            Message::BeginCreateRoute => {
                if let Screen::Main(s) = &mut self.screen {
                    s.modal = Some(Modal::TextInput {
                        prompt: "Create new route:".to_string(),
                        value: String::new(),
                        action: TextAction::CreateRoute,
                    });
                }
                Task::none()
            }

            Message::BeginCreateSwitchRoute => {
                if let Screen::Main(s) = &mut self.screen {
                    s.modal = Some(Modal::TextInput {
                        prompt: "Create and switch to new route:".to_string(),
                        value: String::new(),
                        action: TextAction::CreateSwitchRoute,
                    });
                }
                Task::none()
            }

            Message::BeginSwitchRoute => {
                let (dir, name, is_dirty) = match &self.screen {
                    Screen::Main(s) => match s.selected_route() {
                        Some(r) if r.is_current => {
                            if let Screen::Main(ms) = &mut self.screen {
                                ms.notify_err("Already on this route");
                            }
                            return Task::none();
                        }
                        Some(r) => (s.dir.clone(), r.name.clone(), s.is_dirty()),
                        None => return Task::none(),
                    },
                    _ => return Task::none(),
                };

                if is_dirty {
                    if let Screen::Main(s) = &mut self.screen {
                        s.modal = Some(Modal::ResolveDirty {
                            prompt: format!(
                                "Switch to route '{}' with unsaved changes?",
                                name
                            ),
                            action: PendingAction::SwitchRoute { name },
                        });
                    }
                    Task::none()
                } else {
                    if let Screen::Main(s) = &mut self.screen {
                        s.busy = true;
                    }
                    Task::perform(
                        async move { switch_route(dir, name) },
                        Message::ActionDone,
                    )
                }
            }

            Message::BeginRenameRoute => {
                if let Screen::Main(s) = &mut self.screen {
                    if let Some(r) = s.selected_route() {
                        let old = r.name.clone();
                        s.modal = Some(Modal::TextInput {
                            prompt: format!("Rename route '{old}':"),
                            value: old.clone(),
                            action: TextAction::RenameRoute { old_name: old },
                        });
                    }
                }
                Task::none()
            }

            Message::BeginRecoverRoute => {
                if let Screen::Main(s) = &mut self.screen {
                    if !s.is_recovery {
                        return Task::none();
                    }
                    if let Some(r) = s.selected_route() {
                        let short_hash = r.name.chars().take(7).collect::<String>();
                        let suggested = format!("recovery-{short_hash}");
                        s.modal = Some(Modal::TextInput {
                            prompt: format!(
                                "Recover snapshot\nEnter new route name (optional):\nDefault: {suggested}",
                            ),
                            value: String::new(),
                            action: TextAction::RecoverRoute { old_name: r.name.clone() },
                        });
                    } else {
                        s.notify_err("No recovery snapshot selected");
                    }
                }
                Task::none()
            }

            // ── Main – misc ──────────────────────────────────────────────────
            Message::BeginDiscard => {
                if let Screen::Main(s) = &mut self.screen {
                    s.modal = Some(Modal::Confirm {
                        prompt: "Discard ALL unsaved changes?\n\
                                 (A recovery snapshot will be created.)\n\
                                 This cannot be undone."
                            .to_string(),
                        action: ConfirmAction::DiscardChanges,
                    });
                }
                Task::none()
            }

            Message::BeginAmend => {
                if let Screen::Main(s) = &mut self.screen {
                    let current = s
                        .history
                        .first()
                        .map(|e| e.message.clone())
                        .unwrap_or_default();
                    s.modal = Some(Modal::TextInput {
                        prompt: "Edit latest save description:".to_string(),
                        value: current,
                        action: TextAction::AmendMessage,
                    });
                }
                Task::none()
            }

            Message::ToggleRecovery => {
                if let Screen::Main(s) = &mut self.screen {
                    s.is_recovery = !s.is_recovery;
                    s.sel_route = 0;
                    s.sel_hist = 0;
                    s.route_history_ready = false;
                    s.route_history_ids.clear();
                }
                self.trigger_refresh()
            }

            // ── Modal ────────────────────────────────────────────────────────
            Message::ModalInput(v) => {
                if let Screen::Main(s) = &mut self.screen {
                    if let Some(Modal::TextInput { value, .. }) = &mut s.modal {
                        *value = v;
                    }
                }
                Task::none()
            }

            Message::ModalOk => {
                let modal = match &mut self.screen {
                    Screen::Main(s) => s.modal.take(),
                    _ => None,
                };
                if let Some(m) = modal {
                    self.execute_modal(m)
                } else {
                    Task::none()
                }
            }

            Message::ModalCancel => {
                if let Screen::Main(s) = &mut self.screen {
                    s.modal = None;
                }
                Task::none()
            }

            Message::ResolveDirtySave => {
                let action = match &mut self.screen {
                    Screen::Main(s) => match s.modal.take() {
                        Some(Modal::ResolveDirty { action, .. }) => action,
                        other => {
                            s.modal = other;
                            return Task::none();
                        }
                    },
                    _ => return Task::none(),
                };
                let dir = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.notif = None;
                        s.dir.clone()
                    }
                    _ => return Task::none(),
                };
                let request = SaveRequest {
                    message: guard_message_for_action(&action),
                    after: Some(action),
                };
                Task::perform(
                    async move { do_save_request(dir, request, false) },
                    Message::SaveDone,
                )
            }

            Message::ResolveDirtyDiscard => {
                let action = match &mut self.screen {
                    Screen::Main(s) => match s.modal.take() {
                        Some(Modal::ResolveDirty { action, .. }) => action,
                        other => {
                            s.modal = other;
                            return Task::none();
                        }
                    },
                    _ => return Task::none(),
                };
                let dir = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.dir.clone()
                    }
                    _ => return Task::none(),
                };
                let action_for_task = action.clone();
                let action_for_msg = action.clone();
                Task::perform(
                    async move { discard_then_action(dir, action_for_task) },
                    move |result| Message::DiscardThenActionDone(result, action_for_msg.clone()),
                )
            }

            Message::ResolveDirtyCancel => {
                if let Screen::Main(s) = &mut self.screen {
                    s.modal = None;
                }
                Task::none()
            }

            Message::ResolveUnstableForce => {
                let request = match &mut self.screen {
                    Screen::Main(s) => match s.modal.take() {
                        Some(Modal::ResolveUnstable { request, .. }) => request,
                        other => {
                            s.modal = other;
                            return Task::none();
                        }
                    },
                    _ => return Task::none(),
                };
                let dir = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.notif = None;
                        s.dir.clone()
                    }
                    _ => return Task::none(),
                };
                Task::perform(
                    async move { do_save_request(dir, request, true) },
                    Message::SaveDone,
                )
            }

            Message::ResolveUnstableRetry => {
                let request = match &mut self.screen {
                    Screen::Main(s) => match s.modal.take() {
                        Some(Modal::ResolveUnstable { request, .. }) => request,
                        other => {
                            s.modal = other;
                            return Task::none();
                        }
                    },
                    _ => return Task::none(),
                };
                let dir = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.notif = None;
                        s.dir.clone()
                    }
                    _ => return Task::none(),
                };
                Task::perform(
                    async move { do_save_request(dir, request, false) },
                    Message::SaveDone,
                )
            }

            Message::ResolveUnstableCancel => {
                if let Screen::Main(s) = &mut self.screen {
                    s.modal = None;
                    s.notify_err("Save cancelled");
                }
                Task::none()
            }

            Message::ActionDone(result) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.busy = false;
                    match result {
                        Ok(()) => s.notify_ok("Done"),
                        Err(e) => s.notify_err(e),
                    }
                }
                self.trigger_refresh()
            }

            Message::DiscardThenActionDone(result, action) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.busy = false;
                    match result {
                        Ok(()) => {
                            return self.execute_pending_action(action);
                        }
                        Err(e) => s.notify_err(format!("Discard failed: {e}")),
                    }
                }
                self.trigger_refresh()
            }

            // ── Navigation ───────────────────────────────────────────────────
            Message::BackToPicker => {
                self.to_picker();
                Task::none()
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn open_path(&mut self, path: PathBuf) -> Task<Message> {
        if !path.exists() {
            if let Screen::Picker(p) = &mut self.screen {
                p.error = Some(format!("Path does not exist: {}", path.display()));
            }
            return Task::none();
        }
        let is_gitsave =
            Git2Core::open(&path).is_ok() && path.join("gitsave.toml").exists();
        if is_gitsave {
            self.enter_main(path)
        } else {
            self.screen = Screen::Init(build_init_state(path));
            Task::none()
        }
    }

    fn enter_main(&mut self, dir: PathBuf) -> Task<Message> {
        RecentPathCache::new().add_path(&dir);
        let d = dir.clone();
        self.screen = Screen::Main(MainState::new(dir));
        Task::perform(async move { do_refresh(d, false) }, Message::Refreshed)
    }

    fn to_picker(&mut self) {
        let recent = RecentPathCache::new().load_entries();
        let cwd = std::env::current_dir().unwrap_or_default();
        self.screen = Screen::Picker(PickerState {
            input: cwd.to_string_lossy().to_string(),
            recent,
            error: None,
            manage: None,
        });
    }

    fn trigger_refresh(&mut self) -> Task<Message> {
        if let Screen::Main(s) = &mut self.screen {
            let dir = s.dir.clone();
            let recovery = s.is_recovery;
            s.busy = true;
            Task::perform(
                async move { do_refresh(dir, recovery) },
                Message::Refreshed,
            )
        } else {
            Task::none()
        }
    }

    fn start_route_history_refresh(&mut self, route: String) -> Task<Message> {
        let dir = match &self.screen {
            Screen::Main(s) => s.dir.clone(),
            _ => return Task::none(),
        };
        let route_clone = route.clone();
        let route_for_msg = route.clone();
        Task::perform(
            async move { load_route_history_ids(dir, route_clone) },
            move |result| Message::RouteHistoryLoaded { route: route_for_msg.clone(), result },
        )
    }

    fn execute_pending_action(&mut self, action: PendingAction) -> Task<Message> {
        match action {
            PendingAction::RollbackSave { target_id, label } => {
                if let Screen::Main(s) = &mut self.screen {
                    s.modal = Some(Modal::TextInput {
                        prompt: format!(
                            "Roll back to:\n  [{}] {}\n\nEnter a name for the new route:",
                            target_id.chars().take(7).collect::<String>(),
                            label
                        ),
                        value: String::new(),
                        action: TextAction::RollbackNewRoute { target_id },
                    });
                }
                self.trigger_refresh()
            }
            PendingAction::CreateRoute { name, switch } => {
                let dir = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.dir.clone()
                    }
                    _ => return Task::none(),
                };
                if switch {
                    Task::perform(
                        async move { create_switch_route(dir, name) },
                        Message::ActionDone,
                    )
                } else {
                    Task::perform(
                        async move { create_route(dir, name) },
                        Message::ActionDone,
                    )
                }
            }
            PendingAction::SwitchRoute { name } => {
                let dir = match &mut self.screen {
                    Screen::Main(s) => {
                        s.busy = true;
                        s.dir.clone()
                    }
                    _ => return Task::none(),
                };
                Task::perform(
                    async move { switch_route(dir, name) },
                    Message::ActionDone,
                )
            }
            PendingAction::RecoverRoute { old_name, new_name } => {
                let dir = match &mut self.screen {
                    Screen::Main(s) => {
                        s.is_recovery = false;
                        s.busy = true;
                        s.dir.clone()
                    }
                    _ => return Task::none(),
                };
                Task::perform(
                    async move { recover_route(dir, old_name, new_name) },
                    Message::ActionDone,
                )
            }
        }
    }

    fn execute_modal(&mut self, modal: Modal) -> Task<Message> {
        match modal {
            Modal::Confirm { action, .. } => match action {
                ConfirmAction::SwitchRoute { name } => {
                    let dir = match &mut self.screen {
                        Screen::Main(s) => {
                            s.busy = true;
                            s.dir.clone()
                        }
                        _ => return Task::none(),
                    };
                    Task::perform(
                        async move { switch_route(dir, name) },
                        Message::ActionDone,
                    )
                }
                ConfirmAction::DiscardChanges => {
                    let dir = match &mut self.screen {
                        Screen::Main(s) => {
                            s.busy = true;
                            s.dir.clone()
                        }
                        _ => return Task::none(),
                    };
                    Task::perform(
                        async move { discard_changes(dir) },
                        Message::ActionDone,
                    )
                }
            },
            Modal::ResolveDirty { .. } | Modal::ResolveUnstable { .. } => Task::none(),
            Modal::TextInput { value, action, .. } => {
                let name = value.trim().to_string();
                match action {
                    TextAction::RollbackNewRoute { target_id } => {
                        if name.is_empty() {
                            if let Screen::Main(s) = &mut self.screen {
                                s.modal = Some(Modal::TextInput {
                                    prompt:
                                        "Route name cannot be empty.\n\
                                         Enter a name for the new route:"
                                            .to_string(),
                                    value: String::new(),
                                    action: TextAction::RollbackNewRoute { target_id },
                                });
                            }
                            return Task::none();
                        }
                        let dir = match &mut self.screen {
                            Screen::Main(s) => {
                                s.busy = true;
                                s.dir.clone()
                            }
                            _ => return Task::none(),
                        };
                        Task::perform(
                            async move { rollback_to_new_route(dir, target_id, name) },
                            Message::RollbackDone,
                        )
                    }
                    TextAction::CreateRoute => {
                        if name.is_empty() {
                            if let Screen::Main(s) = &mut self.screen {
                                s.modal = Some(Modal::TextInput {
                                    prompt: "Route name cannot be empty:".to_string(),
                                    value: String::new(),
                                    action: TextAction::CreateRoute,
                                });
                            }
                            return Task::none();
                        }
                        if let Screen::Main(s) = &mut self.screen {
                            if s.is_dirty() {
                                s.modal = Some(Modal::ResolveDirty {
                                    prompt: format!("Create new route '{name}'?"),
                                    action: PendingAction::CreateRoute {
                                        name,
                                        switch: false,
                                    },
                                });
                                return Task::none();
                            }
                        }
                        let dir = match &mut self.screen {
                            Screen::Main(s) => {
                                s.busy = true;
                                s.dir.clone()
                            }
                            _ => return Task::none(),
                        };
                        Task::perform(
                            async move { create_route(dir, name) },
                            Message::ActionDone,
                        )
                    }
                    TextAction::CreateSwitchRoute => {
                        if name.is_empty() {
                            if let Screen::Main(s) = &mut self.screen {
                                s.modal = Some(Modal::TextInput {
                                    prompt: "Route name cannot be empty:".to_string(),
                                    value: String::new(),
                                    action: TextAction::CreateSwitchRoute,
                                });
                            }
                            return Task::none();
                        }
                        if let Screen::Main(s) = &mut self.screen {
                            if s.is_dirty() {
                                s.modal = Some(Modal::ResolveDirty {
                                    prompt: format!("Create and switch to route '{name}'?"),
                                    action: PendingAction::CreateRoute {
                                        name,
                                        switch: true,
                                    },
                                });
                                return Task::none();
                            }
                        }
                        let dir = match &mut self.screen {
                            Screen::Main(s) => {
                                s.busy = true;
                                s.dir.clone()
                            }
                            _ => return Task::none(),
                        };
                        Task::perform(
                            async move { create_switch_route(dir, name) },
                            Message::ActionDone,
                        )
                    }
                    TextAction::RenameRoute { old_name } => {
                        if name.is_empty() || name == old_name {
                            return Task::none();
                        }
                        let dir = match &mut self.screen {
                            Screen::Main(s) => {
                                s.busy = true;
                                s.dir.clone()
                            }
                            _ => return Task::none(),
                        };
                        Task::perform(
                            async move { rename_route(dir, old_name, name) },
                            Message::ActionDone,
                        )
                    }
                    TextAction::AmendMessage => {
                        if name.is_empty() {
                            return Task::none();
                        }
                        let dir = match &mut self.screen {
                            Screen::Main(s) => {
                                s.busy = true;
                                s.dir.clone()
                            }
                            _ => return Task::none(),
                        };
                        Task::perform(
                            async move { amend_message(dir, name) },
                            Message::ActionDone,
                        )
                    }
                    TextAction::RecoverRoute { old_name } => {
                        let short_hash = old_name.chars().take(7).collect::<String>();
                        let fallback = format!("recovery-{short_hash}");
                        let new_name = if name.is_empty() { fallback } else { name };
                        let dir = match &mut self.screen {
                            Screen::Main(s) => {
                                if !is_valid_route_name(&new_name) {
                                    s.notify_err("Invalid route name. Use letters, digits, '-', '_', '/'.");
                                    return Task::none();
                                }
                                if let Ok(core) = Git2Core::open(&s.dir) {
                                    if let Ok(routes) = core.list_routes() {
                                        if routes.iter().any(|route| route.name == new_name) {
                                            s.notify_err(format!("Route '{}' already exists.", new_name));
                                            return Task::none();
                                        }
                                    }
                                }
                                if s.is_dirty() {
                                    s.modal = Some(Modal::ResolveDirty {
                                        prompt: format!(
                                            "Recover snapshot and switch to route '{new_name}'?",
                                        ),
                                        action: PendingAction::RecoverRoute {
                                            old_name,
                                            new_name,
                                        },
                                    });
                                    return Task::none();
                                }
                                s.busy = true;
                                s.dir.clone()
                            }
                            _ => return Task::none(),
                        };
                        Task::perform(
                            async move { recover_route(dir, old_name, new_name) },
                            Message::ActionDone,
                        )
                    }
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        match &self.screen {
            Screen::Picker(s) => view_picker(s),
            Screen::Init(s) => view_init(s),
            Screen::Main(s) => view_main(s),
        }
    }
}

// ─── Screen views ────────────────────────────────────────────────────────────

fn view_picker(s: &PickerState) -> Element<Message> {
    let input_row = row![
        text_input("Path to save directory…", &s.input)
            .on_input(Message::PickerInput)
            .on_submit(Message::PickerSubmit)
            .padding([8, 10])
            .size(14)
            .width(Length::Fill),
        horizontal_space().width(8),
        button(text(" Browse… ").size(14))
            .on_press(Message::PickerBrowse)
            .style(style_btn_secondary)
            .padding([8, 12]),
        horizontal_space().width(4),
        button(text("  Open  ").size(14))
            .on_press(Message::PickerSubmit)
            .style(style_btn_primary)
            .padding([8, 18]),
    ]
    .align_y(Alignment::Center);

    let recent_items: Vec<Element<Message>> = if s.recent.is_empty() {
        vec![text("No recent paths").size(13).color(C_DIM).into()]
    } else {
        s.recent
            .iter()
            .map(|p| {
                let path = PathBuf::from(&p.path);
                let last_used = format_last_used(p.last_used);
                let label = if last_used.is_empty() {
                    p.path.clone()
                } else {
                    format!("{} · last used {}", p.path, last_used)
                };
                row![
                    button(text(label).size(13))
                        .on_press(Message::PickerOpenRecent(path.clone()))
                        .style(style_btn_ghost)
                        .padding([5, 10])
                        .width(Length::Fill),
                    horizontal_space().width(6),
                    button(text("Manage").size(12).color(C_DIM))
                        .on_press(Message::PickerSelectManage(path))
                        .style(style_btn_ghost)
                        .padding([4, 8]),
                ]
                .align_y(Alignment::Center)
                .into()
            })
            .collect()
    };

    let error_elem: Element<Message> = match &s.error {
        Some(e) => text(e.as_str()).size(13).color(C_ERROR).into(),
        None => vertical_space().height(0).into(),
    };

    let manage_panel: Element<Message> = match &s.manage {
        Some(m) => view_picker_manage_panel(m),
        None => vertical_space().height(0).into(),
    };

    let card = container(
        column![
            text("Gitsave — Open Save Directory").size(20).color(C_TEXT),
            vertical_space().height(20),
            input_row,
            vertical_space().height(6),
            error_elem,
            vertical_space().height(20),
            horizontal_rule(1),
            vertical_space().height(14),
            text("Recent paths:").size(12).color(C_DIM),
            vertical_space().height(6),
            column(recent_items).spacing(2),
            vertical_space().height(18),
            manage_panel,
        ]
        .spacing(0)
        .padding(32)
        .width(Length::Fill),
    )
    .width(580)
    .style(style_card);

    center_widget(card.into())
}

fn view_init(s: &InitState) -> Element<Message> {
    let error_elem: Element<Message> = match &s.error {
        Some(e) => text(e.as_str()).size(13).color(C_ERROR).into(),
        None => vertical_space().height(0).into(),
    };

    let preview_lines: Vec<Element<Message>> = if s.entries.is_empty() {
        vec![text("(empty directory)").size(12).color(C_DIM).into()]
    } else {
        s.entries
            .iter()
            .take(MAX_INIT_PREVIEW_ITEMS)
            .map(|entry| {
                let prefix = if entry.is_dir { "[D]" } else { "[F]" };
                text(format!("{prefix} {}", entry.name)).size(12).color(C_DIM).into()
            })
            .collect()
    };

    let preview_more: Element<Message> = if s.entries.len() > MAX_INIT_PREVIEW_ITEMS {
        text(format!(
            "… and {} more",
            s.entries.len() - MAX_INIT_PREVIEW_ITEMS
        ))
        .size(12)
        .color(C_DIM)
        .into()
    } else {
        vertical_space().height(0).into()
    };

    let author_block: Element<Message> = match s.mode {
        InitMode::AuthorInput => column![
            vertical_space().height(12),
            text("Git user not configured. Enter author info (optional).")
                .size(13)
                .color(C_DIM),
            vertical_space().height(8),
            text_input("Author name", &s.author_name)
                .on_input(Message::InitAuthorNameChanged)
                .padding([8, 10])
                .size(13)
                .width(Length::Fill),
            vertical_space().height(6),
            text_input("Author email", &s.author_email)
                .on_input(Message::InitAuthorEmailChanged)
                .padding([8, 10])
                .size(13)
                .width(Length::Fill),
        ]
        .spacing(0)
        .into(),
        InitMode::Confirm => vertical_space().height(0).into(),
    };

    let action_row: Element<Message> = match s.mode {
        InitMode::Confirm => row![
            button(text("  Initialize  ").size(14))
                .on_press(Message::InitYes)
                .style(style_btn_primary)
                .padding([9, 20]),
            horizontal_space().width(12),
            button(text("  Cancel  ").size(14))
                .on_press(Message::InitNo)
                .style(style_btn_secondary)
                .padding([9, 20]),
        ]
        .align_y(Alignment::Center)
        .into(),
        InitMode::AuthorInput => row![
            button(text("  Confirm  ").size(14))
                .on_press(Message::InitAuthorConfirm)
                .style(style_btn_primary)
                .padding([9, 20]),
            horizontal_space().width(12),
            button(text("  Skip  ").size(14))
                .on_press(Message::InitAuthorSkip)
                .style(style_btn_secondary)
                .padding([9, 20]),
        ]
        .align_y(Alignment::Center)
        .into(),
    };

    let card = container(
        column![
            text("Initialize Gitsave Repository?").size(20).color(C_TEXT),
            vertical_space().height(12),
            text(s.dir.display().to_string()).size(14).color(C_ACCENT),
            vertical_space().height(12),
            text(
                "This directory does not contain a gitsave repository.\n\
                 Review the contents before initializing."
            )
            .size(14)
            .color(C_DIM),
            vertical_space().height(10),
            text(s.entry_summary.as_str()).size(12).color(C_DIM),
            vertical_space().height(8),
            container(column(preview_lines).spacing(2))
                .style(style_panel_right)
                .padding([8, 10])
                .width(Length::Fill),
            preview_more,
            author_block,
            vertical_space().height(18),
            error_elem,
            action_row,
        ]
        .spacing(0)
        .padding(32),
    )
    .width(560)
    .style(style_card);

    center_widget(card.into())
}

fn view_picker_manage_panel(m: &PickerManageState) -> Element<Message> {
    let path_label = m.target.display().to_string();
    let last_used = m
        .last_used
        .map(format_last_used)
        .unwrap_or_else(|| "Unknown".to_string());
    let repo_size = m
        .info
        .repo_size
        .map(format_bytes)
        .unwrap_or_else(|| "Unknown".to_string());
    let repo_status = if m.info.has_git { "Git repo detected" } else { "No Git repo" };
    let gitsave_status = if m.info.has_git && !m.info.has_gitsave {
        Some("Warning: missing gitsave.toml")
    } else {
        None
    };

    let message_elem: Element<Message> = match &m.message {
        Some(msg) => {
            let style = if msg.to_lowercase().contains("error")
                || msg.to_lowercase().contains("fail")
            {
                C_ERROR
            } else {
                C_SUCCESS
            };
            text(msg.as_str()).size(12).color(style).into()
        }
        None => vertical_space().height(0).into(),
    };

    container(
        column![
            text("Manage Selected Path").size(12).color(C_DIM),
            vertical_space().height(6),
            text(path_label).size(12).color(C_TEXT),
            vertical_space().height(6),
            text(repo_status).size(12).color(C_DIM),
            gitsave_status
                .map(|warn| text(warn).size(12).color(C_WARN).into())
                .unwrap_or_else(|| vertical_space().height(0).into()),
            text(format!("Last used: {last_used}")).size(12).color(C_DIM),
            text(format!("Repo size: {repo_size}")).size(12).color(C_DIM),
            vertical_space().height(10),
            row![
                button(text("Open").size(12))
                    .on_press(Message::PickerManageOpen)
                    .style(style_btn_secondary)
                    .padding([5, 10]),
            ]
            .align_y(Alignment::Center),
            vertical_space().height(10),
            text("Export archive (includes .git)").size(12).color(C_DIM),
            vertical_space().height(4),
            text_input("Output file name", &m.export_name)
                .on_input(Message::PickerManageExportNameChanged)
                .padding([6, 8])
                .size(12)
                .width(Length::Fill),
            vertical_space().height(6),
            button(text("Export").size(12))
                .on_press(Message::PickerManageExport)
                .style(style_btn_secondary)
                .padding([5, 10]),
            vertical_space().height(10),
            text("Cleanup (removes .git, gitsave.toml, .gitattributes)")
                .size(12)
                .color(C_DIM),
            vertical_space().height(4),
            text_input("Type full path to confirm", &m.cleanup_confirm)
                .on_input(Message::PickerManageCleanupChanged)
                .padding([6, 8])
                .size(12)
                .width(Length::Fill),
            vertical_space().height(6),
            button(text("Cleanup").size(12).color(C_ERROR))
                .on_press(Message::PickerManageCleanup)
                .style(style_btn_secondary)
                .padding([5, 10]),
            vertical_space().height(8),
            message_elem,
        ]
        .spacing(0)
        .padding(16)
        .width(Length::Fill),
    )
    .style(style_panel_left)
    .width(Length::Fill)
    .into()
}

fn view_main(s: &MainState) -> Element<Message> {
    if let Some(modal) = &s.modal {
        return view_modal(modal);
    }

    column![
        view_header(s),
        view_body(s),
        view_save_bar(s),
        horizontal_rule(1),
        view_status_bar(s),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

// ── Header ────────────────────────────────────────────────────────────────────

fn view_header(s: &MainState) -> Element<Message> {
    let current_name = s
        .routes
        .iter()
        .find(|r| r.is_current)
        .map(|r| r.name.as_str())
        .unwrap_or("—");

    let recovery_badge: Element<Message> = if s.is_recovery {
        text("  [RECOVERY MODE]  ").size(12).color(C_RECOVERY).into()
    } else {
        horizontal_space().width(0).into()
    };

    let busy_badge: Element<Message> = if s.busy {
        text("  ...  ").size(12).color(C_DIM).into()
    } else {
        horizontal_space().width(0).into()
    };

    container(
        row![
            text(s.dir.display().to_string()).size(12).color(C_DIM),
            text(format!("  ●  {current_name}")).size(13).color(C_ACCENT),
            recovery_badge,
            busy_badge,
            horizontal_space(),
            button(text("~").size(14).color(C_DIM))
                .on_press(Message::Refresh)
                .style(style_btn_ghost)
                .padding([4, 8]),
            horizontal_space().width(4),
            button(text("< Back").size(12).color(C_DIM))
                .on_press(Message::BackToPicker)
                .style(style_btn_ghost)
                .padding([4, 10]),
        ]
        .align_y(Alignment::Center)
        .padding([6, 12])
        .width(Length::Fill),
    )
    .style(style_header)
    .width(Length::Fill)
    .into()
}

// ── Body ─────────────────────────────────────────────────────────────────────

fn view_body(s: &MainState) -> Element<Message> {
    row![view_routes_panel(s), view_history_panel(s)]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ── Routes panel ─────────────────────────────────────────────────────────────

fn view_routes_panel(s: &MainState) -> Element<Message> {
    let route_items: Vec<Element<Message>> = if s.routes.is_empty() {
        vec![text("No routes found").size(13).color(C_DIM).into()]
    } else {
        s.routes
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let is_sel = i == s.sel_route;
                let is_rec = is_recovery_branch_name(&r.name);
                let color = if is_rec {
                    C_RECOVERY
                } else if r.is_current {
                    C_ACCENT
                } else {
                    C_TEXT
                };
                let prefix = if r.is_current { "● " } else { "  " };
                let label = if is_rec {
                    format!("{prefix}[recovery]")
                } else {
                    format!("{prefix}{}", r.name)
                };
                let count = format!(" {}", r.save_count);

                button(
                    row![
                        text(label).size(13).color(color).width(Length::Fill),
                        text(count).size(11).color(C_DIM),
                    ]
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SelectRoute(i))
                .style(move |_, _| style_item_btn(is_sel))
                .padding([6, 8])
                .width(Length::Fill)
                .into()
            })
            .collect()
    };

    let can_switch = s
        .routes
        .get(s.sel_route)
        .map(|r| !r.is_current)
        .unwrap_or(false)
        && !s.busy;

    let switch_btn = {
        let b = button(text("> Switch to").size(12))
            .style(style_btn_secondary)
            .padding([5, 10])
            .width(Length::Fill);
        if can_switch {
            b.on_press(Message::BeginSwitchRoute)
        } else {
            b
        }
    };

    let rename_btn = {
        let b = button(text("Rename").size(12))
            .style(style_btn_secondary)
            .padding([5, 10])
            .width(Length::Fill);
        if !s.routes.is_empty() && !s.busy {
            b.on_press(Message::BeginRenameRoute)
        } else {
            b
        }
    };

    let recover_btn: Element<Message> = if s.is_recovery {
        let enabled = !s.routes.is_empty() && !s.busy;
        let b = button(text("Recover Snapshot").size(12))
            .style(style_btn_secondary)
            .padding([5, 10])
            .width(Length::Fill);
        if enabled {
            b.on_press(Message::BeginRecoverRoute).into()
        } else {
            b.into()
        }
    } else {
        vertical_space().height(0).into()
    };

    let recovery_label =
        if s.is_recovery { "Exit Recovery" } else { "Recovery Mode" };
    let recovery_color = if s.is_recovery { C_WARN } else { C_DIM };

    let recovery_spacer: Element<Message> = if s.is_recovery {
        vertical_space().height(2).into()
    } else {
        vertical_space().height(0).into()
    };

    container(
        column![
            text("Routes").size(12).color(C_DIM),
            vertical_space().height(4),
            scrollable(column(route_items).spacing(1)).height(Length::Fill),
            vertical_space().height(8),
            horizontal_rule(1),
            vertical_space().height(6),
            button(text("+ Create").size(12))
                .on_press(Message::BeginCreateRoute)
                .style(style_btn_secondary)
                .padding([5, 10])
                .width(Length::Fill),
            vertical_space().height(2),
            button(text("+ Create & Switch").size(12))
                .on_press(Message::BeginCreateSwitchRoute)
                .style(style_btn_secondary)
                .padding([5, 10])
                .width(Length::Fill),
            vertical_space().height(2),
            switch_btn,
            vertical_space().height(2),
            rename_btn,
            recovery_spacer,
            recover_btn,
            vertical_space().height(8),
            button(text(recovery_label).size(12).color(recovery_color))
                .on_press(Message::ToggleRecovery)
                .style(style_btn_ghost)
                .padding([5, 10])
                .width(Length::Fill),
        ]
        .spacing(0)
        .padding([8, 8])
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .style(style_panel_left)
    .width(250)
    .height(Length::Fill)
    .into()
}

// ── History panel ─────────────────────────────────────────────────────────────

fn view_history_panel(s: &MainState) -> Element<Message> {
    let history_items: Vec<Element<Message>> = if s.history.is_empty() {
        vec![text("No save history").size(13).color(C_DIM).into()]
    } else {
        s.history
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let is_sel = i == s.sel_hist;
                let ts = e
                    .timestamp
                    .with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M")
                    .to_string();
                button(
                    row![
                        text(format!("[{}]", e.short_id))
                            .size(12)
                            .color(C_DIM)
                            .width(70),
                        text(e.message.as_str())
                            .size(13)
                            .color(C_TEXT)
                            .width(Length::Fill),
                        text(ts).size(11).color(C_DIM),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(8),
                )
                .on_press(Message::SelectHistory(i))
                .style(move |_, _| style_item_btn(is_sel))
                .padding([6, 10])
                .width(Length::Fill)
                .into()
            })
            .collect()
    };

    container(
        column![
            text("Save History").size(12).color(C_DIM),
            vertical_space().height(4),
            scrollable(column(history_items).spacing(1)).height(Length::Fill),
        ]
        .spacing(0)
        .padding([8, 8])
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .style(style_panel_right)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

// ── Save bar ──────────────────────────────────────────────────────────────────

fn view_save_bar(s: &MainState) -> Element<Message> {
    let placeholder = if s.busy { "Processing…" } else { "Save description (optional)…" };

    let input = {
        let ti = text_input(placeholder, &s.save_msg)
            .size(13)
            .padding([7, 10])
            .width(Length::Fill);
        if s.busy {
            ti
        } else {
            ti.on_input(Message::SaveMsgChanged).on_submit(Message::TrySave)
        }
    };

    let save_btn = {
        let b = button(text("Save").size(13))
            .style(style_btn_primary)
            .padding([7, 14]);
        if s.busy { b } else { b.on_press(Message::TrySave) }
    };

    let force_btn = {
        let b = button(text("Force Save").size(13))
            .style(style_btn_secondary)
            .padding([7, 14]);
        if s.busy { b } else { b.on_press(Message::ForceSave) }
    };

    let rollback_btn = {
        let enabled = !s.history.is_empty() && !s.busy;
        let b = button(text("Rollback").size(13))
            .style(if enabled { style_btn_secondary } else { style_btn_disabled })
            .padding([7, 14]);
        if enabled { b.on_press(Message::BeginRollback) } else { b }
    };

    let discard_btn = {
        let enabled = s.is_dirty() && !s.busy;
        let label_color = if enabled { C_ERROR } else { C_DIM };
        let b = button(text("Discard").size(13).color(label_color))
            .style(style_btn_ghost)
            .padding([7, 14]);
        if enabled { b.on_press(Message::BeginDiscard) } else { b }
    };

    let amend_btn = {
        let enabled = !s.history.is_empty() && !s.busy;
        let b = button(text("Amend").size(13).color(C_DIM))
            .style(style_btn_ghost)
            .padding([7, 14]);
        if enabled { b.on_press(Message::BeginAmend) } else { b }
    };

    let notif_elem: Element<Message> = match &s.notif {
        Some(n) => {
            let color = if n.kind == NotifKind::Ok { C_SUCCESS } else { C_ERROR };
            text(n.text.as_str()).size(12).color(color).into()
        }
        None => horizontal_space().width(0).into(),
    };

    container(
        column![
            row![input, horizontal_space().width(6), save_btn, horizontal_space().width(4), force_btn,]
                .align_y(Alignment::Center),
            vertical_space().height(6),
            row![
                rollback_btn,
                horizontal_space().width(4),
                discard_btn,
                horizontal_space().width(4),
                amend_btn,
                horizontal_space(),
                notif_elem,
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(0)
        .padding([10, 12])
        .width(Length::Fill),
    )
    .style(style_savebar)
    .width(Length::Fill)
    .into()
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn view_status_bar(s: &MainState) -> Element<Message> {
    let content: Element<Message> = match &s.status {
        None => text("Loading status…").size(12).color(C_DIM).into(),
        Some(status) if !status.has_uncommitted_changes => {
            text("Working directory clean").size(12).color(C_SUCCESS).into()
        }
        Some(status) => {
            let n = status.pending_changes.len();
            let file_rows: Vec<Element<Message>> = status
                .pending_changes
                .iter()
                .take(8)
                .map(|c| {
                    let sym = match c.status {
                        ChangeStatus::Added => "+",
                        ChangeStatus::Modified => "~",
                        ChangeStatus::Deleted => "-",
                    };
                    text(format!("  {sym}  {}", c.path))
                        .size(11)
                        .color(C_DIM)
                        .into()
                })
                .collect();

            let more: Element<Message> = if n > 8 {
                text(format!("  … {} more", n - 8)).size(11).color(C_DIM).into()
            } else {
                vertical_space().height(0).into()
            };

            column![
                text(format!("!  {n} uncommitted change(s)"))
                    .size(12)
                    .color(C_WARN),
                column(file_rows).spacing(2),
                more,
            ]
            .spacing(2)
            .into()
        }
    };

    container(content).padding([6, 12]).width(Length::Fill).into()
}

// ── Modal ─────────────────────────────────────────────────────────────────────

fn view_modal(modal: &Modal) -> Element<Message> {
    let card: Element<Message> = match modal {
        Modal::Confirm { prompt, .. } => container(
            column![
                text(prompt.as_str()).size(14).color(C_TEXT),
                vertical_space().height(20),
                row![
                    horizontal_space(),
                    button(text("  No  ").size(13))
                        .on_press(Message::ModalCancel)
                        .style(style_btn_secondary)
                        .padding([8, 18]),
                    horizontal_space().width(10),
                    button(text("  Yes  ").size(13))
                        .on_press(Message::ModalOk)
                        .style(style_btn_primary)
                        .padding([8, 18]),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(0)
            .padding(28),
        )
        .width(460)
        .style(style_card)
        .into(),

        Modal::TextInput { prompt, value, .. } => container(
            column![
                text(prompt.as_str()).size(14).color(C_TEXT),
                vertical_space().height(16),
                text_input("Enter value…", value.as_str())
                    .on_input(Message::ModalInput)
                    .on_submit(Message::ModalOk)
                    .padding([8, 10])
                    .size(14)
                    .width(Length::Fill),
                vertical_space().height(16),
                row![
                    horizontal_space(),
                    button(text("  Cancel  ").size(13))
                        .on_press(Message::ModalCancel)
                        .style(style_btn_secondary)
                        .padding([8, 18]),
                    horizontal_space().width(10),
                    button(text("  Confirm  ").size(13))
                        .on_press(Message::ModalOk)
                        .style(style_btn_primary)
                        .padding([8, 18]),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(0)
            .padding(28),
        )
        .width(480)
        .style(style_card)
        .into(),

        Modal::ResolveDirty { prompt, .. } => container(
            column![
                text(prompt.as_str()).size(14).color(C_TEXT),
                vertical_space().height(16),
                row![
                    horizontal_space(),
                    button(text(" Save First ").size(13))
                        .on_press(Message::ResolveDirtySave)
                        .style(style_btn_primary)
                        .padding([8, 14]),
                    horizontal_space().width(8),
                    button(text(" Discard ").size(13).color(C_ERROR))
                        .on_press(Message::ResolveDirtyDiscard)
                        .style(style_btn_secondary)
                        .padding([8, 14]),
                    horizontal_space().width(8),
                    button(text(" Cancel ").size(13))
                        .on_press(Message::ResolveDirtyCancel)
                        .style(style_btn_secondary)
                        .padding([8, 14]),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(0)
            .padding(28),
        )
        .width(520)
        .style(style_card)
        .into(),

        Modal::ResolveUnstable { prompt, .. } => container(
            column![
                text(prompt.as_str()).size(14).color(C_TEXT),
                vertical_space().height(16),
                row![
                    horizontal_space(),
                    button(text(" Force ").size(13))
                        .on_press(Message::ResolveUnstableForce)
                        .style(style_btn_primary)
                        .padding([8, 16]),
                    horizontal_space().width(8),
                    button(text(" Retry ").size(13))
                        .on_press(Message::ResolveUnstableRetry)
                        .style(style_btn_secondary)
                        .padding([8, 16]),
                    horizontal_space().width(8),
                    button(text(" Cancel ").size(13))
                        .on_press(Message::ResolveUnstableCancel)
                        .style(style_btn_secondary)
                        .padding([8, 16]),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(0)
            .padding(28),
        )
        .width(500)
        .style(style_card)
        .into(),
    };

    center_widget(card)
}

// ─── Layout helper ───────────────────────────────────────────────────────────

fn center_widget(widget: Element<Message>) -> Element<Message> {
    column![
        vertical_space(),
        row![horizontal_space(), widget, horizontal_space()].align_y(Alignment::Center),
        vertical_space(),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

// ─── Style functions ─────────────────────────────────────────────────────────

fn style_card(theme: &Theme) -> iced::widget::container::Style {
    let _ = theme;
    iced::widget::container::Style {
        background: Some(Background::Color(C_SURFACE)),
        border: Border { color: C_BORDER, width: 1.0, radius: 8.0.into() },
        ..Default::default()
    }
}

fn style_header(theme: &Theme) -> iced::widget::container::Style {
    let _ = theme;
    iced::widget::container::Style {
        background: Some(Background::Color(C_BG)),
        border: Border { color: C_BORDER, width: 0.0, radius: 0.0.into() },
        ..Default::default()
    }
}

fn style_panel_left(theme: &Theme) -> iced::widget::container::Style {
    let _ = theme;
    iced::widget::container::Style {
        background: Some(Background::Color(C_SURFACE)),
        border: Border { color: C_BORDER, width: 0.0, radius: 0.0.into() },
        ..Default::default()
    }
}

fn style_panel_right(theme: &Theme) -> iced::widget::container::Style {
    let _ = theme;
    iced::widget::container::Style {
        background: Some(Background::Color(C_BG)),
        border: Border { color: C_BORDER, width: 0.0, radius: 0.0.into() },
        ..Default::default()
    }
}

fn style_savebar(theme: &Theme) -> iced::widget::container::Style {
    let _ = theme;
    iced::widget::container::Style {
        background: Some(Background::Color(C_SURFACE)),
        border: Border { color: C_BORDER, width: 0.0, radius: 0.0.into() },
        ..Default::default()
    }
}

fn style_btn_primary(
    theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let _ = theme;
    let bg = match status {
        Status::Hovered => Color { r: 0.22, g: 0.52, b: 0.92, a: 1.0 },
        Status::Pressed => Color { r: 0.18, g: 0.45, b: 0.80, a: 1.0 },
        Status::Disabled => Color { r: 0.25, g: 0.25, b: 0.30, a: 1.0 },
        _ => Color { r: 0.15, g: 0.42, b: 0.82, a: 1.0 },
    };
    iced::widget::button::Style {
        background: Some(Background::Color(bg)),
        text_color: C_TEXT,
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 5.0.into() },
        ..Default::default()
    }
}

fn style_btn_secondary(
    theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let _ = theme;
    let bg = match status {
        Status::Hovered => Color { r: 0.28, g: 0.28, b: 0.38, a: 1.0 },
        Status::Pressed => Color { r: 0.20, g: 0.20, b: 0.28, a: 1.0 },
        Status::Disabled => Color { r: 0.18, g: 0.18, b: 0.22, a: 0.5 },
        _ => Color { r: 0.20, g: 0.20, b: 0.28, a: 1.0 },
    };
    iced::widget::button::Style {
        background: Some(Background::Color(bg)),
        text_color: C_TEXT,
        border: Border { color: C_BORDER, width: 1.0, radius: 5.0.into() },
        ..Default::default()
    }
}

fn style_btn_ghost(
    theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let _ = theme;
    let bg = match status {
        Status::Hovered => Color { r: 1.0, g: 1.0, b: 1.0, a: 0.07 },
        Status::Pressed => Color { r: 1.0, g: 1.0, b: 1.0, a: 0.12 },
        _ => Color::TRANSPARENT,
    };
    iced::widget::button::Style {
        background: Some(Background::Color(bg)),
        text_color: C_TEXT,
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 5.0.into() },
        ..Default::default()
    }
}

fn style_btn_disabled(
    theme: &Theme,
    _status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let _ = theme;
    iced::widget::button::Style {
        background: Some(Background::Color(Color { r: 0.18, g: 0.18, b: 0.22, a: 0.5 })),
        text_color: C_DIM,
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 5.0.into() },
        ..Default::default()
    }
}

fn style_item_btn(selected: bool) -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: Some(Background::Color(if selected {
            C_SEL
        } else {
            Color::TRANSPARENT
        })),
        text_color: C_TEXT,
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 4.0.into() },
        ..Default::default()
    }
}

// ─── Backend operations ───────────────────────────────────────────────────────

fn do_refresh(dir: PathBuf, is_recovery: bool) -> Result<RefreshData, String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    let save_mgr = SaveManager::new(core);
    let status = save_mgr.get_status().map_err(|e| e.to_string())?;
    let raw_history = save_mgr.get_history().map_err(|e| e.to_string())?;
    let route_mgr = RouteManager::new(save_mgr.into_core());
    let all_routes = route_mgr.list_routes().map_err(|e| e.to_string())?;

    let (routes, history, all_history) = if is_recovery {
        let rr: Vec<RouteInfo> = all_routes
            .into_iter()
            .filter(|r| is_recovery_branch_name(&r.name))
            .collect();
        let hist: Vec<SaveEntry> =
            rr.iter().filter_map(|r| r.latest_save.clone()).collect();
        let all_hist = hist.clone();
        (rr, hist, all_hist)
    } else {
        let normal: Vec<RouteInfo> = all_routes
            .into_iter()
            .filter(|r| !is_recovery_branch_name(&r.name))
            .collect();
        let all_hist = raw_history.clone();
        (normal, raw_history, all_hist)
    };

    Ok(RefreshData { routes, history, all_history, status })
}

fn do_save_request(dir: PathBuf, request: SaveRequest, force: bool) -> SaveResponse {
    let result = (|| {
        let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
        let mut mgr = SaveManager::new(core);
        let save_result = if force {
            mgr.save_force(&request.message)
        } else {
            mgr.save(&request.message)
        };
        match save_result {
            Ok(r) => Ok(SaveOutcome::Saved(format!("[{}] {}", r.short_oid, r.message))),
            Err(SaveError::UnstableSave { attempts }) => Ok(SaveOutcome::Unstable(attempts)),
            Err(e) => Ok(SaveOutcome::Failed(e.to_string())),
        }
    })();

    let outcome = result.unwrap_or_else(|err| SaveOutcome::Failed(err));
    SaveResponse { outcome, request }
}

fn rollback_to_new_route(
    dir: PathBuf,
    target_id: String,
    new_route: String,
) -> Result<(), String> {
    // Create and switch to a new branch from current HEAD
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    let mut route_mgr = RouteManager::new(core);
    route_mgr.switch_create_route(&new_route).map_err(|e| e.to_string())?;

    // Hard-reset the new branch to the target commit
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    let mut save_mgr = SaveManager::new(core);
    save_mgr.load(&target_id, false).map_err(|e| e.to_string())
}

fn switch_route(dir: PathBuf, name: String) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    RouteManager::new(core).switch_route(&name).map_err(|e| e.to_string())
}

fn create_route(dir: PathBuf, name: String) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    RouteManager::new(core).create_route(&name).map_err(|e| e.to_string())
}

fn create_switch_route(dir: PathBuf, name: String) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    RouteManager::new(core).switch_create_route(&name).map_err(|e| e.to_string())
}

fn rename_route(dir: PathBuf, old_name: String, new_name: String) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    RouteManager::new(core)
        .rename_route(&old_name, &new_name)
        .map_err(|e| e.to_string())
}

fn discard_changes(dir: PathBuf) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    SaveManager::new(core).discard_changes().map_err(|e| e.to_string())
}

fn amend_message(dir: PathBuf, message: String) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    SaveManager::new(core)
        .amend_head_message(&message)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn init_repo_prepare(dir: PathBuf) -> Result<InitOutcome, String> {
    if let Ok(existing) = Git2Core::open(&dir) {
        let config_path = existing.workdir().join("gitsave.toml");
        if config_path.exists() {
            return Err("gitsave already initialized here".to_string());
        }
        return Err("Git repository already exists; choose another folder".to_string());
    }

    let mut core = Git2Core::init(&dir).map_err(|e| e.to_string())?;
    core.set_core_compression(DEFAULT_COMPRESSION).map_err(|e| e.to_string())?;
    if core.repo().signature().is_err() {
        return Ok(InitOutcome::NeedsAuthor);
    }

    write_config_and_commit(&mut core, &dir, "", "").map_err(|e| e.to_string())?;
    Ok(InitOutcome::Ready)
}

fn init_repo_finalize(dir: PathBuf, author_name: &str, author_email: &str) -> Result<(), String> {
    let mut core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    write_config_and_commit(&mut core, &dir, author_name, author_email)
        .map_err(|e| e.to_string())
}

fn write_config_and_commit(
    core: &mut Git2Core,
    base_path: &Path,
    author_name: &str,
    author_email: &str,
) -> anyhow::Result<()> {
    core.set_core_compression(DEFAULT_COMPRESSION)
        .map_err(|err| anyhow::anyhow!("Failed to set core.compression: {}", err))?;
    let config_content = build_config_content(author_name, author_email);
    let config_path = base_path.join("gitsave.toml");
    fs::write(&config_path, config_content)
        .map_err(|err| anyhow::anyhow!("Failed to write config: {}", err))?;

    let attributes_path = base_path.join(".gitattributes");
    let attributes_content = "# Treat game saves as binary\nsaves/** -text -diff -merge\n";
    fs::write(&attributes_path, attributes_content)
        .map_err(|err| anyhow::anyhow!("Failed to write .gitattributes: {}", err))?;

    core.commit_files(&[config_path, attributes_path], "init gitsave config")?;
    Ok(())
}

fn build_config_content(author_name: &str, author_email: &str) -> String {
    let name = escape_toml_string(author_name);
    let email = escape_toml_string(author_email);
    format!(
        "# gitsave configuration\n[save]\nmax_history = 50\ncompression = {}\n\n[auto_save]\nenabled = false\n\n[author]\nname = \"{}\"\nemail = \"{}\"\n",
        DEFAULT_COMPRESSION, name, email
    )
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn load_route_history_ids(dir: PathBuf, route: String) -> Result<HashSet<String>, String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    core.get_history_ids_for_route(&route).map_err(|e| e.to_string())
}

fn discard_then_action(dir: PathBuf, _action: PendingAction) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    SaveManager::new(core)
        .discard_changes()
        .map_err(|e| e.to_string())
}

fn recover_route(dir: PathBuf, old_name: String, new_name: String) -> Result<(), String> {
    let mut core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    core.rename_route(&old_name, &new_name).map_err(|e| e.to_string())?;
    core.switch_route(&new_name).map_err(|e| e.to_string())
}

fn build_init_state(dir: PathBuf) -> InitState {
    let (entries, summary) = match load_dir_preview(&dir) {
        Ok((entries, summary)) => (entries, summary),
        Err(err) => (Vec::new(), format!("Preview error: {err}")),
    };
    InitState {
        dir,
        error: None,
        entries,
        entry_summary: summary,
        mode: InitMode::Confirm,
        author_name: String::new(),
        author_email: String::new(),
    }
}

fn load_dir_preview(path: &Path) -> std::result::Result<(Vec<DirEntryInfo>, String), String> {
    let mut entries = Vec::new();
    let mut dir_count = 0usize;
    let mut file_count = 0usize;

    let read_dir = fs::read_dir(path).map_err(|err| err.to_string())?;
    for entry in read_dir {
        let entry = entry.map_err(|err| err.to_string())?;
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = file_type.is_dir();
        if is_dir {
            dir_count += 1;
        } else {
            file_count += 1;
        }
        entries.push(DirEntryInfo { name, is_dir });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let summary = format!(
        "Dirs: {}  Files: {}  Total: {}",
        dir_count,
        file_count,
        dir_count + file_count
    );

    Ok((entries, summary))
}

fn build_manage_state(target: PathBuf, last_used: Option<i64>) -> PickerManageState {
    let info = build_manage_info(&target);
    let export_name = default_export_name(&target);
    PickerManageState {
        target,
        last_used,
        info,
        export_name,
        cleanup_confirm: String::new(),
        message: None,
    }
}

fn build_manage_info(path: &Path) -> ManageInfo {
    ManageInfo {
        has_git: has_git_dir(path),
        has_gitsave: has_gitsave_config(path),
        repo_size: repo_size_bytes(path),
    }
}

fn format_last_used(timestamp: i64) -> String {
    if timestamp <= 0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

fn has_git_dir(path: &Path) -> bool {
    let git_dir = path.join(".git");
    git_dir.exists() && git_dir.is_dir()
}

fn has_gitsave_config(path: &Path) -> bool {
    let config = path.join("gitsave.toml");
    config.exists() && config.is_file()
}

fn cleanup_repo(path: &Path) -> std::result::Result<(), String> {
    if !path.exists() || !path.is_dir() {
        return Err("Path is not a directory".to_string());
    }
    let git_dir = path.join(".git");
    if !git_dir.exists() {
        return Err("No .git directory found".to_string());
    }
    if !git_dir.is_dir() {
        return Err(".git exists but is not a directory".to_string());
    }
    fs::remove_dir_all(&git_dir).map_err(|err| err.to_string())?;

    let config_path = path.join("gitsave.toml");
    if config_path.is_file() {
        let content = fs::read_to_string(&config_path).map_err(|err| err.to_string())?;
        let first_line = content.lines().next().unwrap_or("");
        if first_line == "# gitsave configuration" {
            fs::remove_file(&config_path).map_err(|err| err.to_string())?;
        }
    }

    let attributes_path = path.join(".gitattributes");
    if attributes_path.is_file() {
        let content = fs::read_to_string(&attributes_path).map_err(|err| err.to_string())?;
        if content == "# Treat game saves as binary\nsaves/** -text -diff -merge\n" {
            fs::remove_file(&attributes_path).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

fn paths_match(input: &str, expected: &str) -> bool {
    let input_norm = normalize_path_string(input);
    let expected_norm = normalize_path_string(expected);

    if let (Some(input_path), Some(expected_path)) = (input_norm, expected_norm) {
        return paths_equal(&input_path, &expected_path);
    }

    false
}

fn normalize_path_string(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed);
    let path = PathBuf::from(unquoted);
    path.canonicalize().ok().or(Some(path))
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn repo_size_bytes(path: &Path) -> Option<u64> {
    let git_dir = path.join(".git");
    if !git_dir.exists() || !git_dir.is_dir() {
        return None;
    }
    Some(dir_size_bytes(&git_dir))
}

fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_dir() {
                total = total.saturating_add(dir_size_bytes(&path));
            } else {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= MB {
        format!("{:.2} MB", value / MB)
    } else if value >= KB {
        format!("{:.2} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn export_base_dir(path: &Path) -> PathBuf {
    path.parent().unwrap_or(path).to_path_buf()
}

fn default_export_name(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("gitsave_export");
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    format!("{}-{}.zip", name, timestamp)
}

fn validate_export_name(input: &str) -> std::result::Result<String, String> {
    let name = input.trim();
    if name.is_empty() {
        return Err("File name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') || name.contains(':') {
        return Err("Use a file name only (no path separators)".to_string());
    }
    Ok(name.to_string())
}

fn export_archive(source: &Path, output: &Path) -> std::result::Result<(), String> {
    if !source.exists() || !source.is_dir() {
        return Err("Source path is not a directory".to_string());
    }
    if output.exists() {
        return Err("Output file already exists".to_string());
    }
    if let Some(parent) = output.parent() {
        if !parent.exists() {
            return Err("Output directory does not exist".to_string());
        }
    }

    let file = fs::File::create(output).map_err(|err| err.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, source, source, options)?;
    zip.finish().map_err(|err| err.to_string())?;
    Ok(())
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    base: &Path,
    path: &Path,
    options: zip::write::FileOptions,
) -> std::result::Result<(), String> {
    let entries = fs::read_dir(path).map_err(|err| err.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let entry_path = entry.path();
        let metadata = entry.metadata().map_err(|err| err.to_string())?;
        if metadata.is_dir() {
            let rel = entry_path
                .strip_prefix(base)
                .map_err(|err| err.to_string())?;
            let name = format!("{}/", zip_path(rel));
            let _ = zip.add_directory(name, options);
            add_dir_to_zip(zip, base, &entry_path, options)?;
        } else if metadata.is_file() {
            let rel = entry_path
                .strip_prefix(base)
                .map_err(|err| err.to_string())?;
            let name = zip_path(rel);
            zip.start_file(name, options)
                .map_err(|err| err.to_string())?;
            let mut file = fs::File::open(&entry_path).map_err(|err| err.to_string())?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|err| err.to_string())?;
            zip.write_all(&buffer).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn zip_path(path: &Path) -> String {
    let mut out = String::new();
    for component in path.components() {
        if let std::path::Component::Normal(part) = component {
            if !out.is_empty() {
                out.push('/');
            }
            out.push_str(&part.to_string_lossy());
        }
    }
    out
}

fn guard_message_for_action(action: &PendingAction) -> String {
    let detail = match action {
        PendingAction::RollbackSave { target_id, .. } => {
            let short = target_id.chars().take(7).collect::<String>();
            format!("before rollback {}", short)
        }
        PendingAction::CreateRoute { name, switch } => {
            if *switch {
                format!("before create+switch route {}", name)
            } else {
                format!("before create route {}", name)
            }
        }
        PendingAction::SwitchRoute { name } => format!("before switch route {}", name),
        PendingAction::RecoverRoute { new_name, .. } => format!("before recover route {}", new_name),
    };
    format!("[guard] {}", detail)
}

fn is_valid_route_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '/')
}

fn is_valid_route_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_valid_route_char)
}

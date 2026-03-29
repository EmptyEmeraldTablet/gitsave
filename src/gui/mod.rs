use std::path::{Path, PathBuf};

#[cfg(feature = "gui")]
use rfd::AsyncFileDialog;

use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, vertical_space,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Size, Task, Theme, window};

use crate::cache::RecentPathCache;
use crate::core::{ChangeStatus, RouteInfo, SaveEntry, SaveStatus};
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
    recent: Vec<PathBuf>,
    error: Option<String>,
}

// ── Init ─────────────────────────────────────────────────────────────────────

struct InitState {
    dir: PathBuf,
    error: Option<String>,
}

// ── Main ─────────────────────────────────────────────────────────────────────

struct MainState {
    dir: PathBuf,
    routes: Vec<RouteInfo>,
    history: Vec<SaveEntry>,
    status: Option<SaveStatus>,
    sel_route: usize,
    sel_hist: usize,
    save_msg: String,
    modal: Option<Modal>,
    notif: Option<Notif>,
    is_recovery: bool,
    busy: bool,
}

impl MainState {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            routes: vec![],
            history: vec![],
            status: None,
            sel_route: 0,
            sel_hist: 0,
            save_msg: String::new(),
            modal: None,
            notif: None,
            is_recovery: false,
            busy: false,
        }
    }

    fn selected_route(&self) -> Option<&RouteInfo> {
        self.routes.get(self.sel_route)
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
}

// ── Modal ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Modal {
    Confirm { prompt: String, action: ConfirmAction },
    TextInput { prompt: String, value: String, action: TextAction },
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    SwitchRoute { name: String },
    DiscardChanges,
    DeleteRoute { name: String },
}

#[derive(Debug, Clone)]
enum TextAction {
    RollbackNewRoute { target_id: String },
    CreateRoute,
    CreateSwitchRoute,
    RenameRoute { old_name: String },
    AmendMessage,
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
    // Init
    InitYes,
    InitNo,
    InitDone(Result<(), String>),
    // Main – data
    Refresh,
    Refreshed(Result<RefreshData, String>),
    // Main – selection
    SelectRoute(usize),
    SelectHistory(usize),
    // Main – save
    SaveMsgChanged(String),
    TrySave,
    ForceSave,
    SaveDone(Result<String, String>),
    // Main – rollback
    BeginRollback,
    RollbackDone(Result<(), String>),
    // Main – route management
    BeginCreateRoute,
    BeginCreateSwitchRoute,
    BeginSwitchRoute,
    BeginRenameRoute,
    // Main – misc
    BeginDiscard,
    BeginAmend,
    BeginDeleteRoute,
    ToggleRecovery,
    // Modal
    ModalInput(String),
    ModalOk,
    ModalCancel,
    // Generic action result
    ActionDone(Result<(), String>),
    // Navigation
    BackToPicker,
}

// ─── App ─────────────────────────────────────────────────────────────────────

impl GitsaveApp {
    fn new(save_dir: PathBuf) -> (Self, Task<Message>) {
        let cache = RecentPathCache::new();
        let recent = cache.load_paths();
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

            Message::PickerBrowse => Task::perform(
                async {
                    AsyncFileDialog::new()
                        .set_title("选择游戏存档目录")
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
                Task::perform(async move { init_repo(dir) }, Message::InitDone)
            }

            Message::InitNo => {
                self.to_picker();
                Task::none()
            }

            Message::InitDone(Ok(())) => {
                let dir = match &self.screen {
                    Screen::Init(s) => s.dir.clone(),
                    _ => return Task::none(),
                };
                self.enter_main(dir)
            }

            Message::InitDone(Err(e)) => {
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
                            s.history = data.history;
                            s.status = Some(data.status);
                            s.sel_route = prev_name
                                .and_then(|n| s.routes.iter().position(|r| r.name == n))
                                .or_else(|| s.routes.iter().position(|r| r.is_current))
                                .unwrap_or(0);
                            if s.sel_hist >= s.history.len() {
                                s.sel_hist = 0;
                            }
                        }
                        Err(e) => s.notify_err(format!("Refresh error: {e}")),
                    }
                }
                Task::none()
            }

            // ── Main – selection ─────────────────────────────────────────────
            Message::SelectRoute(i) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.sel_route = i;
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
                Task::perform(async move { do_save(dir, msg, false) }, Message::SaveDone)
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
                Task::perform(async move { do_save(dir, msg, true) }, Message::SaveDone)
            }

            Message::SaveDone(result) => {
                if let Screen::Main(s) = &mut self.screen {
                    s.busy = false;
                    match result {
                        Ok(label) => {
                            s.notify_ok(format!("Saved: {label}"));
                            s.save_msg.clear();
                        }
                        Err(e) => s.notify_err(format!("Save failed: {e}")),
                    }
                }
                self.trigger_refresh()
            }

            // ── Main – rollback ──────────────────────────────────────────────
            Message::BeginRollback => {
                if let Screen::Main(s) = &mut self.screen {
                    if let Some(entry) = s.selected_history_entry().cloned() {
                        s.modal = Some(Modal::TextInput {
                            prompt: format!(
                                "Roll back to:\n  [{}] {}\n\nEnter a name for the new route:",
                                entry.short_id, entry.message
                            ),
                            value: String::new(),
                            action: TextAction::RollbackNewRoute { target_id: entry.id },
                        });
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
                        prompt: "Create new route\n(working directory must be clean):"
                            .to_string(),
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
                        s.modal = Some(Modal::Confirm {
                            prompt: format!(
                                "You have unsaved changes.\n\
                                 Switching to '{}' will discard them.\n\
                                 Continue?",
                                name
                            ),
                            action: ConfirmAction::SwitchRoute { name },
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

            Message::BeginDeleteRoute => {
                if let Screen::Main(s) = &mut self.screen {
                    if let Some(r) = s.selected_route() {
                        if r.is_current {
                            s.notify_err("Cannot delete the current route");
                        } else {
                            let name = r.name.clone();
                            s.modal = Some(Modal::Confirm {
                                prompt: format!(
                                    "Delete route '{name}'?\n\
                                     All saves on this route will be permanently removed.\n\
                                     This cannot be undone."
                                ),
                                action: ConfirmAction::DeleteRoute { name },
                            });
                        }
                    }
                }
                Task::none()
            }

            Message::ToggleRecovery => {
                if let Screen::Main(s) = &mut self.screen {
                    s.is_recovery = !s.is_recovery;
                    s.sel_route = 0;
                    s.sel_hist = 0;
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
            self.screen = Screen::Init(InitState { dir: path, error: None });
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
        let recent = RecentPathCache::new().load_paths();
        let cwd = std::env::current_dir().unwrap_or_default();
        self.screen = Screen::Picker(PickerState {
            input: cwd.to_string_lossy().to_string(),
            recent,
            error: None,
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
                ConfirmAction::DeleteRoute { name } => {
                    let dir = match &mut self.screen {
                        Screen::Main(s) => {
                            s.busy = true;
                            s.dir.clone()
                        }
                        _ => return Task::none(),
                    };
                    Task::perform(
                        async move { delete_route(dir, name) },
                        Message::ActionDone,
                    )
                }
            },
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
                let label = p.to_string_lossy().to_string();
                button(text(label).size(13))
                    .on_press(Message::PickerOpenRecent(p.clone()))
                    .style(style_btn_ghost)
                    .padding([5, 10])
                    .width(Length::Fill)
                    .into()
            })
            .collect()
    };

    let error_elem: Element<Message> = match &s.error {
        Some(e) => text(e.as_str()).size(13).color(C_ERROR).into(),
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

    let card = container(
        column![
            text("Initialize Gitsave Repository?").size(20).color(C_TEXT),
            vertical_space().height(12),
            text(s.dir.display().to_string()).size(14).color(C_ACCENT),
            vertical_space().height(12),
            text(
                "This directory does not contain a gitsave repository.\n\
                 Would you like to initialize one here?"
            )
            .size(14)
            .color(C_DIM),
            vertical_space().height(20),
            error_elem,
            row![
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
            .align_y(Alignment::Center),
        ]
        .spacing(0)
        .padding(32),
    )
    .width(500)
    .style(style_card);

    center_widget(card.into())
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
        text("  ⚕ RECOVERY MODE  ").size(12).color(C_RECOVERY).into()
    } else {
        horizontal_space().width(0).into()
    };

    let busy_badge: Element<Message> = if s.busy {
        text("  ⟳  ").size(12).color(C_DIM).into()
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
            button(text("⟳").size(14).color(C_DIM))
                .on_press(Message::Refresh)
                .style(style_btn_ghost)
                .padding([4, 8]),
            horizontal_space().width(4),
            button(text("← Back").size(12).color(C_DIM))
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
        let b = button(text("→ Switch to").size(12))
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
        let b = button(text("✎ Rename").size(12))
            .style(style_btn_secondary)
            .padding([5, 10])
            .width(Length::Fill);
        if !s.routes.is_empty() && !s.busy {
            b.on_press(Message::BeginRenameRoute)
        } else {
            b
        }
    };

    let can_delete = s
        .routes
        .get(s.sel_route)
        .map(|r| !r.is_current && !is_recovery_branch_name(&r.name))
        .unwrap_or(false)
        && !s.busy;

    let delete_btn = {
        let b = button(text("✕ Delete Route").size(12).color(C_ERROR))
            .style(style_btn_ghost)
            .padding([5, 10])
            .width(Length::Fill);
        if can_delete {
            b.on_press(Message::BeginDeleteRoute)
        } else {
            b
        }
    };

    let recovery_label =
        if s.is_recovery { "✕ Exit Recovery" } else { "⚕ Recovery Mode" };
    let recovery_color = if s.is_recovery { C_WARN } else { C_DIM };

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
            vertical_space().height(2),
            delete_btn,
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
        let b = button(text("💾 Save").size(13))
            .style(style_btn_primary)
            .padding([7, 14]);
        if s.busy { b } else { b.on_press(Message::TrySave) }
    };

    let force_btn = {
        let b = button(text("⚡ Force Save").size(13))
            .style(style_btn_secondary)
            .padding([7, 14]);
        if s.busy { b } else { b.on_press(Message::ForceSave) }
    };

    let rollback_btn = {
        let enabled = !s.history.is_empty() && !s.busy;
        let b = button(text("↩ Rollback").size(13))
            .style(if enabled { style_btn_secondary } else { style_btn_disabled })
            .padding([7, 14]);
        if enabled { b.on_press(Message::BeginRollback) } else { b }
    };

    let discard_btn = {
        let enabled = s.is_dirty() && !s.busy;
        let label_color = if enabled { C_ERROR } else { C_DIM };
        let b = button(text("✕ Discard").size(13).color(label_color))
            .style(style_btn_ghost)
            .padding([7, 14]);
        if enabled { b.on_press(Message::BeginDiscard) } else { b }
    };

    let amend_btn = {
        let enabled = !s.history.is_empty() && !s.busy;
        let b = button(text("✎ Amend").size(13).color(C_DIM))
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
            text("✓  Working directory clean").size(12).color(C_SUCCESS).into()
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
                text(format!("⚠  {n} uncommitted change(s)"))
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

    let (routes, history) = if is_recovery {
        let rr: Vec<RouteInfo> = all_routes
            .into_iter()
            .filter(|r| is_recovery_branch_name(&r.name))
            .collect();
        let hist: Vec<SaveEntry> =
            rr.iter().filter_map(|r| r.latest_save.clone()).collect();
        (rr, hist)
    } else {
        let normal: Vec<RouteInfo> = all_routes
            .into_iter()
            .filter(|r| !is_recovery_branch_name(&r.name))
            .collect();
        (normal, raw_history)
    };

    Ok(RefreshData { routes, history, status })
}

fn do_save(dir: PathBuf, message: String, force: bool) -> Result<String, String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    let mut mgr = SaveManager::new(core);
    let result = if force { mgr.save_force(&message) } else { mgr.save(&message) };
    result
        .map(|r| format!("[{}] {}", r.short_oid, r.message))
        .map_err(|e| e.to_string())
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

fn delete_route(dir: PathBuf, name: String) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    RouteManager::new(core).delete_route(&name).map_err(|e| e.to_string())
}

fn amend_message(dir: PathBuf, message: String) -> Result<(), String> {
    let core = Git2Core::open(&dir).map_err(|e| e.to_string())?;
    SaveManager::new(core)
        .amend_head_message(&message)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn init_repo(dir: PathBuf) -> Result<(), String> {
    const DEFAULT_COMPRESSION: i32 = 6;
    let mut core = Git2Core::init(&dir).map_err(|e| e.to_string())?;
    core.set_core_compression(DEFAULT_COMPRESSION).map_err(|e| e.to_string())?;

    let config_content = format!(
        "# gitsave configuration\n\
         [save]\nmax_history = 50\ncompression = {DEFAULT_COMPRESSION}\n\n\
         [auto_save]\nenabled = false\n\n\
         [author]\nname = \"\"\nemail = \"\"\n"
    );
    let config_path = dir.join("gitsave.toml");
    std::fs::write(&config_path, config_content).map_err(|e| e.to_string())?;

    let attr_path = dir.join(".gitattributes");
    std::fs::write(&attr_path, "# Treat game saves as binary\nsaves/** -text -diff -merge\n")
        .map_err(|e| e.to_string())?;

    core.commit_files(&[config_path, attr_path], "init gitsave config")
        .map(|_| ())
        .map_err(|e| e.to_string())
}

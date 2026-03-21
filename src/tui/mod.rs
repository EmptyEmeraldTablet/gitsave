use std::collections::HashSet;
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Local;
use crossterm::ExecutableCommand;
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::terminal::Terminal;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::core::{RouteInfo, SaveEntry, SaveResult, SaveStatus};
use crate::error::SaveError;
use crate::git::Git2Core;
use crate::manager::{AutoSaveConfig, ConfigManager, RouteManager, SaveManager, is_recovery_branch_name};

const AUTO_REFRESH_SECS: u64 = 10;
const BUSY_REDRAW_MS: u64 = 500;
const TICK_RATE_MS: u64 = 400;
const MAX_NOTIFICATION_LINES: usize = 4;

pub fn run(save_dir: &Path) -> Result<()> {
    // Preflight check before switching to alternate screen
    Git2Core::open(save_dir).map_err(|err| {
        anyhow::anyhow!(
            "Not a gitsave repository at {}. Run `gitsave init` first. ({})",
            save_dir.display(),
            err
        )
    })?;

    let mut stdout = stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new(save_dir.to_path_buf())?;
    let tick_rate = Duration::from_millis(TICK_RATE_MS);
    let mut should_quit = false;
    let mut cursor_busy = false;
    let mut last_busy_redraw = Instant::now();

    app.mark_dirty();

    while !should_quit {
        let busy_now = app.busy.is_some();
        if busy_now != cursor_busy {
            let style = if busy_now {
                SetCursorStyle::SteadyBar
            } else {
                SetCursorStyle::SteadyBlock
            };
            terminal.backend_mut().execute(style)?;
            cursor_busy = busy_now;
            app.mark_dirty();
        }

        let mut should_draw = app.dirty;
        if !should_draw
            && busy_now
            && last_busy_redraw.elapsed() >= Duration::from_millis(BUSY_REDRAW_MS)
        {
            should_draw = true;
        }
        if should_draw {
            terminal.draw(|f| draw_ui(f, &app))?;
            app.clear_dirty();
            if busy_now {
                last_busy_redraw = Instant::now();
            }
        }

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if app.handle_key(key.code, &mut should_quit)? {
                        break;
                    }
                }
                Event::Resize(_, _) => {
                    app.refresh()?;
                    app.mark_dirty();
                }
                _ => {}
            }
        }

        if app.last_refresh.elapsed() >= Duration::from_secs(AUTO_REFRESH_SECS) {
            app.refresh()?;
            app.mark_dirty();
        }
    }

    disable_raw_mode()?;
    terminal
        .backend_mut()
        .execute(SetCursorStyle::DefaultUserShape)?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Routes,
    History,
}

impl Focus {
    fn toggle(self) -> Self {
        match self {
            Focus::Routes => Focus::History,
            Focus::History => Focus::Routes,
        }
    }
}

enum UiMode {
    Normal,
    SavePrompt {
        buffer: String,
        mode: SaveMode,
    },
    RollbackPrompt {
        buffer: String,
        action: PendingAction,
    },
    AmendPrompt {
        buffer: String,
    },
    RecoveryRename {
        buffer: String,
        target: RouteInfo,
    },
    RenameRoute {
        buffer: String,
        target: RouteInfo,
    },
    CreateRoute {
        buffer: String,
        switch: bool,
    },
    ConfirmAction {
        prompt: String,
        action: PendingAction,
    },
    ResolveDirty {
        prompt: String,
        action: PendingAction,
    },
    ResolveUnstableSave {
        prompt: String,
        request: SaveRequest,
    },
}

#[derive(Clone)]
enum PendingAction {
    RollbackSave {
        short_id: String,
        label: String,
        force: bool,
    },
    CreateRoute {
        name: String,
        switch: bool,
    },
    SwitchRoute {
        name: String,
        force: bool,
    },
    RecoverRoute {
        old_name: String,
        new_name: String,
    },
    DiscardChanges,
}

#[derive(Clone)]
struct SaveRequest {
    message: String,
    after: Option<PendingAction>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveMode {
    Stable,
    Force,
}

enum SaveOutcome {
    Saved(SaveResult),
    Unstable(u32),
    Failed(String),
}

struct AppState {
    save_dir: PathBuf,
    routes: Vec<RouteInfo>,
    route_index: usize,
    recovery_routes: Vec<RouteInfo>,
    recovery_index: usize,
    recovery_view: bool,
    all_history: Vec<SaveEntry>,
    history: Vec<SaveEntry>,
    history_index: usize,
    route_history_ids: HashSet<String>,
    route_history_ready: bool,
    status: SaveStatus,
    autosave: AutoSaveConfig,
    focus: Focus,
    last_refresh: Instant,
    notifications: Vec<UiLogEntry>,
    mode: UiMode,
    follow_current_route: bool,
    busy: Option<BusyIndicator>,
    dirty: bool,
}

impl AppState {
    fn new(save_dir: PathBuf) -> Result<Self> {
        let mut state = Self {
            save_dir,
            routes: Vec::new(),
            route_index: 0,
            recovery_routes: Vec::new(),
            recovery_index: 0,
            recovery_view: false,
            all_history: Vec::new(),
            history: Vec::new(),
            history_index: 0,
            route_history_ids: HashSet::new(),
            route_history_ready: false,
            status: SaveStatus {
                current_route: String::new(),
                last_save: None,
                pending_changes: Vec::new(),
                has_uncommitted_changes: false,
            },
            autosave: AutoSaveConfig::default(),
            focus: Focus::Routes,
            last_refresh: Instant::now(),
            notifications: Vec::new(),
            mode: UiMode::Normal,
            follow_current_route: true,
            busy: None,
            dirty: true,
        };
        state.log_info("TUI ready. Press r to refresh, q to quit.");
        state.refresh()?;
        Ok(state)
    }

    fn refresh(&mut self) -> Result<()> {
        let core = Git2Core::open(&self.save_dir)?;
        self.routes = core.list_routes()?;
        self.routes
            .retain(|route| !is_recovery_branch_name(&route.name));
        if self.routes.is_empty() {
            self.route_index = 0;
        } else if self.route_index >= self.routes.len() {
            self.route_index = self.routes.len() - 1;
        }

        self.status = core.get_status()?;
        if self.follow_current_route && !self.status.current_route.is_empty() {
            if let Some(idx) = self
                .routes
                .iter()
                .position(|route| route.name == self.status.current_route)
            {
                self.route_index = idx;
            }
        }

        let mut history = core.get_history()?;
        history.retain(|entry| !is_recovery_branch_name(&entry.route));
        history.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        self.all_history = history;
        if let Err(err) = self.update_route_history_ids(&core) {
            self.log_error(format!("Failed to load route history: {}", err));
        }
        self.apply_history_filter();

        self.autosave = ConfigManager::new(&self.save_dir).load_auto_save_config();
        self.last_refresh = Instant::now();
        self.mark_dirty();
        Ok(())
    }

    fn load_recovery_routes(&mut self) -> Result<()> {
        let core = Git2Core::open(&self.save_dir)?;
        let mut routes = core.list_routes()?;
        routes.retain(|route| is_recovery_branch_name(&route.name));
        self.recovery_routes = routes;
        if self.recovery_routes.is_empty() {
            self.recovery_index = 0;
        } else if self.recovery_index >= self.recovery_routes.len() {
            self.recovery_index = self.recovery_routes.len() - 1;
        }
        Ok(())
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    fn current_route_name(&self) -> Option<String> {
        self.routes
            .get(self.route_index)
            .map(|route| route.name.clone())
    }

    fn selected_route_is_current(&self) -> bool {
        self.routes
            .get(self.route_index)
            .map(|route| route.is_current)
            .unwrap_or(true)
    }

    fn in_recovery_mode(&self) -> bool {
        self.recovery_view
    }

    fn update_route_history_ids(&mut self, core: &Git2Core) -> Result<()> {
        self.route_history_ids.clear();
        self.route_history_ready = false;
        let route = match self.current_route_name() {
            Some(route) => route,
            None => return Ok(()),
        };
        let ids = core.get_history_ids_for_route(&route)?;
        self.route_history_ids = ids;
        self.route_history_ready = true;
        Ok(())
    }

    fn refresh_route_history_ids(&mut self) {
        self.route_history_ids.clear();
        self.route_history_ready = false;
        let result = match Git2Core::open(&self.save_dir) {
            Ok(core) => self.update_route_history_ids(&core),
            Err(err) => Err(anyhow::Error::new(err)),
        };
        if let Err(err) = result {
            self.log_error(format!("Failed to load route history: {}", err));
        }
    }

    fn apply_history_filter(&mut self) {
        let filtered: Vec<SaveEntry> = if self.current_route_name().is_some() {
            if self.route_history_ready {
                self.all_history
                    .iter()
                    .filter(|entry| self.route_history_ids.contains(&entry.id))
                    .cloned()
                    .collect()
            } else {
                self.all_history.clone()
            }
        } else {
            self.all_history.clone()
        };

        self.history = filtered;
        if self.history.is_empty() {
            self.history_index = 0;
            return;
        }
        if let Some(current_save) = self.status.last_save.as_ref() {
            if let Some(idx) = self
                .history
                .iter()
                .position(|entry| entry.short_id == current_save.short_id)
            {
                self.history_index = idx;
            } else if self.history_index >= self.history.len() {
                self.history_index = self.history.len() - 1;
            }
        } else if self.history_index >= self.history.len() {
            self.history_index = self.history.len() - 1;
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Routes => {
                if !self.routes.is_empty() && self.route_index + 1 < self.routes.len() {
                    self.route_index += 1;
                    self.follow_current_route = false;
                    self.refresh_route_history_ids();
                    self.apply_history_filter();
                }
            }
            Focus::History => {
                if !self.history.is_empty() && self.history_index + 1 < self.history.len() {
                    self.history_index += 1;
                }
            }
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Routes => {
                if self.route_index > 0 {
                    self.route_index -= 1;
                    self.follow_current_route = false;
                    self.refresh_route_history_ids();
                    self.apply_history_filter();
                }
            }
            Focus::History => {
                if self.history_index > 0 {
                    self.history_index -= 1;
                }
            }
        }
    }

    fn move_recovery_down(&mut self) {
        if !self.recovery_routes.is_empty() && self.recovery_index + 1 < self.recovery_routes.len()
        {
            self.recovery_index += 1;
        }
    }

    fn move_recovery_up(&mut self) {
        if self.recovery_index > 0 {
            self.recovery_index -= 1;
        }
    }

    fn page_down(&mut self) {
        if self.focus == Focus::History && !self.history.is_empty() {
            let jump = (self.history.len() / 5).max(1);
            self.history_index = (self.history_index + jump).min(self.history.len() - 1);
        }
    }

    fn page_up(&mut self) {
        if self.focus == Focus::History && !self.history.is_empty() {
            let jump = (self.history.len() / 5).max(1);
            self.history_index = self.history_index.saturating_sub(jump);
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = self.focus.toggle();
    }

    fn handle_key(&mut self, code: KeyCode, should_quit: &mut bool) -> Result<bool> {
        match &mut self.mode {
            UiMode::Normal => {
                if self.recovery_view {
                    let mut handled = false;
                    match code {
                        KeyCode::Char('q') => {
                            *should_quit = true;
                            return Ok(true);
                        }
                        KeyCode::Char('R') | KeyCode::Esc => {
                            self.recovery_view = false;
                            handled = true;
                        }
                        KeyCode::Char('r') => {
                            self.with_busy("Refreshing recovery...", |s| s.load_recovery_routes())?;
                            handled = true;
                        }
                        KeyCode::Enter => {
                            if self.focus == Focus::Routes {
                                self.start_recovery_rename()?;
                            } else {
                                self.log_info("Recovery view uses route list.");
                            }
                            handled = true;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.focus == Focus::Routes {
                                self.move_recovery_down();
                            } else {
                                self.move_down();
                            }
                            handled = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.focus == Focus::Routes {
                                self.move_recovery_up();
                            } else {
                                self.move_up();
                            }
                            handled = true;
                        }
                        KeyCode::PageDown => {
                            self.page_down();
                            handled = true;
                        }
                        KeyCode::PageUp => {
                            self.page_up();
                            handled = true;
                        }
                        KeyCode::Tab => {
                            self.toggle_focus();
                            handled = true;
                        }
                        KeyCode::Char('s')
                        | KeyCode::Char('S')
                        | KeyCode::Char('l')
                        | KeyCode::Char('L')
                        | KeyCode::Char('c')
                        | KeyCode::Char('C')
                        | KeyCode::Char('n')
                        | KeyCode::Char('x')
                        | KeyCode::Char('X')
                        | KeyCode::Char('d')
                        | KeyCode::Char('m') => {
                            self.log_info("Exit recovery view to manage routes or saves.");
                            handled = true;
                        }
                        _ => {}
                    }
                    if handled {
                        self.mark_dirty();
                    }
                    return Ok(false);
                }
                let mut handled = false;
                match code {
                    KeyCode::Char('q') => {
                        *should_quit = true;
                        return Ok(true);
                    }
                    KeyCode::Char('r') => {
                        self.with_busy("Refreshing...", |s| s.refresh())?;
                        handled = true;
                    }
                    KeyCode::Char('R') => {
                        self.start_recovery_list()?;
                        handled = true;
                    }
                    KeyCode::Char('s') => {
                        self.start_save_prompt(SaveMode::Stable)?;
                        handled = true;
                    }
                    KeyCode::Char('S') => {
                        self.start_save_prompt(SaveMode::Force)?;
                        handled = true;
                    }
                    KeyCode::Char('l') => {
                        self.request_rollback_selected(false);
                        handled = true;
                    }
                    KeyCode::Char('L') => {
                        self.request_rollback_selected(true);
                        handled = true;
                    }
                    KeyCode::Char('c') => {
                        self.start_route_prompt(false);
                        handled = true;
                    }
                    KeyCode::Char('n') => {
                        self.start_route_rename()?;
                        handled = true;
                    }
                    KeyCode::Char('C') => {
                        self.start_route_prompt(true);
                        handled = true;
                    }
                    KeyCode::Char('x') => {
                        self.request_route_switch(false);
                        handled = true;
                    }
                    KeyCode::Char('X') => {
                        self.request_route_switch(true);
                        handled = true;
                    }
                    KeyCode::Char('d') => {
                        self.request_discard_changes();
                        handled = true;
                    }
                    KeyCode::Char('m') => {
                        self.start_amend_prompt()?;
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.activate_selection();
                        handled = true;
                    }
                    KeyCode::Tab => {
                        if self.focus == Focus::Routes && !self.selected_route_is_current() {
                            self.log_info("Switch to this route to browse its history.");
                        } else {
                            self.toggle_focus();
                        }
                        handled = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.move_down();
                        handled = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.move_up();
                        handled = true;
                    }
                    KeyCode::PageDown => {
                        self.page_down();
                        handled = true;
                    }
                    KeyCode::PageUp => {
                        self.page_up();
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::SavePrompt { buffer, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Esc => {
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.confirm_save_prompt()?;
                        handled = true;
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                        handled = true;
                    }
                    KeyCode::Char(ch) => {
                        buffer.push(ch);
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::RollbackPrompt { buffer, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Esc => {
                        self.log_info("Rollback cancelled");
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.confirm_rollback_prompt()?;
                        handled = true;
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                        handled = true;
                    }
                    KeyCode::Char(ch) => {
                        if is_valid_route_char(ch) {
                            buffer.push(ch);
                            handled = true;
                        }
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::AmendPrompt { buffer } => {
                let mut handled = false;
                match code {
                    KeyCode::Esc => {
                        self.log_info("Amend cancelled");
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.confirm_amend_prompt()?;
                        handled = true;
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                        handled = true;
                    }
                    KeyCode::Char(ch) => {
                        buffer.push(ch);
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::RecoveryRename { buffer, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Esc => {
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.confirm_recovery_rename()?;
                        handled = true;
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                        handled = true;
                    }
                    KeyCode::Char(ch) => {
                        if is_valid_route_char(ch) {
                            buffer.push(ch);
                            handled = true;
                        }
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::RenameRoute { buffer, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Esc => {
                        self.log_info("Rename cancelled");
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.confirm_route_rename()?;
                        handled = true;
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                        handled = true;
                    }
                    KeyCode::Char(ch) => {
                        if is_valid_route_char(ch) {
                            buffer.push(ch);
                            handled = true;
                        }
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::CreateRoute { buffer, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Esc => {
                        self.cancel_route_prompt();
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.prepare_route_creation()?;
                        handled = true;
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                        handled = true;
                    }
                    KeyCode::Char(ch) => {
                        if is_valid_route_char(ch) {
                            buffer.push(ch);
                            handled = true;
                        }
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::ConfirmAction { action, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                        'y' => {
                            let pending = action.clone();
                            self.mode = UiMode::Normal;
                            self.execute_pending_action(pending)?;
                            handled = true;
                        }
                        'n' => {
                            self.log_info("Action cancelled");
                            self.mode = UiMode::Normal;
                            handled = true;
                        }
                        _ => {}
                    },
                    KeyCode::Esc => {
                        self.log_info("Action cancelled");
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::ResolveDirty { action, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                        's' => {
                            let pending = action.clone();
                            self.mode = UiMode::Normal;
                            self.save_then_action(pending)?;
                            handled = true;
                        }
                        'd' => {
                            let pending = action.clone();
                            self.mode = UiMode::Normal;
                            self.discard_then_action(pending)?;
                            handled = true;
                        }
                        'c' => {
                            self.log_info("Action cancelled");
                            self.mode = UiMode::Normal;
                            handled = true;
                        }
                        _ => {}
                    },
                    KeyCode::Esc => {
                        self.log_info("Action cancelled");
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
            UiMode::ResolveUnstableSave { request, .. } => {
                let mut handled = false;
                match code {
                    KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                        'f' => {
                            let pending = request.clone();
                            self.mode = UiMode::Normal;
                            self.perform_save_force(pending)?;
                            handled = true;
                        }
                        'r' => {
                            let pending = request.clone();
                            self.mode = UiMode::Normal;
                            self.perform_save_stable(pending)?;
                            handled = true;
                        }
                        'c' => {
                            self.log_info("Save cancelled");
                            self.mode = UiMode::Normal;
                            handled = true;
                        }
                        _ => {}
                    },
                    KeyCode::Esc => {
                        self.log_info("Save cancelled");
                        self.mode = UiMode::Normal;
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    self.mark_dirty();
                }
                Ok(false)
            }
        }
    }

    fn log_info(&mut self, message: impl Into<String>) {
        self.push_notification(UiLogEntry::info(message.into()));
    }

    fn log_error(&mut self, message: impl Into<String>) {
        self.push_notification(UiLogEntry::error(message.into()));
    }

    fn push_notification(&mut self, entry: UiLogEntry) {
        self.notifications.push(entry);
        if self.notifications.len() > MAX_NOTIFICATION_LINES {
            let excess = self.notifications.len() - MAX_NOTIFICATION_LINES;
            self.notifications.drain(0..excess);
        }
    }

    fn latest_notification(&self) -> Option<&UiLogEntry> {
        self.notifications.last()
    }

    fn start_save_prompt(&mut self, mode: SaveMode) -> Result<()> {
        self.refresh_status_only()?;
        if mode == SaveMode::Stable && !self.status.has_uncommitted_changes {
            self.log_info("Working tree clean; no save needed. Use S to force a snapshot.");
            return Ok(());
        }
        self.mode = UiMode::SavePrompt {
            buffer: String::new(),
            mode,
        };
        Ok(())
    }

    fn start_amend_prompt(&mut self) -> Result<()> {
        self.refresh_status_only()?;
        if self.status.has_uncommitted_changes {
            self.log_info("Working tree dirty; save or discard changes first.");
            return Ok(());
        }
        self.mode = UiMode::AmendPrompt {
            buffer: String::new(),
        };
        Ok(())
    }

    fn confirm_save_prompt(&mut self) -> Result<()> {
        let (message, mode) = match &self.mode {
            UiMode::SavePrompt { buffer, mode } => (buffer.trim().to_string(), *mode),
            _ => return Ok(()),
        };
        self.mode = UiMode::Normal;
        let message = if message.is_empty() {
            self.save_message_for_mode(mode)
        } else {
            message
        };
        let request = SaveRequest {
            message,
            after: None,
        };
        match mode {
            SaveMode::Stable => self.perform_save_stable(request),
            SaveMode::Force => self.perform_save_force(request),
        }
    }

    fn confirm_amend_prompt(&mut self) -> Result<()> {
        let message = match &self.mode {
            UiMode::AmendPrompt { buffer } => buffer.trim().to_string(),
            _ => return Ok(()),
        };
        self.mode = UiMode::Normal;
        if message.is_empty() {
            self.log_error("Message cannot be empty.");
            return Ok(());
        }
        self.perform_amend(message)
    }

    fn start_rollback_prompt(&mut self, action: PendingAction) -> Result<()> {
        self.mode = UiMode::RollbackPrompt {
            buffer: String::new(),
            action,
        };
        Ok(())
    }

    fn confirm_rollback_prompt(&mut self) -> Result<()> {
        let (route_name, action) = match &self.mode {
            UiMode::RollbackPrompt { buffer, action } => (buffer.trim().to_string(), action.clone()),
            _ => return Ok(()),
        };
        self.mode = UiMode::Normal;

        if route_name.is_empty() {
            self.log_error("Route name required for rollback.");
            return Ok(());
        }

        match action {
            PendingAction::RollbackSave {
                short_id,
                label,
                force,
            } => self.perform_rollback(&short_id, &label, &route_name, force),
            _ => Ok(()),
        }
    }

    fn start_recovery_list(&mut self) -> Result<()> {
        if self.recovery_view {
            self.recovery_view = false;
            return Ok(());
        }
        self.with_busy("Loading recovery...", |s| s.load_recovery_routes())?;
        if self.recovery_routes.is_empty() {
            self.log_info("No recovery snapshots.");
            self.recovery_view = false;
            return Ok(());
        }
        self.focus = Focus::Routes;
        self.recovery_view = true;
        Ok(())
    }

    fn start_recovery_rename(&mut self) -> Result<()> {
        let target = match self.recovery_routes.get(self.recovery_index) {
            Some(route) => route.clone(),
            None => {
                self.log_error("No recovery snapshot selected.");
                return Ok(());
            }
        };
        self.mode = UiMode::RecoveryRename {
            buffer: String::new(),
            target,
        };
        Ok(())
    }

    fn confirm_recovery_rename(&mut self) -> Result<()> {
        let (buffer, target) = match &self.mode {
            UiMode::RecoveryRename { buffer, target } => (buffer.trim().to_string(), target.clone()),
            _ => return Ok(()),
        };
        self.mode = UiMode::Normal;
        self.refresh_status_only()?;

        let short_hash = target.name.chars().take(7).collect::<String>();
        let default_name = format!("recovery-{}", short_hash);
        let new_name = if buffer.is_empty() {
            default_name
        } else {
            buffer
        };

        if !is_valid_route_name(&new_name) {
            self.log_error("Invalid route name. Use letters, digits, '-', '_', '/'.");
            return Ok(());
        }

        let core = Git2Core::open(&self.save_dir)?;
        let routes = core.list_routes()?;
        if routes.iter().any(|route| route.name == new_name) {
            self.log_error(format!("Route '{}' already exists.", new_name));
            return Ok(());
        }

        let action = PendingAction::RecoverRoute {
            old_name: target.name,
            new_name,
        };

        let prompt = "Recover this snapshot and switch to the route?";
        if self.status.has_uncommitted_changes {
            self.mode = UiMode::ResolveDirty {
                prompt: prompt.to_string(),
                action,
            };
        } else {
            self.execute_pending_action(action)?;
        }

        Ok(())
    }

    fn save_then_action(&mut self, action: PendingAction) -> Result<()> {
        self.refresh_status_only()?;
        if !self.status.has_uncommitted_changes {
            self.log_info("Working tree clean; skipping save.");
            return self.execute_pending_action(action);
        }
        let request = SaveRequest {
            message: self.guard_message_for_action(&action),
            after: Some(action),
        };
        self.perform_save_stable(request)
    }

    fn discard_then_action(&mut self, action: PendingAction) -> Result<()> {
        let mut discarded = false;
        self.with_busy("Discarding changes...", |s| {
            let mut manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            match manager.discard_changes() {
                Ok(()) => {
                    discarded = true;
                    s.log_info("Discarded uncommitted changes (recovery snapshot created).");
                    s.follow_current_route = true;
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!("Discard failed: {}", err));
                }
            }
            Ok(())
        })?;

        if discarded {
            self.execute_pending_action(action)?;
        }

        Ok(())
    }

    fn request_rollback_selected(&mut self, force: bool) {
        if !self.selected_route_is_current() {
            self.log_info("Rollback locked. Switch to this route first.");
            return;
        }
        let entry = match self.history.get(self.history_index) {
            Some(entry) => entry.clone(),
            None => {
                self.log_error("No save selected to roll back.");
                return;
            }
        };
        let action = PendingAction::RollbackSave {
            short_id: entry.short_id.clone(),
            label: entry.message.clone(),
            force,
        };
        if force {
            self.mode = UiMode::ConfirmAction {
                prompt: format!(
                    "Force roll back to save {} ({})? Unsaved changes will be discarded. A new route will be created.",
                    entry.short_id, entry.message
                ),
                action,
            };
            return;
        }

        let prompt = format!(
            "Roll back to save {} ({})? A new route will be created.",
            entry.short_id, entry.message
        );
        if self.status.has_uncommitted_changes {
            self.mode = UiMode::ResolveDirty { prompt, action };
        } else {
            self.mode = UiMode::ConfirmAction { prompt, action };
        }
    }

    fn start_route_rename(&mut self) -> Result<()> {
        let target = match self.routes.get(self.route_index) {
            Some(route) => route.clone(),
            None => {
                self.log_error("No route selected.");
                return Ok(());
            }
        };
        self.mode = UiMode::RenameRoute {
            buffer: String::new(),
            target,
        };
        Ok(())
    }

    fn start_route_prompt(&mut self, switch: bool) {
        self.mode = UiMode::CreateRoute {
            buffer: String::new(),
            switch,
        };
    }

    fn cancel_route_prompt(&mut self) {
        self.mode = UiMode::Normal;
    }

    fn prepare_route_creation(&mut self) -> Result<()> {
        let (name, switch) = match &self.mode {
            UiMode::CreateRoute { buffer, switch } => (buffer.trim().to_string(), *switch),
            _ => return Ok(()),
        };
        if name.is_empty() {
            self.log_error("Route name cannot be empty");
            self.mode = UiMode::Normal;
            return Ok(());
        }
        let prompt = if switch {
            format!("Create and switch to route '{}'?", name)
        } else {
            format!("Create new route '{}'?", name)
        };
        let action = PendingAction::CreateRoute { name, switch };
        if self.status.has_uncommitted_changes {
            self.mode = UiMode::ResolveDirty { prompt, action };
        } else {
            self.mode = UiMode::ConfirmAction { prompt, action };
        }
        Ok(())
    }

    fn confirm_route_rename(&mut self) -> Result<()> {
        let (buffer, target) = match &self.mode {
            UiMode::RenameRoute { buffer, target } => (buffer.trim().to_string(), target.clone()),
            _ => return Ok(()),
        };
        self.mode = UiMode::Normal;

        if buffer.is_empty() {
            self.log_error("Route name cannot be empty.");
            return Ok(());
        }
        if !is_valid_route_name(&buffer) {
            self.log_error("Invalid route name. Use letters, digits, '-', '_', '/'.");
            return Ok(());
        }
        if buffer == target.name {
            self.log_info("Route name unchanged.");
            return Ok(());
        }

        let core = Git2Core::open(&self.save_dir)?;
        let routes = core.list_routes()?;
        if routes.iter().any(|route| route.name == buffer) {
            self.log_error(format!("Route '{}' already exists.", buffer));
            return Ok(());
        }

        self.with_busy("Renaming route...", |s| {
            let mut manager = RouteManager::new(Git2Core::open(&s.save_dir)?);
            match manager.rename_route(&target.name, &buffer) {
                Ok(()) => {
                    s.log_info(format!("Renamed route '{}' to '{}'", target.name, buffer));
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!(
                        "Failed to rename route '{}' to '{}': {}",
                        target.name, buffer, err
                    ));
                }
            }
            Ok(())
        })
    }

    fn request_route_switch(&mut self, force: bool) {
        let route = match self.routes.get(self.route_index) {
            Some(r) => r,
            None => {
                self.log_error("No route selected.");
                return;
            }
        };
        if route.is_current {
            self.log_info(format!("Already on route '{}'", route.name));
            return;
        }

        let action = PendingAction::SwitchRoute {
            name: route.name.clone(),
            force,
        };
        if force {
            self.mode = UiMode::ConfirmAction {
                prompt: format!(
                    "Force switch to route '{}'? Unsaved changes will be discarded.",
                    route.name
                ),
                action,
            };
            return;
        }

        let prompt = format!("Switch to route '{}'?", route.name);
        if self.status.has_uncommitted_changes {
            self.mode = UiMode::ResolveDirty { prompt, action };
        } else {
            self.mode = UiMode::ConfirmAction { prompt, action };
        }
    }

    fn request_discard_changes(&mut self) {
        if !self.status.has_uncommitted_changes {
            self.log_info("Working tree already clean.");
            return;
        }
        self.mode = UiMode::ConfirmAction {
            prompt: "Discard all uncommitted changes? This removes untracked files and records a recovery snapshot.".to_string(),
            action: PendingAction::DiscardChanges,
        };
    }

    fn execute_pending_action(&mut self, action: PendingAction) -> Result<()> {
        match action {
            PendingAction::RollbackSave { .. } => self.start_rollback_prompt(action),
            PendingAction::CreateRoute { name, switch } => {
                self.perform_route_creation(&name, switch)
            }
            PendingAction::SwitchRoute { name, force } => self.perform_route_switch(&name, force),
            PendingAction::RecoverRoute { old_name, new_name } => {
                self.perform_recovery_route(&old_name, &new_name)
            }
            PendingAction::DiscardChanges => self.perform_discard_changes(),
        }
    }

    fn perform_save_stable(&mut self, request: SaveRequest) -> Result<()> {
        let message = request.message.clone();
        let mut outcome = SaveOutcome::Failed("Save failed.".to_string());
        self.with_busy("Saving...", |s| {
            let mut manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            outcome = match manager.save(&message) {
                Ok(result) => {
                    manager.update_last_save_time();
                    SaveOutcome::Saved(result)
                }
                Err(SaveError::UnstableSave { attempts }) => SaveOutcome::Unstable(attempts),
                Err(err) => SaveOutcome::Failed(format!("Save error: {}", err)),
            };
            Ok(())
        })?;

        match outcome {
            SaveOutcome::Saved(result) => {
                self.log_info(format!(
                    "Save complete ({} - {})",
                    result.short_oid, result.message
                ));
                self.refresh()?;
                if let Some(action) = request.after {
                    self.execute_pending_action(action)?;
                }
            }
            SaveOutcome::Unstable(attempts) => {
                self.mode = UiMode::ResolveUnstableSave {
                    prompt: format!(
                        "Save files still changing after {} checks. Force save?",
                        attempts
                    ),
                    request,
                };
                self.mark_dirty();
            }
            SaveOutcome::Failed(message) => {
                self.log_error(message);
            }
        }

        Ok(())
    }

    fn perform_save_force(&mut self, request: SaveRequest) -> Result<()> {
        let message = request.message.clone();
        let mut outcome = SaveOutcome::Failed("Save failed.".to_string());
        self.with_busy("Saving...", |s| {
            let mut manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            outcome = match manager.save_force(&message) {
                Ok(result) => {
                    manager.update_last_save_time();
                    SaveOutcome::Saved(result)
                }
                Err(err) => SaveOutcome::Failed(format!("Save error: {}", err)),
            };
            Ok(())
        })?;

        match outcome {
            SaveOutcome::Saved(result) => {
                self.log_info(format!(
                    "Force save complete ({} - {})",
                    result.short_oid, result.message
                ));
                self.refresh()?;
                if let Some(action) = request.after {
                    self.execute_pending_action(action)?;
                }
            }
            SaveOutcome::Failed(message) => {
                self.log_error(message);
            }
            SaveOutcome::Unstable(_) => {}
        }

        Ok(())
    }

    fn perform_amend(&mut self, message: String) -> Result<()> {
        self.with_busy("Updating message...", |s| {
            let mut manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            match manager.amend_head_message(&message) {
                Ok(result) => {
                    s.log_info(format!(
                        "Updated latest save message ({} - {})",
                        result.short_oid, result.message
                    ));
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!("Amend failed: {}", err));
                }
            }
            Ok(())
        })
    }

    fn perform_rollback(
        &mut self,
        short_id: &str,
        label: &str,
        route_name: &str,
        force: bool,
    ) -> Result<()> {
        let banner = if force {
            "Force rolling back..."
        } else {
            "Rolling back..."
        };
        self.with_busy(banner, |s| {
            let manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            let status = manager.get_status()?;
            if status.has_uncommitted_changes && !force {
                s.log_error("Working tree dirty; save or discard changes first.");
                return Ok(());
            }

            let mut core = manager.into_core();
            let routes = match core.list_routes() {
                Ok(routes) => routes,
                Err(err) => {
                    s.log_error(format!("Failed to list routes: {}", err));
                    return Ok(());
                }
            };
            if routes.iter().any(|route| route.name == route_name) {
                s.log_error(format!("Route '{}' already exists.", route_name));
                return Ok(());
            }

            match core.switch_create_route_at(short_id, route_name) {
                Ok(()) => {
                    s.log_info(format!(
                        "Rolled back to {} ({}) on route '{}'",
                        short_id, label, route_name
                    ));
                    s.follow_current_route = true;
                    s.refresh()?;
                }
                Err(SaveError::SaveNotFound(_)) => {
                    s.log_error("Selected save no longer exists.");
                }
                Err(err) => {
                    s.log_error(format!("Rollback failed: {}", err));
                }
            }
            Ok(())
        })
    }

    fn perform_route_creation(&mut self, name: &str, switch: bool) -> Result<()> {
        self.with_busy("Creating route...", |s| {
            let mut manager = RouteManager::new(Git2Core::open(&s.save_dir)?);
            let result = if switch {
                manager.switch_create_route(name)
            } else {
                manager.create_route(name)
            };
            match result {
                Ok(()) => {
                    if switch {
                        s.log_info(format!("Created and switched to route '{}'", name));
                        s.follow_current_route = true;
                    } else {
                        s.log_info(format!("Created route '{}'", name));
                    }
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!("Failed to create route '{}': {}", name, err));
                }
            }
            Ok(())
        })
    }

    fn perform_route_switch(&mut self, name: &str, force: bool) -> Result<()> {
        let banner = if force { "Force switching route..." } else { "Switching route..." };
        self.with_busy(banner, |s| {
            if force {
                let mut core = Git2Core::open(&s.save_dir)?;
                if let Err(err) = core.discard_changes() {
                    s.log_error(format!("Discard failed: {}", err));
                    return Ok(());
                }
                match core.switch_route(name) {
                    Ok(()) => {
                        s.log_info(format!("Switched to route '{}'", name));
                        s.follow_current_route = true;
                        s.refresh()?;
                    }
                    Err(err) => {
                        s.log_error(format!("Failed to switch route '{}': {}", name, err));
                    }
                }
                return Ok(());
            }

            let mut manager = RouteManager::new(Git2Core::open(&s.save_dir)?);
            match manager.switch_route(name) {
                Ok(()) => {
                    s.log_info(format!("Switched to route '{}'", name));
                    s.follow_current_route = true;
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!("Failed to switch route '{}': {}", name, err));
                }
            }
            Ok(())
        })
    }

    fn perform_recovery_route(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.with_busy("Recovering snapshot...", |s| {
            let mut core = Git2Core::open(&s.save_dir)?;
            if let Err(err) = core.rename_route(old_name, new_name) {
                s.log_error(format!("Failed to rename recovery route: {}", err));
                return Ok(());
            }
            match core.switch_route(new_name) {
                Ok(()) => {
                    s.log_info(format!("Recovered to route '{}'", new_name));
                    s.follow_current_route = true;
                    s.recovery_view = false;
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!("Failed to switch to route '{}': {}", new_name, err));
                }
            }
            Ok(())
        })
    }

    fn perform_discard_changes(&mut self) -> Result<()> {
        self.with_busy("Discarding changes...", |s| {
            let mut manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            match manager.discard_changes() {
                Ok(()) => {
                    s.log_info("Discarded uncommitted changes (recovery snapshot created).");
                    s.follow_current_route = true;
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!("Discard failed: {}", err));
                }
            }
            Ok(())
        })
    }

    fn activate_selection(&mut self) {
        match self.focus {
            Focus::Routes => self.request_route_switch(false),
            Focus::History => self.request_rollback_selected(false),
        }
    }

    fn with_busy<F>(&mut self, message: &str, mut action: F) -> Result<()>
    where
        F: FnMut(&mut Self) -> Result<()>,
    {
        self.busy = Some(BusyIndicator::new(message));
        self.mark_dirty();
        let result = action(self);
        self.busy = None;
        self.mark_dirty();
        result
    }

    fn refresh_status_only(&mut self) -> Result<()> {
        let core = Git2Core::open(&self.save_dir)?;
        self.status = core.get_status()?;
        Ok(())
    }

    fn save_message_for_mode(&self, mode: SaveMode) -> String {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        match mode {
            SaveMode::Stable => format!("[quick] {}", timestamp),
            SaveMode::Force => format!("[force] {}", timestamp),
        }
    }

    fn guard_message_for_action(&self, action: &PendingAction) -> String {
        let detail = match action {
            PendingAction::RollbackSave { short_id, .. } => {
                format!("before rollback {}", short_id)
            }
            PendingAction::CreateRoute { name, switch } => {
                if *switch {
                    format!("before create+switch route {}", name)
                } else {
                    format!("before create route {}", name)
                }
            }
            PendingAction::SwitchRoute { name, .. } => {
                format!("before switch route {}", name)
            }
            PendingAction::RecoverRoute { new_name, .. } => {
                format!("before recover route {}", new_name)
            }
            PendingAction::DiscardChanges => "before discard".to_string(),
        };
        format!("[guard] {}", detail)
    }
}

struct UiLogEntry {
    message: String,
    timestamp: chrono::DateTime<Local>,
    is_error: bool,
}

impl UiLogEntry {
    fn info(message: String) -> Self {
        Self {
            message,
            timestamp: Local::now(),
            is_error: false,
        }
    }

    fn error(message: String) -> Self {
        Self {
            message,
            timestamp: Local::now(),
            is_error: true,
        }
    }

    fn style(&self) -> Style {
        if self.is_error {
            Style::default().fg(Color::LightRed)
        } else {
            Style::default().fg(Color::LightGreen)
        }
    }
}

struct BusyIndicator {
    message: String,
    started: Instant,
}

impl BusyIndicator {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            started: Instant::now(),
        }
    }

    fn spinner(&self) -> char {
        const FRAMES: [char; 4] = ['|', '/', '-', '\\'];
        let idx = ((self.started.elapsed().as_millis() / 150) % FRAMES.len() as u128) as usize;
        FRAMES[idx]
    }
}

fn draw_ui(f: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Percentage(60),
                Constraint::Percentage(25),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    let recovery_tag = if app.in_recovery_mode() {
        " · Recovery Mode"
    } else {
        ""
    };
    let mut header_text = format!(
        "gitsave TUI · {} · 路线: {}{} · Autosave: {} ({}s) · 刷新:{:>3}s",
        app.save_dir.display(),
        if app.status.current_route.is_empty() {
            "(unknown)"
        } else {
            &app.status.current_route
        },
        recovery_tag,
        if app.autosave.enabled { "ON" } else { "OFF" },
        app.autosave.interval,
        app.last_refresh.elapsed().as_secs()
    );
    if let Some(busy) = &app.busy {
        header_text.push_str(&format!("  [{}] {}", busy.spinner(), busy.message));
    } else if let Some(note) = app.latest_notification() {
        header_text.push_str(&format!("  最近事件: {}", note.message));
    } else {
        header_text.push_str("  等待操作");
    }
    let header_style = if app.busy.is_some() {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if app.latest_notification().map(|note| note.is_error).unwrap_or(false) {
        Style::default().fg(Color::LightRed)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let header = Paragraph::new(header_text).style(header_style);
    f.render_widget(header, chunks[0]);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .split(chunks[1]);

    draw_routes_panel(f, body_chunks[0], app);
    draw_history_panel(f, body_chunks[1], app);

    draw_notifications(f, chunks[2], app);

    let help = Paragraph::new(Line::from(vec![
        Span::styled("↑/↓ j/k", Style::default().fg(Color::Yellow)),
        Span::raw(" move  "),
        Span::styled("PgUp/PgDn", Style::default().fg(Color::Yellow)),
        Span::raw(" page  "),
        Span::styled("s/S", Style::default().fg(Color::Yellow)),
        Span::raw(": save  "),
        Span::styled("m", Style::default().fg(Color::Yellow)),
        Span::raw(": amend  "),
        Span::styled("l/L", Style::default().fg(Color::Yellow)),
        Span::raw(": rollback  "),
        Span::styled("c/C", Style::default().fg(Color::Yellow)),
        Span::raw(": create  "),
        Span::styled("n", Style::default().fg(Color::Yellow)),
        Span::raw(": rename  "),
        Span::styled("x/X", Style::default().fg(Color::Yellow)),
        Span::raw(": switch  "),
        Span::styled("R", Style::default().fg(Color::Yellow)),
        Span::raw(": recovery  "),
        Span::styled("d", Style::default().fg(Color::Yellow)),
        Span::raw(": discard  "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": focus  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(": quit"),
    ]));
    f.render_widget(help, chunks[3]);

    if let Some(modal) = ModalOverlay::from_app(app) {
        let area = centered_rect(60, 40, f.size());
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(modal.title)
            .border_style(Style::default().fg(Color::Yellow));
        let widget = Paragraph::new(modal.lines)
            .block(block)
            .wrap(Wrap { trim: true });
        f.render_widget(widget, area);
    }
}

fn draw_routes_panel(f: &mut Frame, area: Rect, app: &AppState) {
    let panel_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(5)].as_ref())
        .split(area);

    let recovery_mode = app.in_recovery_mode();
    let routes_block = Block::default()
        .borders(Borders::ALL)
        .title(if recovery_mode {
            "Recovery Routes"
        } else {
            "Routes"
        })
        .border_style(panel_border_style(app.focus == Focus::Routes));

    let route_items: Vec<ListItem> = if recovery_mode {
        if app.recovery_routes.is_empty() {
            vec![ListItem::new("No recovery snapshots")]
        } else {
            app.recovery_routes
                .iter()
                .map(|route| {
                    let short_hash: String = route.name.chars().take(7).collect();
                    let latest = route
                        .latest_save
                        .as_ref()
                        .map(|s| format!(" · {}", s.message))
                        .unwrap_or_default();
                    ListItem::new(format!("  {}{}", short_hash, latest))
                })
                .collect()
        }
    } else if app.routes.is_empty() {
        vec![ListItem::new("No routes yet")]
    } else {
        app.routes
            .iter()
            .map(|route| {
                let marker = if route.is_current { "*" } else { " " };
                let latest = route
                    .latest_save
                    .as_ref()
                    .map(|s| format!(" · {}", s.message))
                    .unwrap_or_default();
                ListItem::new(format!("{} {}", marker, route.name) + &latest)
            })
            .collect()
    };

    let routes_list = List::new(route_items)
        .block(routes_block)
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let mut route_state = ListState::default();
    if recovery_mode {
        if !app.recovery_routes.is_empty() {
            route_state.select(Some(app.recovery_index));
        }
    } else if !app.routes.is_empty() {
        route_state.select(Some(app.route_index));
    }
    f.render_stateful_widget(routes_list, panel_chunks[0], &mut route_state);

    let (aux_title, aux_lines) = if recovery_mode {
        (
            "Recovery",
            vec![
                Line::from("Enter: restore snapshot"),
                Line::from("Esc: exit recovery"),
                Line::from("r: refresh list"),
                Line::from("Rename after restore"),
            ],
        )
    } else {
        (
            "Autosave",
            vec![
                Line::from(format!(
                    "Status : {}",
                    if app.autosave.enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                )),
                Line::from(format!("Interval: {}s", app.autosave.interval)),
                Line::from(format!("Max saves: {}", app.autosave.max_count)),
                Line::from(match app.autosave.last_save_time {
                    Some(ts) => format!(
                        "Last save: {}",
                        chrono::DateTime::from_timestamp(ts, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    ),
                    None => "Last save: never".to_string(),
                }),
            ],
        )
    };

    let aux_block = Block::default().borders(Borders::ALL).title(aux_title);
    let aux_widget = Paragraph::new(aux_lines)
        .block(aux_block)
        .wrap(Wrap { trim: true });
    f.render_widget(aux_widget, panel_chunks[1]);
}

fn draw_history_panel(f: &mut Frame, area: Rect, app: &AppState) {
    let panel_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(7)].as_ref())
        .split(area);

    let history_block = Block::default()
        .borders(Borders::ALL)
        .title("History")
        .border_style(panel_border_style(app.focus == Focus::History));

    let history_items: Vec<ListItem> = if app.history.is_empty() {
        vec![ListItem::new("No saves yet")]
    } else {
        app.history
            .iter()
            .map(|entry| {
                let ts = entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                ListItem::new(format!("{}  {}  {}", entry.short_id, ts, entry.message))
            })
            .collect()
    };

    let history_list = List::new(history_items)
        .block(history_block)
        .highlight_symbol(">> ")
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );

    let mut history_state = ListState::default();
    if !app.history.is_empty() && app.selected_route_is_current() {
        history_state.select(Some(app.history_index));
    }
    f.render_stateful_widget(history_list, panel_chunks[0], &mut history_state);

    let detail_block = Block::default().borders(Borders::ALL).title("Status");
    let detail = Paragraph::new(status_message(app))
        .block(detail_block)
        .wrap(Wrap { trim: true });
    f.render_widget(detail, panel_chunks[1]);
}

fn status_message(app: &AppState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(current) = app.status.last_save.as_ref() {
        let current_route = if app.status.current_route.is_empty() {
            current.route.clone()
        } else {
            app.status.current_route.clone()
        };
        lines.push(Line::from(Span::styled(
            format!("Current save: {} ({})", current.short_id, current_route),
            Style::default().fg(Color::LightGreen),
        )));
        lines.push(Line::from(current.message.clone()));
    } else {
        lines.push(Line::from("Current save: unknown"));
    }

    if !app.selected_route_is_current() {
        let selected_route = app
            .current_route_name()
            .unwrap_or_else(|| "unknown".to_string());
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Selected route: {} (non-current)", selected_route),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(Line::from("Switch to this route to browse or roll back."));
    } else if let Some(selected) = app.history.get(app.history_index) {
        if app
            .status
            .last_save
            .as_ref()
            .map(|save| save.short_id != selected.short_id)
            .unwrap_or(true)
        {
            let selected_route = app
                .current_route_name()
                .unwrap_or_else(|| selected.route.clone());
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Selected save: {} ({})", selected.short_id, selected_route),
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(selected.message.clone()));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Working tree:",
        Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::BOLD),
    )));
    if app.status.has_uncommitted_changes {
        let total = app.status.pending_changes.len();
        let new_files = app
            .status
            .pending_changes
            .iter()
            .filter(|change| matches!(change.status, crate::core::ChangeStatus::Added))
            .count();
        lines.push(Line::from(Span::styled(
            format!(
                "Dirty files: {} ({} new/untracked) — s to save, d to discard",
                total, new_files
            ),
            Style::default().fg(Color::Red),
        )));
        for change in &app.status.pending_changes {
            let symbol = match change.status {
                crate::core::ChangeStatus::Added => "+",
                crate::core::ChangeStatus::Modified => "~",
                crate::core::ChangeStatus::Deleted => "-",
            };
            lines.push(Line::from(format!("{} {}", symbol, change.path)));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Clean working tree",
            Style::default().fg(Color::Gray),
        )));
    }

    lines
}

fn panel_border_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn draw_notifications(f: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Notifications");
    let lines: Vec<Line> = if app.notifications.is_empty() {
        vec![Line::from("No events yet")]
    } else {
        app.notifications
            .iter()
            .rev()
            .map(|entry| {
                let text = format!("[{}] {}", entry.timestamp.format("%H:%M:%S"), entry.message);
                Line::styled(text, entry.style())
            })
            .collect()
    };

    let widget = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

fn is_valid_route_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '/')
}

fn is_valid_route_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_valid_route_char)
}

struct ModalOverlay {
    title: String,
    lines: Vec<Line<'static>>,
}

impl ModalOverlay {
    fn from_app(app: &AppState) -> Option<Self> {
        match &app.mode {
            UiMode::Normal => None,
            UiMode::CreateRoute { buffer, switch } => {
                let mut lines = Vec::new();
                if *switch {
                    lines.push(Line::from("Enter a new route name to create and switch."));
                } else {
                    lines.push(Line::from("Enter a new route name."));
                }
                lines.push(Line::from(format!("> {}", buffer)));
                lines.push(Line::from("Allowed: letters, digits, -, _, /."));
                lines.push(Line::from("Enter = confirm · Esc = cancel"));
                Some(Self {
                    title: if *switch {
                        "Create & Switch".to_string()
                    } else {
                        "Create Route".to_string()
                    },
                    lines,
                })
            }
            UiMode::ConfirmAction { prompt, .. } => {
                let lines = vec![
                    Line::from(prompt.clone()),
                    Line::from("Press y to confirm, n or Esc to cancel."),
                ];
                Some(Self {
                    title: "Confirm Action".to_string(),
                    lines,
                })
            }
            UiMode::ResolveDirty { prompt, .. } => {
                let lines = vec![
                    Line::from(prompt.clone()),
                    Line::from("Choose: s = save, d = discard, c = cancel."),
                ];
                Some(Self {
                    title: "Working Tree Dirty".to_string(),
                    lines,
                })
            }
            UiMode::ResolveUnstableSave { prompt, .. } => {
                let lines = vec![
                    Line::from(prompt.clone()),
                    Line::from("Choose: f = force, r = retry, c = cancel."),
                ];
                Some(Self {
                    title: "Save Not Stable".to_string(),
                    lines,
                })
            }
            UiMode::RollbackPrompt { buffer, action } => {
                let detail = match action {
                    PendingAction::RollbackSave { short_id, label, .. } => {
                        format!("Rollback to {} ({})", short_id, label)
                    }
                    _ => "Rollback".to_string(),
                };
                let lines = vec![
                    Line::from(detail),
                    Line::from("Enter a new route name for rollback."),
                    Line::from(format!("> {}", buffer)),
                    Line::from("Allowed: letters, digits, -, _, /."),
                    Line::from("Enter = confirm · Esc = cancel"),
                ];
                Some(Self {
                    title: "Rollback".to_string(),
                    lines,
                })
            }
            UiMode::SavePrompt { buffer, mode } => {
                let title = match mode {
                    SaveMode::Stable => "Quick Save",
                    SaveMode::Force => "Force Save",
                };
                let lines = vec![
                    Line::from("Enter a save message (optional)."),
                    Line::from(format!("> {}", buffer)),
                    Line::from("Enter = save · Esc = cancel · Empty = auto message"),
                ];
                Some(Self {
                    title: title.to_string(),
                    lines,
                })
            }
            UiMode::AmendPrompt { buffer } => {
                let lines = vec![
                    Line::from("Edit latest save message (HEAD only)."),
                    Line::from(format!("> {}", buffer)),
                    Line::from("Enter = confirm · Esc = cancel"),
                ];
                Some(Self {
                    title: "Amend Message".to_string(),
                    lines,
                })
            }
            UiMode::RecoveryRename { buffer, target } => {
                let short_hash = target.name.chars().take(7).collect::<String>();
                let default_name = format!("recovery-{}", short_hash);
                let lines = vec![
                    Line::from("Rename recovery snapshot before switching."),
                    Line::from(format!("> {}", buffer)),
                    Line::from(format!("Empty = {}", default_name)),
                    Line::from("Enter = confirm · Esc = back"),
                ];
                Some(Self {
                    title: "Recover Snapshot".to_string(),
                    lines,
                })
            }
            UiMode::RenameRoute { buffer, target } => {
                let lines = vec![
                    Line::from(format!("Rename route '{}'.", target.name)),
                    Line::from(format!("> {}", buffer)),
                    Line::from("Allowed: letters, digits, -, _, /."),
                    Line::from("Enter = confirm · Esc = cancel"),
                ];
                Some(Self {
                    title: "Rename Route".to_string(),
                    lines,
                })
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(area);

    Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(horizontal[1])[1]
}

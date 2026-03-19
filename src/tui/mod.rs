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

use crate::core::{RouteInfo, SaveEntry, SaveStatus};
use crate::error::SaveError;
use crate::git::Git2Core;
use crate::manager::{AutoSaveConfig, ConfigManager, RouteManager, SaveManager};

const AUTO_REFRESH_SECS: u64 = 10;
const AUTO_SAVE_POLL_SECS: u64 = 1;
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
    CreateRoute {
        buffer: String,
    },
    ConfirmAction {
        prompt: String,
        action: PendingAction,
    },
}

#[derive(Clone)]
enum PendingAction {
    QuickSave,
    LoadSave { short_id: String, label: String },
    CreateRoute { name: String },
    SwitchRoute { name: String },
}

struct AppState {
    save_dir: PathBuf,
    routes: Vec<RouteInfo>,
    route_index: usize,
    all_history: Vec<SaveEntry>,
    history: Vec<SaveEntry>,
    history_index: usize,
    route_history_ids: HashSet<String>,
    route_history_ready: bool,
    status: SaveStatus,
    autosave: AutoSaveConfig,
    focus: Focus,
    last_refresh: Instant,
    last_auto_poll: Instant,
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
            last_auto_poll: Instant::now(),
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
        let mut core = Git2Core::open(&self.save_dir)?;
        self.routes = core.list_routes()?;
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
        let mut filtered: Vec<SaveEntry> = if self.current_route_name().is_some() {
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

        if let Some(current_save) = self.status.last_save.as_ref() {
            if let Some(idx) = filtered
                .iter()
                .position(|entry| entry.short_id == current_save.short_id)
            {
                self.history_index = idx;
            } else if self.history_index >= filtered.len() && !filtered.is_empty() {
                self.history_index = filtered.len() - 1;
            }
        } else if self.history_index >= filtered.len() && !filtered.is_empty() {
            self.history_index = filtered.len() - 1;
        }

        if filtered.is_empty() {
            self.history_index = 0;
        }

        self.history = filtered;
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
                    KeyCode::Char('s') => {
                        self.request_quick_save();
                        handled = true;
                    }
                    KeyCode::Char('l') => {
                        self.request_load_selected();
                        handled = true;
                    }
                    KeyCode::Char('c') => {
                        self.start_route_prompt();
                        handled = true;
                    }
                    KeyCode::Char('a') => {
                        self.trigger_manual_autosave()?;
                        handled = true;
                    }
                    KeyCode::Enter => {
                        self.activate_selection();
                        handled = true;
                    }
                    KeyCode::Tab => {
                        self.toggle_focus();
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
            UiMode::CreateRoute { buffer } => {
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
        }
    }

    fn maybe_auto_save(&mut self, force_check: bool) -> Result<()> {
        if !self.autosave.enabled {
            if force_check {
                self.log_info("Auto-save disabled. Use `gitsave autosave --enable` to turn it on.");
            }
            return Ok(());
        }
        if !force_check && self.last_auto_poll.elapsed() < Duration::from_secs(AUTO_SAVE_POLL_SECS)
        {
            return Ok(());
        }
        self.last_auto_poll = Instant::now();

        let mut manager = SaveManager::new(Git2Core::open(&self.save_dir)?);
        if !manager.should_auto_save() {
            if force_check {
                self.log_info("Auto-save interval not reached yet.");
            }
            return Ok(());
        }

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        match manager.save(&format!("[auto] {}", timestamp)) {
            Ok(result) => {
                manager.update_last_save_time();
                self.log_info(format!(
                    "Auto-save complete ({} - {})",
                    result.short_oid, result.message
                ));
                self.refresh()?;
            }
            Err(err) => {
                self.log_error(format!("Auto-save error: {}", err));
            }
        }
        Ok(())
    }

    fn trigger_manual_autosave(&mut self) -> Result<()> {
        self.with_busy("Auto-saving...", |s| s.maybe_auto_save(true))
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

    fn request_quick_save(&mut self) {
        self.mode = UiMode::ConfirmAction {
            prompt: "Quick save current working tree?".to_string(),
            action: PendingAction::QuickSave,
        };
    }

    fn request_load_selected(&mut self) {
        let entry = match self.history.get(self.history_index) {
            Some(entry) => entry.clone(),
            None => {
                self.log_error("No save selected to load.");
                return;
            }
        };
        let mut prompt = format!("Load save {} ({})?", entry.short_id, entry.message);
        if self.status.has_uncommitted_changes {
            prompt.push_str(" Unsaved changes will be discarded!");
        }
        self.mode = UiMode::ConfirmAction {
            prompt,
            action: PendingAction::LoadSave {
                short_id: entry.short_id.clone(),
                label: entry.message.clone(),
            },
        };
    }

    fn start_route_prompt(&mut self) {
        self.mode = UiMode::CreateRoute {
            buffer: String::new(),
        };
    }

    fn cancel_route_prompt(&mut self) {
        self.mode = UiMode::Normal;
    }

    fn prepare_route_creation(&mut self) -> Result<()> {
        let name = match &self.mode {
            UiMode::CreateRoute { buffer } => buffer.trim().to_string(),
            _ => return Ok(()),
        };
        if name.is_empty() {
            self.log_error("Route name cannot be empty");
            self.mode = UiMode::Normal;
            return Ok(());
        }
        self.mode = UiMode::ConfirmAction {
            prompt: format!("Create new route '{}'?", name),
            action: PendingAction::CreateRoute { name },
        };
        Ok(())
    }

    fn execute_pending_action(&mut self, action: PendingAction) -> Result<()> {
        match action {
            PendingAction::QuickSave => self.perform_quick_save(),
            PendingAction::LoadSave { short_id, label } => self.perform_load(&short_id, &label),
            PendingAction::CreateRoute { name } => self.perform_route_creation(&name),
            PendingAction::SwitchRoute { name } => self.perform_route_switch(&name),
        }
    }

    fn perform_quick_save(&mut self) -> Result<()> {
        self.with_busy("Saving...", |s| {
            let mut manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            if s.status.has_uncommitted_changes {
                s.log_info("Working tree dirty; proceeding with quick save.");
            }
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
            match manager.save(&format!("[quick] {}", timestamp)) {
                Ok(result) => {
                    manager.update_last_save_time();
                    s.log_info(format!(
                        "Quick save complete ({} - {})",
                        result.short_oid, result.message
                    ));
                    s.refresh()?;
                }
                Err(err) => s.log_error(format!("Quick save error: {}", err)),
            }
            Ok(())
        })
    }

    fn perform_load(&mut self, short_id: &str, label: &str) -> Result<()> {
        self.with_busy("Loading save...", |s| {
            let mut manager = SaveManager::new(Git2Core::open(&s.save_dir)?);
            match manager.load(short_id, false) {
                Ok(()) => {
                    s.log_info(format!("Loaded {} ({})", short_id, label));
                    s.follow_current_route = true;
                    s.refresh()?;
                }
                Err(SaveError::SaveNotFound(_)) => {
                    s.log_error("Selected save no longer exists.");
                }
                Err(err) => {
                    s.log_error(format!("Load failed: {}", err));
                }
            }
            Ok(())
        })
    }

    fn perform_route_creation(&mut self, name: &str) -> Result<()> {
        self.with_busy("Creating route...", |s| {
            let mut manager = RouteManager::new(Git2Core::open(&s.save_dir)?);
            match manager.switch_create_route(name) {
                Ok(()) => {
                    s.log_info(format!("Created route '{}'", name));
                    s.follow_current_route = true;
                    s.refresh()?;
                }
                Err(err) => {
                    s.log_error(format!("Failed to create route '{}': {}", name, err));
                }
            }
            Ok(())
        })
    }

    fn perform_route_switch(&mut self, name: &str) -> Result<()> {
        self.with_busy("Switching route...", |s| {
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

    fn activate_selection(&mut self) {
        match self.focus {
            Focus::Routes => self.request_route_switch(),
            Focus::History => self.request_load_selected(),
        }
    }

    fn request_route_switch(&mut self) {
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
        let mut prompt = format!("Switch to route '{}'?", route.name);
        if self.status.has_uncommitted_changes {
            prompt.push_str(" Unsaved changes will be discarded!");
        }
        self.mode = UiMode::ConfirmAction {
            prompt,
            action: PendingAction::SwitchRoute {
                name: route.name.clone(),
            },
        };
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

    let mut header_text = format!(
        "gitsave TUI · {} · 路线: {} · Autosave: {} ({}s) · 刷新:{:>3}s",
        app.save_dir.display(),
        if app.status.current_route.is_empty() {
            "(unknown)"
        } else {
            &app.status.current_route
        },
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
    let header = Paragraph::new(header_text).style(Style::default().fg(Color::Cyan));
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
        Span::raw(" navigate  "),
        Span::styled("PgUp/PgDn", Style::default().fg(Color::Yellow)),
        Span::raw(" fast scroll  "),
        Span::styled("s", Style::default().fg(Color::Yellow)),
        Span::raw(": quick save  "),
        Span::styled("l", Style::default().fg(Color::Yellow)),
        Span::raw(": load selection  "),
        Span::styled("c", Style::default().fg(Color::Yellow)),
        Span::raw(": new route  "),
        Span::styled("Focus:", Style::default().fg(Color::Gray)),
        Span::raw(format!(
            " {}",
            match app.focus {
                Focus::Routes => "Routes",
                Focus::History => "History",
            }
        )),
        Span::raw("  autosave runs automatically when enabled"),
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

    let routes_block = Block::default()
        .borders(Borders::ALL)
        .title("Routes")
        .border_style(panel_border_style(app.focus == Focus::Routes));

    let route_items: Vec<ListItem> = if app.routes.is_empty() {
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
    if !app.routes.is_empty() {
        route_state.select(Some(app.route_index));
    }
    f.render_stateful_widget(routes_list, panel_chunks[0], &mut route_state);

    let autosave_lines = vec![
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
    ];

    let autosave_block = Block::default().borders(Borders::ALL).title("Autosave");
    let autosave_widget = Paragraph::new(autosave_lines)
        .block(autosave_block)
        .wrap(Wrap { trim: true });
    f.render_widget(autosave_widget, panel_chunks[1]);
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
    if !app.history.is_empty() {
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

    if let Some(selected) = app.history.get(app.history_index) {
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
                "Dirty files: {} ({} new/untracked) — save before switching routes!",
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

struct ModalOverlay {
    title: String,
    lines: Vec<Line<'static>>,
}

impl ModalOverlay {
    fn from_app(app: &AppState) -> Option<Self> {
        match &app.mode {
            UiMode::Normal => None,
            UiMode::CreateRoute { buffer } => {
                let mut lines = Vec::new();
                lines.push(Line::from("Enter a new route name."));
                lines.push(Line::from(format!("> {}", buffer)));
                lines.push(Line::from("Allowed: letters, digits, -, _, /."));
                lines.push(Line::from("Enter = confirm · Esc = cancel"));
                Some(Self {
                    title: "Create Route".to_string(),
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

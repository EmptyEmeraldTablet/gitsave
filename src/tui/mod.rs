use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::ExecutableCommand;
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
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::core::{RouteInfo, SaveEntry, SaveStatus};
use crate::git::Git2Core;
use crate::manager::{AutoSaveConfig, ConfigManager};

const AUTO_REFRESH_SECS: u64 = 10;

pub fn run(save_dir: &Path) -> Result<()> {
    let mut stdout = stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new(save_dir.to_path_buf())?;
    let tick_rate = Duration::from_millis(200);
    let mut should_quit = false;

    while !should_quit {
        terminal.draw(|f| draw_ui(f, &app))?;

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') => should_quit = true,
                    KeyCode::Char('r') => app.refresh()?,
                    KeyCode::Tab => app.toggle_focus(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::PageDown => app.page_down(),
                    KeyCode::PageUp => app.page_up(),
                    _ => {}
                },
                Event::Resize(_, _) => app.refresh()?,
                _ => {}
            }
        }

        if app.last_refresh.elapsed() >= Duration::from_secs(AUTO_REFRESH_SECS) {
            app.refresh()?;
        }
    }

    disable_raw_mode()?;
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

struct AppState {
    save_dir: PathBuf,
    routes: Vec<RouteInfo>,
    route_index: usize,
    history: Vec<SaveEntry>,
    history_index: usize,
    status: SaveStatus,
    autosave: AutoSaveConfig,
    focus: Focus,
    last_refresh: Instant,
}

impl AppState {
    fn new(save_dir: PathBuf) -> Result<Self> {
        let mut state = Self {
            save_dir,
            routes: Vec::new(),
            route_index: 0,
            history: Vec::new(),
            history_index: 0,
            status: SaveStatus {
                current_route: String::new(),
                last_save: None,
                pending_changes: Vec::new(),
                has_uncommitted_changes: false,
            },
            autosave: AutoSaveConfig::default(),
            focus: Focus::Routes,
            last_refresh: Instant::now(),
        };
        state.refresh()?;
        Ok(state)
    }

    fn refresh(&mut self) -> Result<()> {
        let mut core = Git2Core::open(&self.save_dir)?;
        self.routes = core.list_routes()?;
        if self.route_index >= self.routes.len() && !self.routes.is_empty() {
            self.route_index = self.routes.len() - 1;
        }

        self.status = core.get_status()?;

        let mut history = core.get_history()?;
        if let Some(route) = self.current_route_name() {
            history.retain(|entry| entry.route == route);
        }
        self.history = history;
        if self.history_index >= self.history.len() && !self.history.is_empty() {
            self.history_index = self.history.len() - 1;
        }

        self.autosave = ConfigManager::new(&self.save_dir).load_auto_save_config();
        self.last_refresh = Instant::now();
        Ok(())
    }

    fn current_route_name(&self) -> Option<String> {
        self.routes
            .get(self.route_index)
            .map(|route| route.name.clone())
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Routes => {
                if !self.routes.is_empty() && self.route_index + 1 < self.routes.len() {
                    self.route_index += 1;
                    let _ = self.refresh();
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
                    let _ = self.refresh();
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
}

fn draw_ui(f: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(f.size());

    let header = Paragraph::new(format!(
        "gitsave TUI · 路线: {} · Autosave: {} ({:>3}s) | q:退出  r:刷新  tab:切换焦点",
        app.status.current_route,
        if app.autosave.enabled { "ON" } else { "OFF" },
        app.autosave.interval
    ))
    .style(Style::default().fg(Color::Cyan));
    f.render_widget(header, chunks[0]);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(chunks[1]);

    draw_routes_panel(f, body_chunks[0], app);
    draw_history_panel(f, body_chunks[1], app);

    let help = Paragraph::new(Line::from(vec![
        Span::styled("↑/↓ j/k", Style::default().fg(Color::Yellow)),
        Span::raw(" navigate  "),
        Span::styled("PgUp/PgDn", Style::default().fg(Color::Yellow)),
        Span::raw(" fast scroll  "),
        Span::styled("Focus:", Style::default().fg(Color::Gray)),
        Span::raw(format!(
            " {}",
            match app.focus {
                Focus::Routes => "Routes",
                Focus::History => "History",
            }
        )),
    ]));
    f.render_widget(help, chunks[2]);
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
    if let Some(entry) = app.history.get(app.history_index) {
        lines.push(Line::from(Span::styled(
            format!("Selected save: {} ({})", entry.short_id, entry.route),
            Style::default().fg(Color::LightGreen),
        )));
        lines.push(Line::from(entry.message.clone()));
    } else {
        lines.push(Line::from("Selected save: N/A"));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Working tree:",
        Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::BOLD),
    )));
    if app.status.has_uncommitted_changes {
        for change in &app.status.pending_changes {
            let symbol = match change.status {
                crate::core::ChangeStatus::Added => "+",
                crate::core::ChangeStatus::Modified => "~",
                crate::core::ChangeStatus::Deleted => "-",
            };
            lines.push(Line::from(format!("{} {}", symbol, change.path)));
        }
    } else {
        lines.push(Line::from("Clean working tree"));
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

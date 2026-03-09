use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::CrosstermBackend,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};

use cortex_common::protocol::{
    DaemonStatus, EventSummary, InsightSummary, RelatedFileEntry, Request, Response,
};

use super::send_request;

#[derive(PartialEq, Clone, Copy)]
enum Panel {
    Activity,
    Insights,
    Graph,
}

struct App {
    status: Option<DaemonStatus>,
    events: Vec<EventSummary>,
    insights: Vec<InsightSummary>,
    graph_data: Vec<RelatedFileEntry>,
    graph_file: Option<String>,
    daemon_error: Option<String>,

    activity_state: ListState,
    insight_state: ListState,
    graph_state: ListState,
    focused_panel: Panel,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        let mut activity_state = ListState::default();
        activity_state.select(Some(0));
        Self {
            status: None,
            events: Vec::new(),
            insights: Vec::new(),
            graph_data: Vec::new(),
            graph_file: None,
            daemon_error: None,
            activity_state,
            insight_state: ListState::default(),
            graph_state: ListState::default(),
            focused_panel: Panel::Activity,
            should_quit: false,
        }
    }

    async fn refresh_data(&mut self) {
        // Fetch status
        match send_request(Request::Status).await {
            Ok(Response::Status(s)) => {
                self.status = Some(s);
                self.daemon_error = None;
            }
            Ok(Response::Error(e)) => {
                self.daemon_error = Some(e);
            }
            Err(e) => {
                self.daemon_error = Some(format!("{e:#}"));
                self.status = None;
                self.events.clear();
                self.insights.clear();
                return;
            }
            _ => {}
        }

        // Fetch history
        if let Ok(Response::HistoryResult(evts)) = send_request(Request::History { limit: 100 }).await {
            self.events = evts;
        }

        // Fetch pending insights
        if let Ok(Response::InsightsResult(ins)) = send_request(Request::GetInsights).await {
            self.insights = ins;
        }
    }

    fn scroll_up(&mut self) {
        match self.focused_panel {
            Panel::Activity => {
                let i = self.activity_state.selected().unwrap_or(0);
                if i > 0 {
                    self.activity_state.select(Some(i - 1));
                }
            }
            Panel::Insights => {
                let i = self.insight_state.selected().unwrap_or(0);
                if i > 0 {
                    self.insight_state.select(Some(i - 1));
                }
            }
            Panel::Graph => {
                let i = self.graph_state.selected().unwrap_or(0);
                if i > 0 {
                    self.graph_state.select(Some(i - 1));
                }
            }
        }
    }

    fn scroll_down(&mut self) {
        match self.focused_panel {
            Panel::Activity => {
                let max = self.events.len().saturating_sub(1);
                let i = self.activity_state.selected().unwrap_or(0);
                if i < max {
                    self.activity_state.select(Some(i + 1));
                }
            }
            Panel::Insights => {
                let max = self.insights.len().saturating_sub(1);
                let i = self.insight_state.selected().unwrap_or(0);
                if i < max {
                    self.insight_state.select(Some(i + 1));
                }
            }
            Panel::Graph => {
                let max = self.graph_data.len().saturating_sub(1);
                let i = self.graph_state.selected().unwrap_or(0);
                if i < max {
                    self.graph_state.select(Some(i + 1));
                }
            }
        }
    }

    fn toggle_panel(&mut self) {
        match self.focused_panel {
            Panel::Activity => {
                self.focused_panel = Panel::Insights;
                if self.insight_state.selected().is_none() && !self.insights.is_empty() {
                    self.insight_state.select(Some(0));
                }
            }
            Panel::Insights => {
                self.focused_panel = Panel::Activity;
            }
            Panel::Graph => {
                self.focused_panel = Panel::Activity;
            }
        }
    }

    fn dismiss_insight(&mut self) {
        if self.focused_panel == Panel::Insights {
            if let Some(i) = self.insight_state.selected() {
                if i < self.insights.len() {
                    self.insights.remove(i);
                    if self.insights.is_empty() {
                        self.insight_state.select(None);
                    } else if i >= self.insights.len() {
                        self.insight_state.select(Some(self.insights.len() - 1));
                    }
                }
            }
        }
    }

    async fn toggle_graph(&mut self) {
        if self.focused_panel == Panel::Graph {
            self.focused_panel = Panel::Activity;
            return;
        }

        // Get the file path from the selected event's summary
        if let Some(evt) = self.selected_event() {
            // Try to extract a file path from the summary
            let file_path = extract_file_from_summary(&evt.summary);
            if let Some(path) = file_path {
                match send_request(Request::GetRelatedFiles {
                    file_path: path.clone(),
                })
                .await
                {
                    Ok(Response::RelatedFilesResult(entries)) => {
                        self.graph_data = entries;
                        self.graph_file = Some(path);
                        if !self.graph_data.is_empty() {
                            self.graph_state.select(Some(0));
                        }
                    }
                    _ => {
                        self.graph_data.clear();
                        self.graph_file = Some(path);
                    }
                }
            }
        }

        self.focused_panel = Panel::Graph;
    }

    fn selected_event(&self) -> Option<&EventSummary> {
        self.activity_state
            .selected()
            .and_then(|i| self.events.get(i))
    }
}

pub async fn run() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Install panic hook that restores terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let mut app = App::new();
    app.refresh_data().await;

    let tick_rate = Duration::from_secs(2);
    let mut last_tick = std::time::Instant::now();

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                        KeyCode::Char('j') | KeyCode::Down => app.scroll_down(),
                        KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
                        KeyCode::Tab => app.toggle_panel(),
                        KeyCode::Char('d') => app.dismiss_insight(),
                        KeyCode::Char('r') => app.refresh_data().await,
                        KeyCode::Char('g') => app.toggle_graph().await,
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.refresh_data().await;
            last_tick = std::time::Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let size = f.area();

    // If daemon is not running, show error message
    if app.daemon_error.is_some() && app.status.is_none() {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                " Daemon not running. Start with: cortexd ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    " Error: {} ",
                    app.daemon_error.as_deref().unwrap_or("unknown")
                ),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                " Press q to quit ",
                Style::default().fg(Color::Gray),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Ambient Cortex ")
                .style(Style::default().fg(Color::White)),
        );
        f.render_widget(msg, size);
        return;
    }

    // Main layout: top bar, body, bottom bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // top bar
            Constraint::Min(5),   // body
            Constraint::Length(3), // bottom bar
        ])
        .split(size);

    render_top_bar(f, outer[0], app);
    render_body(f, outer[1], app);
    render_bottom_bar(f, outer[2]);
}

fn render_top_bar(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let title = Paragraph::new(Line::from(vec![Span::styled(
        " Ambient Cortex ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    let status_text = if let Some(ref s) = app.status {
        let hours = s.uptime_secs / 3600;
        let mins = (s.uptime_secs % 3600) / 60;
        let secs = s.uptime_secs % 60;
        format!(
            "Uptime: {:02}:{:02}:{:02}  |  Events: {}  |  Insights: {}  |  Watchers: {}",
            hours,
            mins,
            secs,
            s.event_count,
            s.insight_count,
            s.watchers_active.len()
        )
    } else {
        "Connecting...".to_string()
    };

    let status = Paragraph::new(Line::from(Span::styled(
        format!(" {status_text} "),
        Style::default().fg(Color::Green),
    )))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(status, chunks[1]);
}

fn render_body(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    if app.focused_panel == Panel::Graph {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        render_activity_stream(f, chunks[0], app);
        render_graph(f, chunks[1], app);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        render_activity_stream(f, chunks[0], app);
        render_right_panel(f, chunks[1], app);
    }
}

fn event_type_color(event_type: &str) -> Color {
    match event_type.to_lowercase().as_str() {
        "error" | "err" => Color::Red,
        "git" | "git_commit" | "git_branch" | "git_push" => Color::Yellow,
        "file" | "file_change" | "file_open" | "file_save" => Color::Blue,
        "command" | "cmd" | "shell" | "shell_command" => Color::Green,
        _ => Color::White,
    }
}

fn insight_type_color(insight_type: &str) -> Color {
    match insight_type.to_lowercase().as_str() {
        "warning" | "warn" => Color::Red,
        "reminder" => Color::Yellow,
        "suggestion" => Color::Cyan,
        "history" => Color::White,
        _ => Color::Gray,
    }
}

fn render_activity_stream(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let focused = app.focused_panel == Panel::Activity;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .events
        .iter()
        .map(|evt| {
            let ts = evt.timestamp.format("%H:%M:%S").to_string();
            let color = event_type_color(&evt.event_type);
            let line = Line::from(vec![
                Span::styled(format!("{ts} "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("[{:^10}] ", evt.event_type),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}: ", evt.source),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(&evt.summary, Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = format!(" Activity Stream ({}) ", app.events.len());
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.activity_state);
}

fn render_right_panel(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_insights(f, chunks[0], app);
    render_quick_info(f, chunks[1], app);
}

fn render_insights(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let focused = app.focused_panel == Panel::Insights;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .insights
        .iter()
        .map(|ins| {
            let color = insight_type_color(&ins.insight_type);
            let line = Line::from(vec![
                Span::styled(
                    format!("[{:^10}] ", ins.insight_type),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:.0}% ", ins.relevance * 100.0),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(&ins.title, Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = format!(" Insights ({}) ", app.insights.len());
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.insight_state);
}

fn render_quick_info(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let content = if let Some(evt) = app.selected_event() {
        vec![
            Line::from(vec![
                Span::styled("Type: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    &evt.event_type,
                    Style::default()
                        .fg(event_type_color(&evt.event_type))
                        .bold(),
                ),
            ]),
            Line::from(vec![
                Span::styled("Source: ", Style::default().fg(Color::Gray)),
                Span::styled(&evt.source, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Time: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    evt.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                &evt.summary,
                Style::default().fg(Color::White),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "Select an event to view details",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Event Details ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn render_graph(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let border_style = Style::default().fg(Color::Cyan);

    let title = match &app.graph_file {
        Some(path) => {
            let short = path.rsplit('/').next().unwrap_or(path);
            format!(" File Graph: {} ({}) ", short, app.graph_data.len())
        }
        None => " File Graph ".to_string(),
    };

    if app.graph_data.is_empty() {
        let msg = if app.graph_file.is_some() {
            "No related files found for this file."
        } else {
            "Select an event and press 'g' to view file relationships."
        };
        let paragraph = Paragraph::new(Span::styled(msg, Style::default().fg(Color::DarkGray)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            );
        f.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .graph_data
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let connector = if i == app.graph_data.len() - 1 {
                "\u{2514}\u{2500}\u{2500}"
            } else {
                "\u{251c}\u{2500}\u{2500}"
            };
            let short_path = entry.path.rsplit('/').next().unwrap_or(&entry.path);
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", connector),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:^14}", entry.relation),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("({:.1}) ", entry.strength),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(short_path, Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.graph_state);
}

fn extract_file_from_summary(summary: &str) -> Option<String> {
    // Try to extract file path from summaries like "saved /path/to/file"
    if let Some(path) = summary.strip_prefix("saved ") {
        return Some(path.to_string());
    }
    // Check for paths with common extensions
    for word in summary.split_whitespace() {
        if word.contains('/') && (word.contains('.') || word.ends_with('/')) {
            return Some(word.to_string());
        }
    }
    None
}

fn render_bottom_bar(f: &mut ratatui::Frame, area: Rect) {
    let help = Line::from(vec![
        Span::styled(" q", Style::default().fg(Color::Yellow).bold()),
        Span::styled(" quit  ", Style::default().fg(Color::Gray)),
        Span::styled("j/k/", Style::default().fg(Color::Yellow).bold()),
        Span::styled(
            "\u{2191}\u{2193}",
            Style::default().fg(Color::Yellow).bold(),
        ),
        Span::styled(" scroll  ", Style::default().fg(Color::Gray)),
        Span::styled("Tab", Style::default().fg(Color::Yellow).bold()),
        Span::styled(" switch panel  ", Style::default().fg(Color::Gray)),
        Span::styled("d", Style::default().fg(Color::Yellow).bold()),
        Span::styled(" dismiss insight  ", Style::default().fg(Color::Gray)),
        Span::styled("r", Style::default().fg(Color::Yellow).bold()),
        Span::styled(" refresh  ", Style::default().fg(Color::Gray)),
        Span::styled("g", Style::default().fg(Color::Yellow).bold()),
        Span::styled(" graph", Style::default().fg(Color::Gray)),
    ]);

    let bar = Paragraph::new(help).block(Block::default().borders(Borders::ALL));
    f.render_widget(bar, area);
}

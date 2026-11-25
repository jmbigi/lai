//! tui.rs - Terminal User Interface for lai
//! Provides diff viewer, file tree, chat panel, and tool call log.
//! 100% local, zero cloud dependencies, privacy by design.

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Represents a file diff for display
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: PathBuf,
    pub old_content: String,
    pub new_content: String,
    pub hunks: Vec<DiffHunk>,
}

/// A single diff hunk
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

/// A single line in a diff
#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Add(String),
    Remove(String),
}

/// Chat message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String, // "user", "assistant", "system", "tool"
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallDisplay>>,
    pub timestamp: String,
}

/// Tool call for display
#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub success: bool,
}

/// File tree node
#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
    pub expanded: bool,
    pub depth: usize,
}

/// TUI application state
pub struct TuiApp {
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,

    // Panels
    pub file_tree: FileTreeState,
    pub chat_panel: ChatPanelState,
    pub diff_viewer: DiffViewerState,
    pub tool_log: ToolLogState,

    // Layout
    pub active_panel: ActivePanel,
    pub show_help: bool,

    // Data
    pub project_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivePanel {
    FileTree,
    Chat,
    Diff,
    ToolLog,
}

#[allow(clippy::derivable_impls)]
impl Default for ActivePanel {
    fn default() -> Self {
        ActivePanel::Chat
    }
}

/// File tree panel state
#[derive(Debug)]
pub struct FileTreeState {
    pub root: Option<FileNode>,
    pub list_state: ListState,
    pub filter: String,
    pub flat_nodes: Vec<(FileNode, bool)>, // (node, is_visible)
}

#[allow(clippy::derivable_impls)]
impl Default for FileTreeState {
    fn default() -> Self {
        Self {
            root: None,
            list_state: ListState::default(),
            filter: String::new(),
            flat_nodes: Vec::new(),
        }
    }
}

/// Chat panel state
#[derive(Debug, Default)]
pub struct ChatPanelState {
    pub messages: Vec<ChatMessage>,
    pub input_buffer: String,
    pub input_mode: bool,
    pub scroll_offset: usize,
    pub list_state: ListState,
}

/// Diff viewer state
#[derive(Debug, Default)]
pub struct DiffViewerState {
    pub diffs: Vec<FileDiff>,
    pub selected_diff: usize,
    pub scroll_offset: usize,
    pub list_state: ListState,
}

/// Tool call log state
#[derive(Debug, Default)]
pub struct ToolLogState {
    pub calls: Vec<ToolCallDisplay>,
    pub list_state: ListState,
    pub scroll_offset: usize,
}

impl TuiApp {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            terminal: None,
            file_tree: FileTreeState::default(),
            chat_panel: ChatPanelState::default(),
            diff_viewer: DiffViewerState::default(),
            tool_log: ToolLogState::default(),
            active_panel: ActivePanel::default(),
            show_help: false,
            project_root,
        }
    }

    /// Initialize and run the TUI
    pub async fn run(&mut self) -> Result<()> {
        self.build_file_tree()?;
        self.setup_terminal()?;
        self.run_event_loop().await?;
        self.restore_terminal()?;
        Ok(())
    }

    fn setup_terminal(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        self.terminal = Some(terminal);
        Ok(())
    }

    fn restore_terminal(&mut self) -> Result<()> {
        if let Some(mut terminal) = self.terminal.take() {
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            terminal.show_cursor()?;
        }
        Ok(())
    }

    fn build_file_tree(&mut self) -> Result<()> {
        let root = self.build_node(&self.project_root, 0)?;
        self.file_tree.root = Some(root);
        self.flatten_tree();
        self.file_tree.list_state.select(Some(0));
        Ok(())
    }

    fn build_node(&self, path: &Path, depth: usize) -> Result<FileNode> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".")
            .to_string();

        let is_dir = path.is_dir();
        let mut children = Vec::new();

        if is_dir {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let child_path = entry.path();
                    // Skip hidden and build directories
                    let child_name = child_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    if child_name.starts_with('.')
                        || matches!(
                            child_name,
                            "target" | "node_modules" | "__pycache__" | ".git" | "build" | "dist"
                        )
                    {
                        continue;
                    }
                    if let Ok(node) = self.build_node(&child_path, depth + 1) {
                        children.push(node);
                    }
                }
            }
            children.sort_by(|a, b| {
                // Directories first, then alphabetical
                match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.cmp(&b.name),
                }
            });
        }

        Ok(FileNode {
            name,
            path: path.to_path_buf(),
            is_dir,
            children,
            expanded: depth < 2, // Auto-expand first 2 levels
            depth,
        })
    }

    fn flatten_tree(&mut self) {
        self.file_tree.flat_nodes.clear();
        if let Some(root) = self.file_tree.root.clone() {
            Self::flatten_node_static(&root, &mut self.file_tree.flat_nodes);
        }
    }

    fn flatten_node_static(node: &FileNode, flat: &mut Vec<(FileNode, bool)>) {
        flat.push((node.clone(), true));
        if node.expanded {
            for child in &node.children {
                Self::flatten_node_static(child, flat);
            }
        }
    }

    async fn run_event_loop(&mut self) -> Result<()> {
        let mut last_tick = std::time::Instant::now();

        loop {
            // Draw - take terminal out temporarily to avoid borrow conflict
            if let Some(mut terminal) = self.terminal.take() {
                terminal.draw(|f| self.render(f))?;
                self.terminal = Some(terminal);
            }

            // Handle events with timeout for periodic updates
            let timeout = Duration::from_millis(100).saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key(key)? {
                        break; // Exit requested
                    }
                }
            }

            // Periodic updates
            if last_tick.elapsed() > Duration::from_secs(1) {
                last_tick = std::time::Instant::now();
                // Could update live data here (e.g., streaming chat)
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Global shortcuts
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Ok(true), // Exit
            (KeyModifiers::CONTROL, KeyCode::Char('h')) => {
                self.show_help = !self.show_help;
                return Ok(false);
            }
            (_, KeyCode::F(1)) => {
                self.show_help = !self.show_help;
                return Ok(false);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('1')) => {
                self.active_panel = ActivePanel::FileTree;
                return Ok(false);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('2')) => {
                self.active_panel = ActivePanel::Chat;
                return Ok(false);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('3')) => {
                self.active_panel = ActivePanel::Diff;
                return Ok(false);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('4')) => {
                self.active_panel = ActivePanel::ToolLog;
                return Ok(false);
            }
            _ => {}
        }

        // Panel-specific handling
        match self.active_panel {
            ActivePanel::FileTree => self.handle_file_tree_key(key),
            ActivePanel::Chat => self.handle_chat_key(key),
            ActivePanel::Diff => self.handle_diff_key(key),
            ActivePanel::ToolLog => self.handle_tool_log_key(key),
        }
    }

    fn handle_file_tree_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Up => self.file_tree_prev(),
            KeyCode::Down => self.file_tree_next(),
            KeyCode::Left => self.file_tree_collapse(),
            KeyCode::Right => self.file_tree_expand(),
            KeyCode::Enter => self.file_tree_open(),
            KeyCode::Char('/') => {
                self.file_tree.filter.clear();
                // Would enter filter mode
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_chat_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.chat_panel.input_mode {
            match key.code {
                KeyCode::Enter => {
                    if !self.chat_panel.input_buffer.trim().is_empty() {
                        self.send_chat_message()?;
                    }
                    self.chat_panel.input_mode = false;
                }
                KeyCode::Esc => {
                    self.chat_panel.input_mode = false;
                }
                KeyCode::Backspace => {
                    self.chat_panel.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.chat_panel.input_buffer.push(c);
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('i') => {
                    self.chat_panel.input_mode = true;
                }
                KeyCode::Up => self.chat_scroll_up(),
                KeyCode::Down => self.chat_scroll_down(),
                KeyCode::PageUp => self.chat_page_up(),
                KeyCode::PageDown => self.chat_page_down(),
                _ => {}
            }
        }
        Ok(false)
    }

    fn handle_diff_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Up => self.diff_prev(),
            KeyCode::Down => self.diff_next(),
            KeyCode::Left => self.diff_prev_file(),
            KeyCode::Right => self.diff_next_file(),
            KeyCode::PageUp => self.diff_page_up(),
            KeyCode::PageDown => self.diff_page_down(),
            _ => {}
        }
        Ok(false)
    }

    fn handle_tool_log_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Up => self.tool_log_prev(),
            KeyCode::Down => self.tool_log_next(),
            KeyCode::PageUp => self.tool_log_page_up(),
            KeyCode::PageDown => self.tool_log_page_down(),
            _ => {}
        }
        Ok(false)
    }

    // Navigation helpers
    fn file_tree_prev(&mut self) {
        let i = self.file_tree.list_state.selected().unwrap_or(0);
        if i > 0 {
            self.file_tree.list_state.select(Some(i - 1));
        }
    }

    fn file_tree_next(&mut self) {
        let i = self.file_tree.list_state.selected().unwrap_or(0);
        if i + 1 < self.file_tree.flat_nodes.len() {
            self.file_tree.list_state.select(Some(i + 1));
        }
    }

    fn file_tree_collapse(&mut self) {
        if let Some(i) = self.file_tree.list_state.selected() {
            if let Some((node, _)) = self.file_tree.flat_nodes.get_mut(i) {
                if node.is_dir && node.expanded {
                    node.expanded = false;
                    self.flatten_tree();
                }
            }
        }
    }

    fn file_tree_expand(&mut self) {
        if let Some(i) = self.file_tree.list_state.selected() {
            if let Some((node, _)) = self.file_tree.flat_nodes.get_mut(i) {
                if node.is_dir && !node.expanded {
                    node.expanded = true;
                    self.flatten_tree();
                }
            }
        }
    }

    fn file_tree_open(&mut self) {
        if let Some(i) = self.file_tree.list_state.selected() {
            if let Some((node, _)) = self.file_tree.flat_nodes.get(i) {
                if !node.is_dir {
                    // Load file content into diff viewer or chat
                    let _ = std::fs::read_to_string(&node.path);
                    // Could show in diff viewer or send to chat
                }
            }
        }
    }

    fn chat_scroll_up(&mut self) {
        if self.chat_panel.scroll_offset > 0 {
            self.chat_panel.scroll_offset -= 1;
        }
    }

    fn chat_scroll_down(&mut self) {
        self.chat_panel.scroll_offset += 1;
    }

    fn chat_page_up(&mut self) {
        self.chat_panel.scroll_offset = self.chat_panel.scroll_offset.saturating_sub(10);
    }

    fn chat_page_down(&mut self) {
        self.chat_panel.scroll_offset += 10;
    }

    fn diff_prev(&mut self) {
        if self.diff_viewer.scroll_offset > 0 {
            self.diff_viewer.scroll_offset -= 1;
        }
    }

    fn diff_next(&mut self) {
        self.diff_viewer.scroll_offset += 1;
    }

    fn diff_prev_file(&mut self) {
        if self.diff_viewer.selected_diff > 0 {
            self.diff_viewer.selected_diff -= 1;
            self.diff_viewer.scroll_offset = 0;
        }
    }

    fn diff_next_file(&mut self) {
        if self.diff_viewer.selected_diff + 1 < self.diff_viewer.diffs.len() {
            self.diff_viewer.selected_diff += 1;
            self.diff_viewer.scroll_offset = 0;
        }
    }

    fn diff_page_up(&mut self) {
        self.diff_viewer.scroll_offset = self.diff_viewer.scroll_offset.saturating_sub(20);
    }

    fn diff_page_down(&mut self) {
        self.diff_viewer.scroll_offset += 20;
    }

    fn tool_log_prev(&mut self) {
        let i = self.tool_log.list_state.selected().unwrap_or(0);
        if i > 0 {
            self.tool_log.list_state.select(Some(i - 1));
        }
    }

    fn tool_log_next(&mut self) {
        let i = self.tool_log.list_state.selected().unwrap_or(0);
        if i + 1 < self.tool_log.calls.len() {
            self.tool_log.list_state.select(Some(i + 1));
        }
    }

    fn tool_log_page_up(&mut self) {
        self.tool_log.scroll_offset = self.tool_log.scroll_offset.saturating_sub(10);
    }

    fn tool_log_page_down(&mut self) {
        self.tool_log.scroll_offset += 10;
    }

    fn send_chat_message(&mut self) -> Result<()> {
        let msg = self.chat_panel.input_buffer.trim().to_string();
        if msg.is_empty() {
            return Ok(());
        }

        self.chat_panel.messages.push(ChatMessage {
            role: "user".to_string(),
            content: msg.clone(),
            tool_calls: None,
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        });

        self.chat_panel.input_buffer.clear();
        self.chat_panel.scroll_offset = self.chat_panel.messages.len().saturating_sub(1);

        // Here you would send to the LLM and get response
        // For now, just echo
        self.chat_panel.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: format!("Echo: {}", msg),
            tool_calls: None,
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        });

        Ok(())
    }

    // ==================== RENDERING ====================

    fn render(&mut self, f: &mut ratatui::Frame) {
        let area = f.area();

        if self.show_help {
            self.render_help(f, area);
            return;
        }

        // Main layout: 3 columns (file tree | chat/diff | tool log)
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20), // File tree
                Constraint::Percentage(55), // Chat or Diff
                Constraint::Percentage(25), // Tool log
            ])
            .split(area);

        // File tree panel
        self.render_file_tree(f, main_chunks[0]);

        // Main content (chat or diff)
        match self.active_panel {
            ActivePanel::Chat => self.render_chat(f, main_chunks[1]),
            ActivePanel::Diff => self.render_diff(f, main_chunks[1]),
            _ => self.render_chat(f, main_chunks[1]),
        }

        // Tool log panel
        self.render_tool_log(f, main_chunks[2]);

        // Status bar at bottom
        let status_area = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };
        self.render_status_bar(f, status_area);
    }

    fn render_file_tree(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .file_tree
            .flat_nodes
            .iter()
            .map(|(node, _)| {
                let indent = "  ".repeat(node.depth);
                let icon = if node.is_dir {
                    if node.expanded {
                        "📂"
                    } else {
                        "📁"
                    }
                } else {
                    "📄"
                };
                let style = if node.is_dir {
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{} {} ", indent, icon),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(node.name.clone(), style),
                ]))
            })
            .collect();

        let title = if self.active_panel == ActivePanel::FileTree {
            "📁 Archivos [ACTIVO] "
        } else {
            "📁 Archivos "
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(if self.active_panel == ActivePanel::FileTree {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Gray)
                    }),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("► ");

        f.render_stateful_widget(list, area, &mut self.file_tree.list_state);
    }

    fn render_chat(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),   // Messages
                Constraint::Length(3), // Input
            ])
            .split(area);

        // Messages
        let messages: Vec<ListItem> = self
            .chat_panel
            .messages
            .iter()
            .rev()
            .map(|msg| {
                let role_color = match msg.role.as_str() {
                    "user" => Color::Green,
                    "assistant" => Color::Cyan,
                    "system" => Color::Yellow,
                    "tool" => Color::Magenta,
                    _ => Color::White,
                };

                let mut lines = vec![Line::from(vec![
                    Span::styled(
                        format!("[{}] ", msg.timestamp),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(
                        format!("{}: ", msg.role),
                        Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                    ),
                ])];

                for line in msg.content.lines() {
                    lines.push(Line::from(vec![Span::raw(format!("  {}", line))]));
                }

                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        lines.push(Line::from(vec![
                            Span::styled("  🔧 ", Style::default().fg(Color::Blue)),
                            Span::styled(&tc.name, Style::default().add_modifier(Modifier::BOLD)),
                            Span::raw(format!("({})", tc.arguments)),
                        ]));
                        if let Some(result) = &tc.result {
                            lines.push(Line::from(vec![
                                Span::styled("    → ", Style::default().fg(Color::Gray)),
                                Span::raw(if result.len() > 80 {
                                    format!("{}...", &result[..80])
                                } else {
                                    result.clone()
                                }),
                            ]));
                        }
                    }
                }

                ListItem::new(lines).style(Style::default().fg(Color::White))
            })
            .collect();

        let list = List::new(messages)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("💬 Chat [ACTIVO] ")
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
            .highlight_symbol("► ");

        f.render_widget(list, chunks[0]);

        // Input area
        let input_text = if self.chat_panel.input_mode {
            format!("> {}", self.chat_panel.input_buffer)
        } else {
            "Presiona 'i' para escribir mensaje".to_string()
        };

        let input_style = if self.chat_panel.input_mode {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let input = Paragraph::new(input_text)
            .style(input_style)
            .block(Block::default().borders(Borders::ALL).title("Input"));
        f.render_widget(input, chunks[1]);
    }

    fn render_diff(&mut self, f: &mut ratatui::Frame, area: Rect) {
        if self.diff_viewer.diffs.is_empty() {
            let empty = Paragraph::new(
                "No hay diffs para mostrar\n\nPresiona Ctrl+1 para seleccionar archivo en el árbol",
            )
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("📝 Diff Viewer"),
            );
            f.render_widget(empty, area);
            return;
        }

        let diff = &self.diff_viewer.diffs[self.diff_viewer.selected_diff];
        let path_str = diff.path.to_string_lossy();

        let mut lines = vec![
            Line::from(vec![
                Span::styled("📝 ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    path_str.as_ref(),
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Yellow),
                ),
                Span::styled(
                    format!(
                        " ({}/{})",
                        self.diff_viewer.selected_diff + 1,
                        self.diff_viewer.diffs.len()
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(""),
        ];

        for hunk in &diff.hunks {
            lines.push(Line::from(vec![Span::styled(
                format!(
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
                ),
                Style::default().fg(Color::Cyan),
            )]));

            for line in &hunk.lines {
                match line {
                    DiffLine::Context(text) => {
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(text, Style::default().fg(Color::Gray)),
                        ]));
                    }
                    DiffLine::Add(text) => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "+ ",
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(text, Style::default().fg(Color::Green)),
                        ]));
                    }
                    DiffLine::Remove(text) => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "- ",
                                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(text, Style::default().fg(Color::Red)),
                        ]));
                    }
                }
            }
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("📝 Diff Viewer [ACTIVO]"),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.diff_viewer.scroll_offset as u16, 0));

        f.render_widget(paragraph, area);
    }

    fn render_tool_log(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .tool_log
            .calls
            .iter()
            .rev()
            .map(|call| {
                let status_icon = if call.success { "✓" } else { "✗" };
                let status_color = if call.success {
                    Color::Green
                } else {
                    Color::Red
                };

                let mut lines = vec![Line::from(vec![
                    Span::styled(
                        format!("{} ", status_icon),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(
                        &call.name,
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Color::Cyan),
                    ),
                ])];

                if !call.arguments.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  args: ", Style::default().fg(Color::Gray)),
                        Span::raw(if call.arguments.len() > 60 {
                            format!("{}...", &call.arguments[..60])
                        } else {
                            call.arguments.clone()
                        }),
                    ]));
                }

                if let Some(result) = &call.result {
                    lines.push(Line::from(vec![
                        Span::styled("  → ", Style::default().fg(Color::Gray)),
                        Span::raw(if result.len() > 60 {
                            format!("{}...", &result[..60])
                        } else {
                            result.clone()
                        }),
                    ]));
                }

                ListItem::new(lines)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("🔧 Tool Calls")
                    .border_style(Style::default().fg(Color::Gray)),
            )
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
            .highlight_symbol("► ");

        f.render_stateful_widget(list, area, &mut self.tool_log.list_state);
    }

    fn render_status_bar(&self, f: &mut ratatui::Frame, area: Rect) {
        let panel_name = match self.active_panel {
            ActivePanel::FileTree => "Archivos",
            ActivePanel::Chat => "Chat",
            ActivePanel::Diff => "Diff",
            ActivePanel::ToolLog => "Tools",
        };

        let shortcuts =
            "Ctrl+1:Archivos  Ctrl+2:Chat  Ctrl+3:Diff  Ctrl+4:Tools  Ctrl+H:Ayuda  Ctrl+C:Salir";

        let status = Paragraph::new(format!(" lai | Panel: {} | {}", panel_name, shortcuts))
            .style(Style::default().fg(Color::White).bg(Color::Blue))
            .alignment(Alignment::Center);
        f.render_widget(status, area);
    }

    fn render_help(&self, f: &mut ratatui::Frame, area: Rect) {
        let help_text = vec![
            Line::from(vec![Span::styled(
                " lai TUI - Ayuda\n",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Cyan),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Navegación Global:\n",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )]),
            Line::from("  Ctrl+1    Panel de Archivos"),
            Line::from("  Ctrl+2    Panel de Chat"),
            Line::from("  Ctrl+3    Panel de Diff"),
            Line::from("  Ctrl+4    Panel de Tool Calls"),
            Line::from("  Ctrl+H    Mostrar/Ocultar esta ayuda"),
            Line::from("  Ctrl+C    Salir"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Panel de Archivos:\n",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )]),
            Line::from("  ↑/↓       Navegar"),
            Line::from("  ←/→       Colapsar/Expandir directorio"),
            Line::from("  Enter     Abrir archivo"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Panel de Chat:\n",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )]),
            Line::from("  i         Entrar en modo input"),
            Line::from("  Enter     Enviar mensaje (en modo input)"),
            Line::from("  Esc       Cancelar input"),
            Line::from("  ↑/↓       Scroll mensajes"),
            Line::from("  PgUp/PgDn Scroll página"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Panel de Diff:\n",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )]),
            Line::from("  ↑/↓       Scroll diff"),
            Line::from("  ←/→       Diff anterior/siguiente"),
            Line::from("  PgUp/PgDn Scroll página"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Panel de Tool Calls:\n",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )]),
            Line::from("  ↑/↓       Navegar calls"),
            Line::from("  PgUp/PgDn Scroll página"),
            Line::from(""),
            Line::from("Presiona 'h' o 'Esc' para cerrar"),
        ];

        let help = Paragraph::new(Text::from(help_text))
            .block(Block::default().borders(Borders::ALL).title(" Ayuda "))
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);
        f.render_widget(help, area);
    }
}

/// Parse a unified diff into FileDiff structure
pub fn parse_unified_diff(diff_text: &str) -> Vec<FileDiff> {
    let mut diffs = Vec::new();
    let mut current_diff: Option<FileDiff> = None;
    let mut current_hunk: Option<DiffHunk> = None;

    for line in diff_text.lines() {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            // New file diff
            if let Some(d) = current_diff.take() {
                if let Some(h) = current_hunk.take() {
                    current_diff.as_mut().unwrap().hunks.push(h);
                }
                diffs.push(d);
            }

            let rest = line
                .strip_prefix("--- ")
                .or_else(|| line.strip_prefix("+++ "))
                .unwrap_or("")
                .trim();
            let path = rest
                .strip_prefix("a/")
                .or_else(|| rest.strip_prefix("b/"))
                .unwrap_or(rest)
                .to_string();

            current_diff = Some(FileDiff {
                path: PathBuf::from(path),
                old_content: String::new(),
                new_content: String::new(),
                hunks: Vec::new(),
            });
        } else if line.starts_with("@@ ") {
            // New hunk
            if let Some(h) = current_hunk.take() {
                current_diff.as_mut().unwrap().hunks.push(h);
            }

            // Parse @@ -old_start,old_lines +new_start,new_lines @@
            let parts: Vec<&str> = line.split_whitespace().collect();
            let (old_start, old_lines) = parse_range(parts.get(1).unwrap_or(&"-1,1"));
            let (new_start, new_lines) = parse_range(parts.get(2).unwrap_or(&"+1,1"));

            current_hunk = Some(DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
            });
        } else if let Some(hunk) = &mut current_hunk {
            if let Some(text) = line.strip_prefix(' ') {
                hunk.lines.push(DiffLine::Context(text.to_string()));
            } else if let Some(text) = line.strip_prefix('+') {
                hunk.lines.push(DiffLine::Add(text.to_string()));
            } else if let Some(text) = line.strip_prefix('-') {
                hunk.lines.push(DiffLine::Remove(text.to_string()));
            }
        }
    }

    if let Some(h) = current_hunk.take() {
        if let Some(d) = current_diff.as_mut() {
            d.hunks.push(h);
        }
    }
    if let Some(d) = current_diff {
        diffs.push(d);
    }

    diffs
}

fn parse_range(s: &str) -> (usize, usize) {
    let s = s.trim_start_matches(['+', '-']);
    let parts: Vec<&str> = s.split(',').collect();
    let start = parts[0].parse().unwrap_or(1);
    let lines = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    (start, lines)
}

/// Add a tool call to the log
pub fn log_tool_call(
    tool_log: &mut ToolLogState,
    name: &str,
    arguments: &str,
    result: Option<String>,
    success: bool,
) {
    tool_log.calls.push(ToolCallDisplay {
        name: name.to_string(),
        arguments: arguments.to_string(),
        result,
        success,
    });
    if tool_log.calls.len() > 1000 {
        tool_log.calls.drain(0..500);
    }
}

/// Add a chat message
pub fn log_chat_message(
    chat: &mut ChatPanelState,
    role: &str,
    content: &str,
    tool_calls: Option<Vec<ToolCallDisplay>>,
) {
    chat.messages.push(ChatMessage {
        role: role.to_string(),
        content: content.to_string(),
        tool_calls,
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
    });
    if chat.messages.len() > 500 {
        chat.messages.drain(0..250);
    }
}

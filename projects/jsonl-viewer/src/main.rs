use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use serde_json::Value;
use std::{
    fs,
    io::{stdout},
};
use std::collections::HashMap;

#[derive(Clone)]
struct JsonlEntry {
    content: String,
    parsed: Option<Value>,
    valid: bool,
    timestamp: Option<String>,
    entry_type: Option<String>,
    recent_message: Option<String>,
}

#[derive(Default)]
struct EntryStats {
    total: usize,
    valid: usize,
    invalid: usize,
    by_type: HashMap<String, usize>,
}

struct App {
    entries: Vec<JsonlEntry>,
    selected_index: usize,
    scroll_offset: usize,
    show_only_invalid: bool,
    stats: EntryStats,
}

impl App {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            show_only_invalid: false,
            stats: EntryStats::default(),
        }
    }

    fn load_jsonl(&mut self, file_path: &str) -> Result<()> {
        let content = fs::read_to_string(file_path)?;
        let mut stats = EntryStats::default();
        
        self.entries = content
            .lines()
            .enumerate()
            .map(|(_i, line)| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    stats.total += 1;
                    JsonlEntry {
                        content: String::new(),
                        parsed: None,
                        valid: true,
                        timestamp: None,
                        entry_type: None,
                        recent_message: None,
                    }
                } else {
                    match serde_json::from_str::<Value>(trimmed) {
                        Ok(value) => {
                            let (timestamp, entry_type, recent_message) = extract_entry_metadata(&value);
                            stats.total += 1;
                            stats.valid += 1;
                            if let Some(ref et) = entry_type {
                                *stats.by_type.entry(et.clone()).or_insert(0) += 1;
                            }
                            JsonlEntry {
                                content: line.to_string(),
                                parsed: Some(value),
                                valid: true,
                                timestamp,
                                entry_type,
                                recent_message,
                            }
                        }
                        Err(_) => {
                            stats.total += 1;
                            stats.invalid += 1;
                            JsonlEntry {
                                content: line.to_string(),
                                parsed: None,
                                valid: false,
                                timestamp: None,
                                entry_type: None,
                                recent_message: None,
                            }
                        }
                    }
                }
            })
            .collect();
        
        self.stats = stats;
        if !self.entries.is_empty() {
            self.selected_index = 0;
        }
        Ok(())
    }

    fn next_entry(&mut self) {
        let filtered_entries = self.get_filtered_entries();
        if !filtered_entries.is_empty() {
            if let Some(pos) = filtered_entries.iter().position(|&i| i == self.selected_index) {
                if pos + 1 < filtered_entries.len() {
                    self.selected_index = filtered_entries[pos + 1];
                }
            }
        }
        self.scroll_offset = 0;
    }

    fn previous_entry(&mut self) {
        let filtered_entries = self.get_filtered_entries();
        if !filtered_entries.is_empty() {
            if let Some(pos) = filtered_entries.iter().position(|&i| i == self.selected_index) {
                if pos > 0 {
                    self.selected_index = filtered_entries[pos - 1];
                }
            }
        }
        self.scroll_offset = 0;
    }

    fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn toggle_invalid(&mut self) {
        self.show_only_invalid = !self.show_only_invalid;
        if self.show_only_invalid {
            if let Some(first_invalid) = self.get_filtered_entries().first() {
                self.selected_index = *first_invalid;
            }
        }
    }

    fn get_filtered_entries(&self) -> Vec<usize> {
        if self.show_only_invalid {
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| !entry.valid)
                .map(|(i, _)| i)
                .collect()
        } else {
            (0..self.entries.len()).collect()
        }
    }
}

fn extract_entry_metadata(value: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let timestamp = value.get("timestamp")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let entry_type = value.get("type")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("kind")
            .and_then(|v| v.as_str()))
        .or_else(|| value.get("role")
            .and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    
    // Extract the most recent message content
    let message_content = extract_recent_message(value);
    
    (timestamp, entry_type, message_content)
}

fn extract_recent_message(value: &Value) -> Option<String> {
    // Try different common patterns for message content
    
    // 1. Direct content field
    if let Some(content) = value.get("content")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty()) {
        return Some(truncate_message(content));
    }
    
    // 2. Message field
    if let Some(message) = value.get("message")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty()) {
        return Some(truncate_message(message));
    }
    
    // 3. Nested content in choices
    if let Some(choices) = value.get("choices").and_then(|v| v.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(content) = choice.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty()) {
                return Some(truncate_message(content));
            }
        }
    }
    
    // 4. Delta content in streaming responses
    if let Some(choices) = value.get("choices").and_then(|v| v.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(content) = choice.get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty()) {
                return Some(truncate_message(content));
            }
        }
    }
    
    // 5. Response field
    if let Some(response) = value.get("response")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty()) {
        return Some(truncate_message(response));
    }
    
    // 6. Text field
    if let Some(text) = value.get("text")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty()) {
        return Some(truncate_message(text));
    }
    
    None
}

fn truncate_message(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.len() <= 60 {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..57])
    }
}

fn draw_entry_list(entries: &[JsonlEntry], selected: usize, show_only_invalid: bool) -> Vec<Line> {
    let filtered_entries: Vec<usize> = if show_only_invalid {
        entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.valid)
            .map(|(i, _)| i)
            .collect()
    } else {
        (0..entries.len()).collect()
    };

    filtered_entries
        .iter()
        .map(|&i| {
            let entry = &entries[i];
            let prefix = if i == selected { "> " } else { "  " };
            
            let mut parts = vec![];
            
            // Show recent message content first, if available
            if let Some(ref recent_msg) = entry.recent_message {
                parts.push(recent_msg.clone());
            }
            
            // Then show timestamp and type
            if let Some(ref timestamp) = entry.timestamp {
                // Show only time portion without date
                let time_part = if timestamp.contains('T') {
                    // Extract time part from ISO format like "2024-10-20T14:30:45"
                    timestamp.split('T').nth(1).unwrap_or(timestamp)
                } else if timestamp.contains(' ') {
                    // Extract time part from format like "2024-10-20 14:30:45"
                    timestamp.split(' ').nth(1).unwrap_or(timestamp)
                } else {
                    timestamp
                };
                // Keep only HH:MM:SS (hour, minute, second)
                let short_timestamp = {
                    let parts: Vec<&str> = time_part.split(':').collect();
                    if parts.len() >= 3 {
                        // Split the third part to extract just seconds (before any decimal)
                        let seconds_part = parts[2].split('.').next().unwrap_or(parts[2]);
                        format!("{}:{}:{}", parts[0], parts[1], seconds_part)
                    } else if parts.len() >= 2 {
                        format!("{}:{}", parts[0], parts[1])
                    } else {
                        time_part.to_string()
                    }
                };
                parts.push(format!("[{}]", short_timestamp));
            }
            
            if let Some(ref entry_type) = entry.entry_type {
                parts.push(format!("({})", entry_type));
            }
            
            let preview = if !parts.is_empty() {
                format!("{}", parts.join(" "))
            } else {
                entry.content.chars().take(50).collect::<String>()
            };
            
            let style = if entry.valid {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };
            
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(preview, style),
            ])
        })
        .collect()
}

fn draw_detail(entry: &JsonlEntry) -> String {
    if let Some(ref parsed) = entry.parsed {
        match serde_json::to_string_pretty(parsed) {
            Ok(formatted) => formatted,
            Err(_) => entry.content.clone(),
        }
    } else {
        entry.content.clone()
    }
}

fn draw_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(f.size());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    // Determine visible height of the entry list (excluding borders)
    let list_height = if main_chunks[0].height > 2 {
        (main_chunks[0].height - 2) as usize
    } else {
        0
    };

    // Compute scroll offset so selected entry is always visible
    let entry_scroll = if app.selected_index + 1 > list_height {
        app.selected_index + 1 - list_height
    } else {
        0
    };

    // Create entry list with only visible entries
    let entry_lines = draw_entry_list(&app.entries, app.selected_index, app.show_only_invalid);
    
    let entry_list_widget = Paragraph::new(entry_lines)
        .block(Block::default().borders(Borders::ALL).title("Entries"))
        .scroll((entry_scroll as u16, 0));
    f.render_widget(entry_list_widget, main_chunks[0]);

    let detail_content = if app.selected_index < app.entries.len() {
        draw_detail(&app.entries[app.selected_index])
    } else {
        String::new()
    };
    let detail_widget = Paragraph::new(detail_content)
        .block(Block::default().borders(Borders::ALL).title("Content"))
        .wrap(Wrap { trim: true })
        .scroll((app.scroll_offset as u16, 0));
    f.render_widget(detail_widget, main_chunks[1]);

    let stats = format!(
        "Total: {} | Valid: {} | Invalid: {} | Types: {}",
        app.stats.total,
        app.stats.valid,
        app.stats.invalid,
        app.stats.by_type.len()
    );
    let keys_widget = Paragraph::new(stats)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(keys_widget, chunks[2]);
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <jsonl_file>", args[0]);
        return Ok(());
    }

    let mut app = App::new();
    app.load_jsonl(&args[1])?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    loop {
        terminal.draw(|f| draw_ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Esc => break,
                KeyCode::Char('j') => app.next_entry(),
                KeyCode::Char('k') => app.previous_entry(),
                KeyCode::Down => app.next_entry(),
                KeyCode::Up => app.previous_entry(),
                KeyCode::Char('d') => app.scroll_down(),
                KeyCode::Char('u') => app.scroll_up(),
                KeyCode::Char('i') => app.toggle_invalid(),
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
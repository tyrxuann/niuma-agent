//! Logs view rendering.
//!
//! Renders the execution logs interface.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::{App, LogLevel};

/// Renders the logs view.
pub fn render_logs(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Execution Logs ")
        .title_style(Style::default().fg(Color::Blue));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if app.logs.is_empty() {
        render_empty_state(frame, inner_area);
        return;
    }

    render_log_entries(frame, inner_area, app);
}

/// Renders the log entries.
fn render_log_entries(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .logs
        .iter()
        .map(|log| {
            let level_style = match log.level {
                LogLevel::Info => Style::default().fg(Color::Green),
                LogLevel::Warn => Style::default().fg(Color::Yellow),
                LogLevel::Error => Style::default().fg(Color::Red),
                LogLevel::Debug => Style::default().fg(Color::Gray),
            };

            let level = Span::styled(
                format!("[{:<5}] ", log.level.display()),
                level_style.add_modifier(Modifier::BOLD),
            );
            let message = Span::styled(&log.message, Style::default().fg(Color::White));

            ListItem::new(Line::from(vec![level, message]))
        })
        .collect();

    let list = List::new(items).style(Style::default().fg(Color::White));

    frame.render_widget(list, area);
}

/// Renders the empty state when no logs exist.
fn render_empty_state(frame: &mut Frame, area: Rect) {
    let empty_text =
        Paragraph::new("No logs yet.\n\nLogs will appear here when tasks are executed.")
            .style(Style::default().fg(Color::Gray));

    frame.render_widget(empty_text, area);
}

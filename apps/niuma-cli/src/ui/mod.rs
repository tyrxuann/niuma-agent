//! UI rendering module.
//!
//! This module provides rendering functionality for all application views.

mod chat;
mod logs;
mod tasks;

pub use chat::render_chat;
pub use logs::render_logs;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};
pub use tasks::render_tasks;

use crate::app::{App, View};

/// Renders the complete application UI.
pub fn render(app: &mut App, frame: &mut Frame) {
    let size = frame.area();

    // Create main layout: title, content, status bar
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Status bar
        ])
        .split(size);

    render_title_bar(frame, main_layout[0]);

    // Split main content into sidebar and content area
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12), // Sidebar
            Constraint::Min(40),    // Content
        ])
        .split(main_layout[1]);

    render_sidebar(frame, content_layout[0], app.current_view);

    // Render the current view
    match app.current_view {
        View::Chat => render_chat(frame, content_layout[1], app),
        View::Tasks => render_tasks(frame, content_layout[1], app),
        View::Logs => render_logs(frame, content_layout[1], app),
    }

    render_status_bar(frame, main_layout[2], app);
}

/// Renders the title bar at the top of the application.
fn render_title_bar(frame: &mut Frame, area: Rect) {
    let title = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan))
        .title(" NIUMA AGENT ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(title, area);
}

/// Renders the sidebar with navigation tabs.
fn render_sidebar(frame: &mut Frame, area: Rect, current_view: View) {
    let titles: Vec<Line> = vec![
        Line::from(Span::styled(
            "  [Chat]  ",
            view_style(View::Chat, current_view),
        )),
        Line::from(Span::styled(
            " [Tasks]  ",
            view_style(View::Tasks, current_view),
        )),
        Line::from(Span::styled(
            "  [Logs]  ",
            view_style(View::Logs, current_view),
        )),
    ];

    let sidebar = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Views ")
                .title_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(sidebar, area);
}

/// Returns the style for a view based on whether it's currently selected.
fn view_style(view: View, current_view: View) -> Style {
    if view == current_view {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    }
}

/// Renders the status bar at the bottom of the application.
fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    // Split status bar into sections
    let status_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(10), // Version
            Constraint::Length(14), // Connection status
            Constraint::Length(12), // Task count
            Constraint::Min(10),    // Spacer
            Constraint::Length(12), // Help hint
        ])
        .split(area);

    let version =
        Paragraph::new(format!(" {} ", app.version)).style(Style::default().fg(Color::Yellow));

    let connection = Paragraph::new(if app.connected {
        " Connected "
    } else {
        " Disconnected "
    })
    .style(Style::default().fg(if app.connected {
        Color::Green
    } else {
        Color::Red
    }));

    let tasks = Paragraph::new(format!(" Tasks: {} ", app.active_task_count()))
        .style(Style::default().fg(Color::Cyan));

    let help = Paragraph::new(" ? help ").style(Style::default().fg(Color::Gray));

    let clear_hint = Paragraph::new(" Ctrl+L clear ").style(Style::default().fg(Color::Gray));

    frame.render_widget(version, status_layout[0]);
    frame.render_widget(connection, status_layout[1]);
    frame.render_widget(tasks, status_layout[2]);
    frame.render_widget(help, status_layout[4]);
    frame.render_widget(clear_hint, status_layout[3]);
}

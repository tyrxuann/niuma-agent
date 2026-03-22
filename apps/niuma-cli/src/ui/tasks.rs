//! Tasks view rendering.
//!
//! Renders the scheduled tasks management interface.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::App;

/// Renders the tasks view.
pub fn render_tasks(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Scheduled Tasks ")
        .title_style(Style::default().fg(Color::Magenta));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if app.tasks.is_empty() {
        render_empty_state(frame, inner_area);
        return;
    }

    render_task_list(frame, inner_area, app);
}

/// Renders the task list.
fn render_task_list(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let status_icon = if task.enabled { "[✓]" } else { "[ ]" };
            let status_color = if task.enabled {
                Color::Green
            } else {
                Color::Gray
            };

            let status = Span::styled(format!("{status_icon} "), Style::default().fg(status_color));
            let name = Span::styled(
                format!("{:<25}", task.name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );
            let schedule = Span::styled(
                format!("  {}", task.schedule),
                Style::default().fg(Color::Cyan),
            );

            ListItem::new(Line::from(vec![status, name, schedule]))
        })
        .collect();

    let list = List::new(items).style(Style::default().fg(Color::White));

    frame.render_widget(list, area);
}

/// Renders the empty state when no tasks exist.
fn render_empty_state(frame: &mut Frame, area: Rect) {
    let empty_text = Paragraph::new(
        "No scheduled tasks.\n\nComplete a task and save it to create a scheduled task.",
    )
    .style(Style::default().fg(Color::Gray));

    frame.render_widget(empty_text, area);
}

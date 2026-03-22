//! Chat view rendering.
//!
//! Renders the main chat interface with message history and input field.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::app::{App, MessageSender};

/// Renders the chat view.
pub fn render_chat(frame: &mut Frame, area: Rect, app: &App) {
    // Split into message area and input area
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Messages
            Constraint::Length(3), // Input
        ])
        .split(area);

    render_messages(frame, layout[0], app);
    render_input(frame, layout[1], app);
}

/// Renders the message history.
fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Chat ")
        .title_style(Style::default().fg(Color::Green));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Build message lines
    let lines: Vec<Line> = app
        .messages
        .iter()
        .flat_map(|msg| {
            let prefix_style = match msg.sender {
                MessageSender::Agent => Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                MessageSender::User => Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            };

            let content_style = Style::default().fg(Color::White);

            let prefix = Span::styled(format!("{}: ", msg.sender.prefix()), prefix_style);
            let content = Span::styled(&msg.content, content_style);

            vec![Line::from(vec![prefix, content])]
        })
        .collect();

    let messages_widget = Paragraph::new(lines);
    frame.render_widget(messages_widget, inner_area);

    // Add scrollbar if needed
    if app.messages.len() > inner_area.height as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

        let mut scrollbar_state = ScrollbarState::new(app.messages.len()).position(
            app.messages
                .len()
                .saturating_sub(inner_area.height as usize),
        );

        frame.render_stateful_widget(scrollbar, inner_area, &mut scrollbar_state);
    }
}

/// Renders the input field.
fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(" Input ")
        .title_style(Style::default().fg(Color::Yellow));

    let inner_area = input_block.inner(area);
    frame.render_widget(input_block, area);

    // Create input line with cursor
    let input_text = if app.input.is_empty() {
        Span::styled("> _", Style::default().fg(Color::Gray))
    } else {
        Span::raw(format!("> {}", app.input))
    };

    let input_widget = Paragraph::new(Line::from(input_text));
    frame.render_widget(input_widget, inner_area);

    // Position cursor at end of input
    #[allow(clippy::cast_possible_truncation)]
    let cursor_x = inner_area.x + 2 + app.input.len() as u16;
    let cursor_y = inner_area.y;
    frame.set_cursor_position((cursor_x.min(inner_area.right().saturating_sub(1)), cursor_y));
}

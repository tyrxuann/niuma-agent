//! Command line interface for niuma agent.
//!
//! Niuma is an AI-powered task assistant that combines LLM reasoning
//! with MCP tools to help you get things done.

mod app;
mod error;
mod event;
mod terminal;
mod ui;

use std::time::Duration;

use clap::Parser;

use crate::{app::App, error::CliResult, event::EventHandler, terminal::TerminalGuard, ui::render};

/// Niuma - An AI-powered task assistant.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Run in TUI mode (default: true)
    #[arg(short, long, default_value = "true")]
    tui: bool,
}

fn main() -> CliResult<()> {
    let args = Args::parse();

    if args.tui {
        run_tui()?;
    } else {
        run_cli();
    }

    Ok(())
}

/// Runs the CLI in non-TUI mode.
fn run_cli() {
    println!("Niuma CLI - AI Task Assistant");
    println!("Use --tui flag for interactive TUI mode (default)");
}

/// Runs the TUI application.
fn run_tui() -> CliResult<()> {
    // Set up panic hook to restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Attempt to restore terminal state
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    // Initialize terminal
    let mut terminal_guard = TerminalGuard::new()?;

    // Create app and event handler
    let mut app = App::new();
    let events = EventHandler::new(Duration::from_millis(100));

    // Main event loop
    while !app.should_quit {
        // Draw UI
        terminal_guard
            .terminal()
            .draw(|frame| render(&mut app, frame))?;

        // Handle input
        let event = events.next()?;
        app.handle_event(event);
    }

    Ok(())
}

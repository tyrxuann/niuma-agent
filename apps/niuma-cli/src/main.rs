//! Command line interface for niuma agent.
//!
//! Niuma is an AI-powered task assistant that combines LLM reasoning
//! with MCP tools to help you get things done.

mod agent;
mod app;
mod error;
mod event;
mod terminal;
mod ui;

use std::{sync::Arc, time::Duration};

use clap::Parser;
use tracing::info;

use crate::{
    agent::AgentEngine, app::App, error::CliResult, event::EventHandler, terminal::TerminalGuard,
    ui::render,
};

/// Niuma - An AI-powered task assistant.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Run in TUI mode (default: true)
    #[arg(short, long, default_value = "true")]
    tui: bool,
}

fn main() -> CliResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();

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

    // Create agent engine
    let agent = create_agent_engine();

    // Create app and event handler
    let mut app = App::with_agent(Arc::clone(&agent));
    let events = EventHandler::new(Duration::from_millis(100));

    // Main event loop
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create runtime");

    runtime.block_on(async {
        while !app.should_quit {
            // Process any pending messages
            app.process_pending().await;

            // Draw UI
            terminal_guard
                .terminal()
                .draw(|frame| render(&mut app, frame))
                .expect("Failed to draw");

            // Handle input
            let event = events.next().expect("Failed to read event");
            app.handle_event(event);
        }
    });

    info!("Application shutting down");
    Ok(())
}

/// Creates the agent engine with configured LLM provider.
fn create_agent_engine() -> Arc<AgentEngine> {
    // Try to get API key from environment
    let api_key = std::env::var("CLAUDE_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .unwrap_or_else(|_| {
            info!("No API key found, using mock mode");
            "mock-api-key".to_string()
        });

    let llm_provider: Arc<dyn niuma_llm::LLMProvider> =
        Arc::new(niuma_llm::ClaudeProvider::new(&api_key));

    info!(provider = llm_provider.name(), "Agent engine initialized");
    Arc::new(AgentEngine::new(llm_provider))
}

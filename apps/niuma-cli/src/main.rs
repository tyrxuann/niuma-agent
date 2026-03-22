//! Command line interface for niuma agent.
//!
//! Niuma is an AI-powered task assistant that combines LLM reasoning
//! with MCP tools to help you get things done.

mod agent;
mod app;
mod config;
mod error;
mod event;
mod logger;
mod terminal;
mod ui;

use std::{path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use tracing::info;

use crate::{
    agent::AgentEngine, app::App, config::Config, error::CliResult, event::EventHandler,
    terminal::TerminalGuard, ui::render,
};

/// Niuma - An AI-powered task assistant.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Run in TUI mode (default: true)
    #[arg(short, long, default_value = "true")]
    tui: bool,

    /// Path to configuration file.
    /// If not specified, searches in order:
    /// ./config.yaml, ~/.config/niuma/config.yaml
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
}

fn main() -> CliResult<()> {
    let args = Args::parse();

    // Load configuration
    let config_path = args.config.unwrap_or_else(Config::default_path);
    let config = Config::load(&config_path)
        .map_err(|e| {
            eprintln!("Warning: Failed to load config: {}. Using defaults.", e);
            Config::default()
        })
        .unwrap_or_default();

    // Initialize logging to file
    if let Err(e) = crate::logger::init_logging(&config) {
        eprintln!(
            "Warning: Failed to initialize file logging: {}. Logs will not be saved.",
            e
        );
    }

    info!(
        config_path = %config_path.display(),
        "Configuration loaded"
    );

    if args.tui {
        run_tui(&config)?;
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
fn run_tui(config: &Config) -> CliResult<()> {
    // Set up panic hook to restore terminal BEFORE any potential panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal state immediately
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen);
        // Print panic info to stderr
        eprintln!("\nPanic occurred: {}", panic_info);
        original_hook(panic_info);
    }));

    // Create agent engine before entering alternate screen
    let agent = create_agent_engine(config);

    info!(provider = agent.provider_name(), "Agent engine initialized");

    // Initialize terminal (enters alternate screen and raw mode)
    let mut terminal_guard = TerminalGuard::new()?;

    // Create app and event handler
    let mut app = App::with_agent(Arc::clone(&agent));
    let events = EventHandler::new(Duration::from_millis(100));

    // Main event loop
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| crate::error::CliError::TuiInit(e.to_string()))?;

    runtime.block_on(async {
        while !app.should_quit {
            // Process any pending messages
            app.process_pending().await;

            // Draw UI
            if let Err(e) = terminal_guard
                .terminal()
                .draw(|frame| render(&mut app, frame))
            {
                info!(error = %e, "Draw error, exiting TUI loop");
                break;
            }

            // Handle input
            match events.next() {
                Ok(event) => app.handle_event(event),
                Err(e) => {
                    info!(error = %e, "Input error, exiting TUI loop");
                    break;
                }
            }
        }
    });

    info!("Application shutting down");
    // TerminalGuard drop will restore terminal automatically
    Ok(())
}

/// Creates the agent engine with configured LLM provider.
fn create_agent_engine(config: &Config) -> Arc<AgentEngine> {
    // Get provider config from config file, or fall back to environment
    let api_key = config
        .llm
        .default_provider()
        .ok()
        .and_then(|p| {
            if p.api_key.is_empty() || p.api_key.starts_with("${") {
                None
            } else {
                Some(p.api_key.clone())
            }
        })
        .or_else(|| std::env::var("CLAUDE_API_KEY").ok())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .unwrap_or_else(|| {
            info!("No API key found, using mock mode");
            "mock-api-key".to_string()
        });

    let model = config
        .llm
        .default_provider()
        .ok()
        .and_then(|p| p.model.clone())
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    info!(model = %model, "Using LLM model");

    let llm_provider: Arc<dyn niuma_llm::LLMProvider> =
        Arc::new(niuma_llm::ClaudeProvider::new(&api_key));

    Arc::new(AgentEngine::new(llm_provider))
}

//! Command line interface for niuma agent.

use clap::Parser;

/// Niuma - An AI-powered task assistant.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Run in TUI mode
    #[arg(short, long)]
    tui: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.tui {
        run_tui()?;
    } else {
        run_cli()?;
    }

    Ok(())
}

fn run_cli() -> anyhow::Result<()> {
    println!("Niuma CLI - AI Task Assistant");
    println!("Use --tui flag for interactive TUI mode");
    Ok(())
}

fn run_tui() -> anyhow::Result<()> {
    // TUI implementation will be added later
    println!("TUI mode - coming soon");
    Ok(())
}

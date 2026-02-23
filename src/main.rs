#![allow(dead_code)]

mod app;
mod config;
mod discovery;
mod git;
mod ui;
mod util;
mod watch;

use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing::info;

use app::App;
use config::AppConfig;

#[derive(Parser, Debug)]
#[command(
    name = "diffdon",
    version,
    about = "A TUI tool for reviewing Git diffs"
)]
struct Cli {
    /// Path to the repository (default: current directory)
    #[arg(long, short = 'C')]
    repo: Option<PathBuf>,

    /// Print the default configuration and exit
    #[arg(long)]
    print_default_config: bool,

    /// Print the current key bindings and exit
    #[arg(long)]
    print_keys: bool,

    /// Config file path
    #[arg(long)]
    config: Option<PathBuf>,

    /// Log file path (default: stderr if RUST_LOG is set)
    #[arg(long)]
    log_file: Option<PathBuf>,
}

fn setup_logging(log_file: Option<PathBuf>) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    if let Some(path) = log_file {
        let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let filename = path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("diffdon.log"));
        let file_appender = tracing_appender::rolling::never(dir, filename);
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(file_appender)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(io::stderr)
            .init();
    }
}

fn setup_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        default_hook(info);
        tracing::error!("Panic: {}", info);
    }));
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.print_default_config {
        let config = AppConfig::default();
        println!("{}", toml::to_string_pretty(&config)?);
        return Ok(());
    }

    if cli.print_keys {
        app::print_keys();
        return Ok(());
    }

    setup_logging(cli.log_file);
    setup_panic_hook();

    info!("Starting diffdon");

    let config = if let Some(ref path) = cli.config {
        AppConfig::load_from(path).context("Failed to load config")?
    } else {
        AppConfig::load_default()
    };

    let repo_path = cli
        .repo
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    let result = App::new(config, repo_path).run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

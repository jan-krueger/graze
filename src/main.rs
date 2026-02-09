use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use graze::app::App;

#[derive(Parser)]
#[command(name = "graze", about = "TUI for exploring tabular data files")]
struct Cli {
    /// Path to a data file (CSV, Parquet, JSON)
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let path = cli
        .file
        .canonicalize()
        .with_context(|| format!("File not found: {}", cli.file.display()))?;

    let terminal = ratatui::init();
    let mut app = App::new(path)?;
    let result = app.run(terminal);
    ratatui::restore();
    result
}

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use graze::app::App;
use graze::ui::theme::Theme;

#[derive(Parser)]
#[command(name = "graze", about = "TUI for exploring tabular data files")]
struct Cli {
    /// Paths to data files (CSV, Parquet, JSON)
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let paths: Vec<PathBuf> = cli
        .files
        .iter()
        .map(|f| {
            f.canonicalize()
                .with_context(|| format!("File not found: {}", f.display()))
        })
        .collect::<Result<_>>()?;

    let theme = Theme::detect();
    let terminal = ratatui::init();
    let mut app = App::new(paths, theme)?;
    let result = app.run(terminal);
    ratatui::restore();
    result
}

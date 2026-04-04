use std::io::{self, IsTerminal};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use graze::app::App;
use graze::diff_app::DiffApp;
use graze::ui::theme::Theme;

#[derive(Parser)]
#[command(name = "graze", about = "TUI for exploring tabular data files")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Paths to data files (CSV, Parquet, JSON)
    files: Vec<PathBuf>,

    /// Format for stdin data (csv, tsv, json, parquet). Defaults to csv.
    #[arg(short = 'F', long)]
    format: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Compare two data files side by side
    Diff {
        /// First data file
        file_a: PathBuf,
        /// Second data file
        file_b: PathBuf,
        /// Key columns for matching rows (comma-separated)
        #[arg(long, value_delimiter = ',')]
        key: Option<Vec<String>>,
        /// Columns to compare (comma-separated, defaults to all non-key columns)
        #[arg(long, value_delimiter = ',')]
        cols: Option<Vec<String>>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Diff {
            file_a,
            file_b,
            key,
            cols,
        }) => {
            let path_a = file_a
                .canonicalize()
                .with_context(|| format!("File not found: {}", file_a.display()))?;
            let path_b = file_b
                .canonicalize()
                .with_context(|| format!("File not found: {}", file_b.display()))?;

            let theme = Theme::detect();
            let terminal = ratatui::init();
            crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
            let mut app = DiffApp::new(path_a, path_b, key, cols, theme)?;
            let result = app.run(terminal);
            crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture).ok();
            ratatui::restore();
            result
        }
        None => {
            let (paths, stdin_mode) = if cli.files.is_empty() {
                if io::stdin().is_terminal() {
                    anyhow::bail!(
                        "No files specified. Usage: graze <files...> or graze diff <file_a> <file_b>"
                    );
                }
                // Read stdin into a temp file
                let ext = match cli.format.as_deref() {
                    Some("tsv") => "tsv",
                    Some("json") => "json",
                    Some("parquet") => "parquet",
                    Some("csv") | None => "csv",
                    Some(other) => anyhow::bail!("Unknown format: {other}. Use csv, tsv, json, or parquet."),
                };
                let mut tmp = tempfile::Builder::new()
                    .prefix("graze-stdin-")
                    .suffix(&format!(".{ext}"))
                    .tempfile()
                    .context("Failed to create temp file for stdin")?;
                io::copy(&mut io::stdin().lock(), &mut tmp)
                    .context("Failed to read stdin")?;
                let path = tmp.into_temp_path();
                let kept = path.keep().context("Failed to persist temp file")?;
                (vec![kept], true)
            } else {
                let paths: Vec<PathBuf> = cli
                    .files
                    .iter()
                    .map(|f| {
                        f.canonicalize()
                            .with_context(|| format!("File not found: {}", f.display()))
                    })
                    .collect::<Result<_>>()?;
                (paths, false)
            };

            let theme = Theme::detect();
            let terminal = ratatui::init();
            crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
            let mut app = App::new(paths, theme)?;
            if stdin_mode {
                app.stdin_label = true;
            }
            let result = app.run(terminal);
            crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture).ok();
            ratatui::restore();

            // Print marked rows as CSV to stdout on quit
            if let Some(csv) = app.marked_row_csv() {
                print!("{csv}");
            }

            result
        }
    }
}

mod backend;
mod csv;
mod json;
mod parquet;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;

use crate::event::SortState;

pub use self::csv::CsvProvider;
pub use self::json::JsonProvider;
pub use self::parquet::ParquetProvider;

pub trait DataProvider: Send {
    fn name(&self) -> &str;
    fn table_name(&self) -> &str;
    fn schema(&self) -> Arc<Schema>;
    fn total_rows(&self) -> usize;
    fn fetch_page(&self, offset: usize, limit: usize) -> Result<RecordBatch>;
    fn apply_sort_state(&mut self, state: &SortState) -> Result<()>;
    fn apply_filter(&mut self, filter: &str) -> Result<usize>;
    fn reset_filters(&mut self) -> Result<usize>;
    fn execute_sql(&self, sql: &str) -> Result<RecordBatch>;
}

pub fn create_provider(path: &Path) -> Result<Box<dyn DataProvider>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "csv" | "tsv" => Ok(Box::new(CsvProvider::new(path)?)),
        "parquet" => Ok(Box::new(ParquetProvider::new(path)?)),
        "json" | "jsonl" | "ndjson" => Ok(Box::new(JsonProvider::new(path)?)),
        _ => anyhow::bail!("Unsupported file extension: .{ext}"),
    }
}

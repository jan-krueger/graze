mod backend;
mod formats;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;

use crate::event::SortState;

use self::backend::DuckDbBackend;

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
    fn collect_match_rows(&self, term: &str, is_regex: bool) -> Result<Vec<usize>>;

    /// Populate the Arrow cache after TABLE materialization. No-op by default.
    fn materialize_cache(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct FileProvider {
    format_name: &'static str,
    backend: DuckDbBackend,
}

impl DataProvider for FileProvider {
    fn name(&self) -> &str {
        self.format_name
    }

    fn table_name(&self) -> &str {
        self.backend.table_name()
    }

    fn schema(&self) -> Arc<Schema> {
        self.backend.schema()
    }

    fn total_rows(&self) -> usize {
        self.backend.total_rows()
    }

    fn fetch_page(&self, offset: usize, limit: usize) -> Result<RecordBatch> {
        self.backend.fetch_page(offset, limit)
    }

    fn apply_sort_state(&mut self, state: &SortState) -> Result<()> {
        self.backend.apply_sort_state(state)
    }

    fn apply_filter(&mut self, filter: &str) -> Result<usize> {
        self.backend.apply_filter(filter)
    }

    fn reset_filters(&mut self) -> Result<usize> {
        self.backend.reset_filters()
    }

    fn execute_sql(&self, sql: &str) -> Result<RecordBatch> {
        self.backend.execute_sql(sql)
    }

    fn collect_match_rows(&self, term: &str, is_regex: bool) -> Result<Vec<usize>> {
        self.backend.collect_match_rows(term, is_regex)
    }

    fn materialize_cache(&mut self) -> Result<()> {
        self.backend.populate_cache()
    }
}

pub fn create_provider(path: &Path) -> Result<Box<dyn DataProvider>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "csv" | "tsv" => Ok(Box::new(formats::csv(path)?)),
        "parquet" => Ok(Box::new(formats::parquet(path)?)),
        "json" | "jsonl" | "ndjson" => Ok(Box::new(formats::json(path)?)),
        _ => anyhow::bail!("Unsupported file extension: .{ext}"),
    }
}

/// Create a VIEW-based provider for instant startup (queries hit the raw file).
pub fn create_provider_quick(path: &Path) -> Result<Box<dyn DataProvider>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "csv" | "tsv" => Ok(Box::new(formats::csv_quick(path)?)),
        "parquet" => Ok(Box::new(formats::parquet_quick(path)?)),
        "json" | "jsonl" | "ndjson" => Ok(Box::new(formats::json_quick(path)?)),
        _ => anyhow::bail!("Unsupported file extension: .{ext}"),
    }
}

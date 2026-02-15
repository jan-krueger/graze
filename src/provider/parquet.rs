use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::Connection;

use super::backend::DuckDbBackend;
use crate::event::SortState;
use crate::provider::DataProvider;

pub struct ParquetProvider {
    backend: DuckDbBackend,
}

impl ParquetProvider {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let path_str = path.to_string_lossy();
        let load_sql =
            format!("CREATE TABLE parquet_data AS SELECT * FROM read_parquet('{path_str}')");
        let backend = DuckDbBackend::new(conn, &load_sql, "parquet_data")?;
        Ok(Self { backend })
    }

    pub fn new_quick(path: &Path) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let path_str = path.to_string_lossy();
        let load_sql =
            format!("CREATE VIEW parquet_data AS SELECT * FROM read_parquet('{path_str}')");
        let backend = DuckDbBackend::new_without_count(conn, &load_sql, "parquet_data")?;
        Ok(Self { backend })
    }
}

impl DataProvider for ParquetProvider {
    fn name(&self) -> &str {
        "Parquet"
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

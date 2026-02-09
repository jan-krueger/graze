use std::sync::Arc;

use anyhow::{Context, Result};
use duckdb::arrow::array::RecordBatch;
use duckdb::arrow::compute::concat_batches;
use duckdb::arrow::datatypes::Schema;
use duckdb::Connection;

use crate::event::SortState;

pub struct DuckDbBackend {
    conn: Connection,
    table_name: String,
    schema: Arc<Schema>,
    total_rows: usize,
    sort_state: SortState,
    current_filter: Option<String>,
}

impl DuckDbBackend {
    pub fn new(conn: Connection, load_sql: &str, table_name: &str) -> Result<Self> {
        conn.execute_batch(load_sql)
            .context("Failed to load file into DuckDB")?;

        let schema = Self::query_schema(&conn, table_name)?;
        let total_rows = Self::query_count(&conn, table_name, None)?;

        Ok(Self {
            conn,
            table_name: table_name.to_string(),
            schema,
            total_rows,
            sort_state: SortState::default(),
            current_filter: None,
        })
    }

    fn query_schema(conn: &Connection, table_name: &str) -> Result<Arc<Schema>> {
        let sql = format!("SELECT * FROM {table_name} LIMIT 0");
        let mut stmt = conn.prepare(&sql)?;
        let arrow = stmt.query_arrow([])?;
        Ok(arrow.get_schema())
    }

    fn query_count(conn: &Connection, table_name: &str, filter: Option<&str>) -> Result<usize> {
        let sql = if let Some(f) = filter {
            format!("SELECT COUNT(*) FROM {table_name} WHERE {f}")
        } else {
            format!("SELECT COUNT(*) FROM {table_name}")
        };
        let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn schema(&self) -> Arc<Schema> {
        self.schema.clone()
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub fn fetch_page(&self, offset: usize, limit: usize) -> Result<RecordBatch> {
        let mut sql = format!("SELECT * FROM {}", self.table_name);

        if let Some(ref filter) = self.current_filter {
            sql.push_str(&format!(" WHERE {filter}"));
        }

        if let Some(order_clause) = self.sort_state.to_sql() {
            sql.push_str(&format!(" ORDER BY {order_clause}"));
        }

        sql.push_str(&format!(" LIMIT {limit} OFFSET {offset}"));

        let mut stmt = self.conn.prepare(&sql)?;
        let arrow = stmt.query_arrow([])?;
        let schema = arrow.get_schema();
        let batches: Vec<RecordBatch> = arrow.collect();

        if batches.is_empty() {
            Ok(RecordBatch::new_empty(schema))
        } else {
            concat_batches(&schema, &batches).context("Failed to concat batches")
        }
    }

    pub fn apply_sort_state(&mut self, state: &SortState) -> Result<()> {
        self.sort_state = state.clone();
        Ok(())
    }

    pub fn apply_filter(&mut self, filter: &str) -> Result<usize> {
        let count = Self::query_count(&self.conn, &self.table_name, Some(filter))
            .context("Invalid filter expression")?;
        self.current_filter = Some(filter.to_string());
        self.total_rows = count;
        Ok(count)
    }

    pub fn reset_filters(&mut self) -> Result<usize> {
        self.current_filter = None;
        self.total_rows = Self::query_count(&self.conn, &self.table_name, None)?;
        Ok(self.total_rows)
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn execute_sql(&self, sql: &str) -> Result<RecordBatch> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .context("Failed to prepare SQL query")?;
        let arrow = stmt.query_arrow([])?;
        let schema = arrow.get_schema();
        let batches: Vec<RecordBatch> = arrow.collect();

        if batches.is_empty() {
            Ok(RecordBatch::new_empty(schema))
        } else {
            concat_batches(&schema, &batches).context("Failed to concat SQL result batches")
        }
    }
}

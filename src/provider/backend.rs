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

    /// Build a SQL condition that matches any column against the search term.
    fn build_match_clause(&self, term: &str, is_regex: bool) -> Option<String> {
        let fields = self.schema.fields();
        if fields.is_empty() {
            return None;
        }
        let escaped = term.replace('\'', "''");
        let conditions: Vec<String> = fields
            .iter()
            .map(|f| {
                let col = format!("\"{}\"", f.name());
                if is_regex {
                    format!("regexp_matches({}::VARCHAR, '(?i){}')", col, escaped)
                } else {
                    format!("{}::VARCHAR ILIKE '%{}%'", col, escaped)
                }
            })
            .collect();
        Some(conditions.join(" OR "))
    }

    /// Build the base query (SELECT * FROM table with filter + sort).
    fn build_base_query(&self) -> String {
        let mut base = format!("SELECT * FROM {}", self.table_name);
        if let Some(ref filter) = self.current_filter {
            base.push_str(&format!(" WHERE {filter}"));
        }
        if let Some(order_clause) = self.sort_state.to_sql() {
            base.push_str(&format!(" ORDER BY {order_clause}"));
        }
        base
    }

    /// Count how many matches exist at or before the given row (1-based index).
    pub fn match_index_at(&self, term: &str, row: usize, is_regex: bool) -> Result<usize> {
        let match_clause = match self.build_match_clause(term, is_regex) {
            Some(c) => c,
            None => return Ok(0),
        };
        let base = self.build_base_query();
        let sql = format!(
            "WITH numbered AS (SELECT *, ROW_NUMBER() OVER () - 1 AS _rn FROM ({base})) \
             SELECT COUNT(*) FROM numbered WHERE ({match_clause}) AND _rn <= {row}"
        );
        let count: i64 = self.conn.query_row(&sql, [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn count_matches(&self, term: &str, is_regex: bool) -> Result<usize> {
        let match_clause = match self.build_match_clause(term, is_regex) {
            Some(c) => c,
            None => return Ok(0),
        };
        let mut sql = format!("SELECT COUNT(*) FROM {}", self.table_name);
        sql.push_str(" WHERE ");
        if let Some(ref filter) = self.current_filter {
            sql.push_str(&format!("({filter}) AND "));
        }
        sql.push_str(&format!("({match_clause})"));
        let count: i64 = self.conn.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn find_match_row(
        &self,
        term: &str,
        current_row: usize,
        forward: bool,
        is_regex: bool,
    ) -> Result<Option<usize>> {
        let match_clause = match self.build_match_clause(term, is_regex) {
            Some(c) => c,
            None => return Ok(None),
        };
        let base = self.build_base_query();

        // First: directional search from current position
        let (dir_cmp, dir_order) = if forward {
            (">", "ASC")
        } else {
            ("<", "DESC")
        };

        let directional_sql = format!(
            "WITH numbered AS (SELECT *, ROW_NUMBER() OVER () - 1 AS _rn FROM ({base})) \
             SELECT _rn FROM numbered WHERE ({match_clause}) AND _rn {dir_cmp} {current_row} \
             ORDER BY _rn {dir_order} LIMIT 1"
        );

        if let Some(row) = self.query_single_usize(&directional_sql)? {
            return Ok(Some(row));
        }

        // Wrap-around: search from the other end
        let (wrap_cmp, wrap_order) = if forward {
            ("<=", "ASC")
        } else {
            (">=", "DESC")
        };

        let wrap_sql = format!(
            "WITH numbered AS (SELECT *, ROW_NUMBER() OVER () - 1 AS _rn FROM ({base})) \
             SELECT _rn FROM numbered WHERE ({match_clause}) AND _rn {wrap_cmp} {current_row} \
             ORDER BY _rn {wrap_order} LIMIT 1"
        );

        self.query_single_usize(&wrap_sql)
    }

    fn query_single_usize(&self, sql: &str) -> Result<Option<usize>> {
        let result: std::result::Result<i64, _> =
            self.conn.query_row(sql, [], |row| row.get(0));
        match result {
            Ok(v) => Ok(Some(v as usize)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
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

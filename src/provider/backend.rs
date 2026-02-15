use std::sync::Arc;

use anyhow::{Context, Result};
use duckdb::arrow::array::RecordBatch;
use duckdb::arrow::compute::concat_batches;
use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::util::display::ArrayFormatter;
use duckdb::Connection;

use crate::event::SortState;
use crate::search::SearchMatcher;

pub struct DuckDbBackend {
    conn: Connection,
    table_name: String,
    schema: Arc<Schema>,
    total_rows: usize,
    sort_state: SortState,
    current_filter: Option<String>,
    cached_data: Option<RecordBatch>,
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
            cached_data: None,
        })
    }

    /// Create a backend without counting rows (for VIEW-based quick startup).
    /// total_rows is set to 0, meaning "unknown".
    pub fn new_without_count(conn: Connection, load_sql: &str, table_name: &str) -> Result<Self> {
        conn.execute_batch(load_sql)
            .context("Failed to load file into DuckDB")?;

        let schema = Self::query_schema(&conn, table_name)?;

        Ok(Self {
            conn,
            table_name: table_name.to_string(),
            schema,
            total_rows: 0,
            sort_state: SortState::default(),
            current_filter: None,
            cached_data: None,
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

    /// Fetch all rows from DuckDB and store as a single cached RecordBatch.
    pub fn populate_cache(&mut self) -> Result<()> {
        let base = self.build_base_query();
        let mut stmt = self.conn.prepare(&base)?;
        let arrow = stmt.query_arrow([])?;
        let schema = arrow.get_schema();
        let batches: Vec<RecordBatch> = arrow.collect();

        let batch = if batches.is_empty() {
            RecordBatch::new_empty(schema)
        } else {
            concat_batches(&schema, &batches).context("Failed to concat batches for cache")?
        };

        self.total_rows = batch.num_rows();
        self.cached_data = Some(batch);
        Ok(())
    }

    pub fn fetch_page(&self, offset: usize, limit: usize) -> Result<RecordBatch> {
        // Cache-first: zero-copy slice
        if let Some(cache) = &self.cached_data {
            let total = cache.num_rows();
            if offset >= total {
                return Ok(RecordBatch::new_empty(self.schema.clone()));
            }
            let len = limit.min(total - offset);
            return Ok(cache.slice(offset, len));
        }

        // Fallback: SQL path (VIEW phase)
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
        if self.cached_data.is_some() {
            self.populate_cache()?;
        }
        Ok(())
    }

    pub fn apply_filter(&mut self, filter: &str) -> Result<usize> {
        // Validate the filter first via COUNT
        let count = Self::query_count(&self.conn, &self.table_name, Some(filter))
            .context("Invalid filter expression")?;
        self.current_filter = Some(filter.to_string());
        self.total_rows = count;
        if self.cached_data.is_some() {
            self.populate_cache()?;
        }
        Ok(self.total_rows)
    }

    pub fn reset_filters(&mut self) -> Result<usize> {
        self.current_filter = None;
        self.total_rows = Self::query_count(&self.conn, &self.table_name, None)?;
        if self.cached_data.is_some() {
            self.populate_cache()?;
        }
        Ok(self.total_rows)
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
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

    /// Collect all matching row indices.
    /// When cache exists, uses Rust-native string scan.
    /// Falls back to SQL when cache is None (VIEW phase).
    pub fn collect_match_rows(&self, term: &str, is_regex: bool) -> Result<Vec<usize>> {
        if let Some(cache) = &self.cached_data {
            return self.collect_match_rows_cached(cache, term, is_regex);
        }
        self.collect_match_rows_sql(term, is_regex)
    }

    /// Rust-native search over the cached RecordBatch.
    fn collect_match_rows_cached(
        &self,
        cache: &RecordBatch,
        term: &str,
        is_regex: bool,
    ) -> Result<Vec<usize>> {
        let matcher = match SearchMatcher::new(term, is_regex) {
            Some(m) => m,
            None => return Ok(Vec::new()),
        };

        let num_rows = cache.num_rows();
        let num_cols = cache.num_columns();

        // Build formatters for each column
        let formatters: Vec<Option<ArrayFormatter<'_>>> = (0..num_cols)
            .map(|i| {
                ArrayFormatter::try_new(cache.column(i).as_ref(), &Default::default()).ok()
            })
            .collect();

        let mut matches = Vec::new();
        for row in 0..num_rows {
            for (col_idx, fmt_opt) in formatters.iter().enumerate() {
                if cache.column(col_idx).is_null(row) {
                    continue;
                }
                if let Some(fmt) = fmt_opt {
                    let val = fmt.value(row).to_string();
                    if matcher.is_match(&val) {
                        matches.push(row);
                        break;
                    }
                }
            }
        }

        Ok(matches)
    }

    /// SQL-based search (used during VIEW phase when cache is not available).
    fn collect_match_rows_sql(&self, term: &str, is_regex: bool) -> Result<Vec<usize>> {
        let match_clause = match self.build_match_clause(term, is_regex) {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };
        let base = self.build_base_query();
        let sql = format!(
            "WITH numbered AS (SELECT *, ROW_NUMBER() OVER () - 1 AS _rn FROM ({base})) \
             SELECT _rn FROM numbered WHERE ({match_clause}) ORDER BY _rn ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map([], |row| {
                let v: i64 = row.get(0)?;
                Ok(v as usize)
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
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
}

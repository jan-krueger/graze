use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use duckdb::arrow::array::{Array, AsArray, RecordBatch};
use duckdb::arrow::datatypes::{DataType, Schema};
use duckdb::Connection;

use crate::state::{DiffCounts, DiffMarker};

/// Page of diff data fetched on demand from DuckDB.
pub struct DiffPageData {
    pub display_batch: RecordBatch,
    pub b_side_batch: RecordBatch,
    pub changed_cells: Vec<Vec<bool>>,
    /// Maps batch row index → data row index.
    pub row_indices: Vec<usize>,
}

/// Persistent DuckDB connection holding the diff join result as a TABLE.
/// Data is fetched page-by-page instead of materializing the entire result.
pub struct DiffBackend {
    conn: Connection,
    pub schema: Arc<Schema>,
    pub markers: Vec<DiffMarker>,
    pub counts: DiffCounts,
    pub b_side_col_map: Vec<Option<usize>>,
    pub total_rows: usize,
    pub changed_row_indices: Vec<usize>,
    // Internal: SQL fragments for building queries
    display_col_sql: String,
    b_side_col_sql: String,
    num_display_cols: usize,
    // For computing changed_cells: maps (display_col_idx, b_side_batch_col_idx)
    b_col_pairs: Vec<(usize, usize)>,
}

fn load_sql_for_path(path: &Path, table_name: &str) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let path_str = path.to_string_lossy();
    let read_fn = match ext.as_str() {
        "csv" | "tsv" => "read_csv_auto",
        "parquet" => "read_parquet",
        "json" | "jsonl" | "ndjson" => "read_json_auto",
        _ => anyhow::bail!("Unsupported file extension: .{ext}"),
    };
    Ok(format!(
        "CREATE TABLE {table_name} AS SELECT * FROM {read_fn}('{path_str}')"
    ))
}

impl DiffBackend {
    pub fn compute(
        path_a: &Path,
        path_b: &Path,
        key_columns: &[String],
        diff_columns: &[String],
    ) -> Result<Self> {
        let conn = Connection::open_in_memory().context("Failed to open DuckDB connection")?;

        let sql_a = load_sql_for_path(path_a, "table_a")?;
        let sql_b = load_sql_for_path(path_b, "table_b")?;

        conn.execute_batch(&sql_a)
            .context("Failed to load file A")?;
        conn.execute_batch(&sql_b)
            .context("Failed to load file B")?;

        // Get schema of table_a to know all columns
        let schema_a = {
            let mut stmt = conn.prepare("SELECT * FROM table_a LIMIT 0")?;
            let arrow = stmt.query_arrow([])?;
            arrow.get_schema()
        };

        let all_columns: Vec<String> = schema_a
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();

        // Build the SELECT clause
        let mut select_parts = Vec::new();

        // Row number for indexed access
        select_parts.push("ROW_NUMBER() OVER () - 1 AS _rownum".to_string());

        // Key columns: COALESCE(a.key, b.key) — cast to VARCHAR
        for key in key_columns {
            select_parts.push(format!(
                "COALESCE(a.\"{key}\"::VARCHAR, b.\"{key}\"::VARCHAR) AS \"{key}\""
            ));
        }

        // Non-key columns: COALESCE(a.col, b.col) — cast to VARCHAR
        for col in &all_columns {
            if key_columns.contains(col) {
                continue;
            }
            select_parts.push(format!(
                "COALESCE(a.\"{col}\"::VARCHAR, b.\"{col}\"::VARCHAR) AS \"{col}\""
            ));
        }

        // B-side diff columns for comparison
        for col in diff_columns {
            select_parts.push(format!("b.\"{col}\" AS \"_b_{col}\""));
        }

        // _diff CASE expression
        let mut diff_conditions = Vec::new();
        for col in diff_columns {
            diff_conditions.push(format!(
                "a.\"{col}\"::VARCHAR IS DISTINCT FROM b.\"{col}\"::VARCHAR"
            ));
        }
        let changed_expr = if diff_conditions.is_empty() {
            "FALSE".to_string()
        } else {
            diff_conditions.join(" OR ")
        };

        let key_null_checks_a: Vec<String> = key_columns
            .iter()
            .map(|k| format!("a.\"{k}\" IS NULL"))
            .collect();
        let key_null_checks_b: Vec<String> = key_columns
            .iter()
            .map(|k| format!("b.\"{k}\" IS NULL"))
            .collect();

        let case_expr = format!(
            "CASE WHEN {a_null} THEN 'only_b' WHEN {b_null} THEN 'only_a' WHEN {changed} THEN 'changed' ELSE 'common' END AS _diff",
            a_null = key_null_checks_a.join(" AND "),
            b_null = key_null_checks_b.join(" AND "),
            changed = changed_expr,
        );
        select_parts.push(case_expr);

        // JOIN condition
        let join_conds: Vec<String> = key_columns
            .iter()
            .map(|k| format!("a.\"{k}\"::VARCHAR = b.\"{k}\"::VARCHAR"))
            .collect();
        let join_on = join_conds.join(" AND ");

        // ORDER BY key columns
        let order_parts: Vec<String> = key_columns
            .iter()
            .map(|k| format!("COALESCE(a.\"{k}\"::VARCHAR, b.\"{k}\"::VARCHAR)"))
            .collect();
        let order_by = order_parts.join(", ");

        let create_sql = format!(
            "CREATE TABLE diff_result AS SELECT {select} FROM table_a a FULL OUTER JOIN table_b b ON {join} ORDER BY {order}",
            select = select_parts.join(", "),
            join = join_on,
            order = order_by,
        );

        conn.execute_batch(&create_sql)
            .context("Failed to create diff result table")?;

        // Drop source tables to free memory
        conn.execute_batch("DROP TABLE table_a; DROP TABLE table_b")
            .context("Failed to drop source tables")?;

        // Get the full schema of diff_result
        let full_schema = {
            let mut stmt = conn.prepare("SELECT * FROM diff_result LIMIT 0")?;
            let arrow = stmt.query_arrow([])?;
            arrow.get_schema()
        };

        // Build display column list (without _rownum, _b_*, _diff)
        let display_col_indices: Vec<usize> = full_schema
            .fields()
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                let name = f.name();
                name != "_diff" && name != "_rownum" && !name.starts_with("_b_")
            })
            .map(|(i, _)| i)
            .collect();

        let display_fields: Vec<_> = display_col_indices
            .iter()
            .map(|&i| full_schema.fields()[i].clone())
            .collect();
        let display_schema = Arc::new(Schema::new(display_fields));
        let num_display_cols = display_col_indices.len();

        // Build display column SQL
        let display_col_names: Vec<String> = display_col_indices
            .iter()
            .map(|&i| format!("\"{}\"", full_schema.fields()[i].name()))
            .collect();
        let display_col_sql = display_col_names.join(", ");

        // Build b-side column mapping
        let b_col_map: Vec<(usize, String)> = diff_columns
            .iter()
            .filter_map(|col| {
                let b_name = format!("_b_{col}");
                // Verify it exists in schema
                full_schema.fields().iter().position(|f| f.name() == &b_name)?;
                let a_idx = display_col_indices
                    .iter()
                    .position(|&i| full_schema.fields()[i].name() == col)?;
                Some((a_idx, b_name))
            })
            .collect();

        let b_side_col_names: Vec<String> = b_col_map.iter().map(|(_, name)| format!("\"{name}\"")).collect();
        let b_side_col_sql = if b_side_col_names.is_empty() {
            String::new()
        } else {
            b_side_col_names.join(", ")
        };

        // b_col_pairs: (display_idx, b_side_batch_col_idx)
        let b_col_pairs: Vec<(usize, usize)> = b_col_map
            .iter()
            .enumerate()
            .map(|(b_idx, (a_idx, _))| (*a_idx, b_idx))
            .collect();

        // b_side_col_map: for each display col, Some(idx in b_side_batch) or None
        let b_side_col_map: Vec<Option<usize>> = (0..num_display_cols)
            .map(|display_idx| {
                b_col_pairs
                    .iter()
                    .position(|&(a_idx, _)| a_idx == display_idx)
                    .map(|pos| b_col_pairs[pos].1)
            })
            .collect();

        // Phase 1: Scan only _diff column for markers
        let mut stmt = conn
            .prepare("SELECT _diff FROM diff_result ORDER BY _rownum")
            .context("Failed to prepare marker scan")?;
        let arrow = stmt.query_arrow([])?;
        let batches: Vec<RecordBatch> = arrow.collect();

        let mut markers = Vec::new();
        let mut counts = DiffCounts::default();
        let mut changed_row_indices = Vec::new();

        let mut row_idx = 0usize;
        for batch in &batches {
            let diff_col = batch.column(0);
            let num_rows = batch.num_rows();

            for row in 0..num_rows {
                let marker_str = if diff_col.is_null(row) {
                    "common"
                } else {
                    match diff_col.data_type() {
                        DataType::Utf8 => {
                            let arr = diff_col.as_string::<i32>();
                            arr.value(row)
                        }
                        DataType::LargeUtf8 => {
                            let arr = diff_col.as_string::<i64>();
                            arr.value(row)
                        }
                        _ => "common",
                    }
                };

                let marker = match marker_str {
                    "only_a" => {
                        counts.only_a += 1;
                        changed_row_indices.push(row_idx);
                        DiffMarker::OnlyA
                    }
                    "only_b" => {
                        counts.only_b += 1;
                        changed_row_indices.push(row_idx);
                        DiffMarker::OnlyB
                    }
                    "changed" => {
                        counts.changed += 1;
                        changed_row_indices.push(row_idx);
                        DiffMarker::Changed
                    }
                    _ => {
                        counts.common += 1;
                        DiffMarker::Common
                    }
                };
                markers.push(marker);
                row_idx += 1;
            }
        }

        let total_rows = markers.len();

        Ok(DiffBackend {
            conn,
            schema: display_schema,
            markers,
            counts,
            b_side_col_map,
            total_rows,
            changed_row_indices,
            display_col_sql,
            b_side_col_sql,
            num_display_cols,
            b_col_pairs,
        })
    }

    fn compute_changed_cells(
        display_batch: &RecordBatch,
        b_side_batch: &RecordBatch,
        row_indices: &[usize],
        markers: &[DiffMarker],
        b_col_pairs: &[(usize, usize)],
        num_display_cols: usize,
    ) -> Vec<Vec<bool>> {
        let num_rows = display_batch.num_rows();
        let display_formatters: Vec<Option<duckdb::arrow::util::display::ArrayFormatter>> =
            (0..display_batch.num_columns())
                .map(|i| {
                    duckdb::arrow::util::display::ArrayFormatter::try_new(
                        display_batch.column(i).as_ref(),
                        &Default::default(),
                    )
                    .ok()
                })
                .collect();
        let b_formatters: Vec<Option<duckdb::arrow::util::display::ArrayFormatter>> =
            (0..b_side_batch.num_columns())
                .map(|i| {
                    duckdb::arrow::util::display::ArrayFormatter::try_new(
                        b_side_batch.column(i).as_ref(),
                        &Default::default(),
                    )
                    .ok()
                })
                .collect();

        let mut changed_cells = Vec::with_capacity(num_rows);
        for batch_row in 0..num_rows {
            let data_row = row_indices[batch_row];
            let mut row_changes = vec![false; num_display_cols];

            if data_row < markers.len() && markers[data_row] == DiffMarker::Changed {
                for &(display_idx, b_batch_idx) in b_col_pairs {
                    let a_col = display_batch.column(display_idx);
                    let b_col = b_side_batch.column(b_batch_idx);

                    let differs = if a_col.is_null(batch_row) != b_col.is_null(batch_row) {
                        true
                    } else if a_col.is_null(batch_row) {
                        false
                    } else {
                        let a_val = display_formatters[display_idx]
                            .as_ref()
                            .map(|f| f.value(batch_row).to_string())
                            .unwrap_or_default();
                        let b_val = b_formatters[b_batch_idx]
                            .as_ref()
                            .map(|f| f.value(batch_row).to_string())
                            .unwrap_or_default();
                        a_val != b_val
                    };
                    if differs {
                        row_changes[display_idx] = true;
                    }
                }
            }
            changed_cells.push(row_changes);
        }
        changed_cells
    }

    /// Fetch specific rows by their data row indices.
    /// Returns a DiffPageData with display batch, b-side batch, and changed_cells.
    pub fn fetch_rows(&self, row_indices: &[usize]) -> Result<DiffPageData> {
        if row_indices.is_empty() {
            let empty_display = RecordBatch::new_empty(self.schema.clone());
            // Build empty b-side schema
            let b_fields: Vec<_> = self.b_col_pairs.iter().map(|&(_, b_idx)| {
                // Use display schema field type for b-side (they're comparable)
                let _ = b_idx;
                // b-side fields come from the b_side_col_sql columns
                Arc::new(duckdb::arrow::datatypes::Field::new("empty", DataType::Utf8, true))
            }).collect();
            let b_schema = Arc::new(Schema::new(b_fields));
            let empty_b = RecordBatch::new_empty(b_schema);
            return Ok(DiffPageData {
                display_batch: empty_display,
                b_side_batch: empty_b,
                changed_cells: Vec::new(),
                row_indices: Vec::new(),
            });
        }

        // Build SQL to fetch specific rows
        let indices_str: Vec<String> = row_indices.iter().map(|i| i.to_string()).collect();
        let in_clause = indices_str.join(", ");

        let select_cols = if self.b_side_col_sql.is_empty() {
            self.display_col_sql.clone()
        } else {
            format!("{}, {}", self.display_col_sql, self.b_side_col_sql)
        };

        let sql = format!(
            "SELECT {} FROM diff_result WHERE _rownum IN ({}) ORDER BY _rownum",
            select_cols, in_clause
        );

        let mut stmt = self.conn.prepare(&sql).context("Failed to prepare page query")?;
        let arrow = stmt.query_arrow([])?;
        let schema = arrow.get_schema();
        let batches: Vec<RecordBatch> = arrow.collect();

        let full_batch = if batches.is_empty() {
            RecordBatch::new_empty(schema.clone())
        } else if batches.len() == 1 {
            batches.into_iter().next().unwrap()
        } else {
            duckdb::arrow::compute::concat_batches(&schema, &batches)
                .context("Failed to concat page batches")?
        };

        // Split into display batch and b-side batch
        let display_columns: Vec<Arc<dyn Array>> = (0..self.num_display_cols)
            .map(|i| full_batch.column(i).clone())
            .collect();
        let display_batch = RecordBatch::try_new(self.schema.clone(), display_columns)
            .context("Failed to build display batch")?;

        // Build b-side batch from remaining columns
        let b_side_columns: Vec<Arc<dyn Array>> = (self.num_display_cols..full_batch.num_columns())
            .map(|i| full_batch.column(i).clone())
            .collect();
        let b_side_fields: Vec<_> = (self.num_display_cols..schema.fields().len())
            .map(|i| schema.fields()[i].clone())
            .collect();
        let b_side_schema = Arc::new(Schema::new(b_side_fields));
        let b_side_batch = if b_side_columns.is_empty() {
            RecordBatch::new_empty(b_side_schema)
        } else {
            RecordBatch::try_new(b_side_schema, b_side_columns)
                .context("Failed to build b_side batch")?
        };

        // Compute changed_cells for these rows
        let changed_cells = Self::compute_changed_cells(
            &display_batch,
            &b_side_batch,
            row_indices,
            &self.markers,
            &self.b_col_pairs,
            self.num_display_cols,
        );

        Ok(DiffPageData {
            display_batch,
            b_side_batch,
            changed_cells,
            row_indices: row_indices.to_vec(),
        })
    }
}

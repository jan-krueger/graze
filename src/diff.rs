use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use duckdb::arrow::array::{Array, AsArray, RecordBatch};
use duckdb::arrow::compute::concat_batches;
use duckdb::arrow::datatypes::{DataType, Schema};
use duckdb::Connection;

use crate::state::{DiffCounts, DiffMarker};

pub type DiffResult = Result<DiffResultData>;

pub struct DiffResultData {
    pub batch: RecordBatch,
    pub schema: Arc<Schema>,
    pub markers: Vec<DiffMarker>,
    pub changed_cells: Vec<Vec<bool>>,
    pub counts: DiffCounts,
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

pub fn compute_diff(
    path_a: &Path,
    path_b: &Path,
    key_columns: &[String],
    diff_columns: &[String],
) -> DiffResult {
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

    // Key columns: COALESCE(a.key, b.key)
    for key in key_columns {
        select_parts.push(format!(
            "COALESCE(a.\"{key}\", b.\"{key}\") AS \"{key}\""
        ));
    }

    // Non-key columns from A: COALESCE(a.col, b.col) so only_b rows show B's values
    for col in &all_columns {
        if key_columns.contains(col) {
            continue;
        }
        select_parts.push(format!(
            "COALESCE(a.\"{col}\", b.\"{col}\") AS \"{col}\""
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
            "a.\"{col}\" IS DISTINCT FROM b.\"{col}\""
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
        .map(|k| format!("a.\"{k}\" = b.\"{k}\""))
        .collect();
    let join_on = join_conds.join(" AND ");

    // ORDER BY key columns
    let order_parts: Vec<String> = key_columns
        .iter()
        .map(|k| format!("COALESCE(a.\"{k}\", b.\"{k}\")"))
        .collect();
    let order_by = order_parts.join(", ");

    let sql = format!(
        "SELECT {select} FROM table_a a FULL OUTER JOIN table_b b ON {join} ORDER BY {order}",
        select = select_parts.join(", "),
        join = join_on,
        order = order_by,
    );

    // Execute query
    let mut stmt = conn.prepare(&sql).context("Failed to prepare diff query")?;
    let arrow = stmt.query_arrow([])?;
    let schema = arrow.get_schema();
    let batches: Vec<RecordBatch> = arrow.collect();

    let full_batch = if batches.is_empty() {
        RecordBatch::new_empty(schema.clone())
    } else {
        concat_batches(&schema, &batches).context("Failed to concat diff batches")?
    };

    let num_rows = full_batch.num_rows();

    // Find _diff column
    let diff_col_idx = schema
        .fields()
        .iter()
        .position(|f| f.name() == "_diff")
        .context("_diff column not found in result")?;

    // Parse markers
    let diff_col = full_batch.column(diff_col_idx);
    let mut markers = Vec::with_capacity(num_rows);
    let mut counts = DiffCounts::default();

    // The _diff column is a string
    let diff_strings: Vec<String> = (0..num_rows)
        .map(|row| {
            if diff_col.is_null(row) {
                "common".to_string()
            } else {
                // Try Utf8 first, then LargeUtf8
                match diff_col.data_type() {
                    DataType::Utf8 => {
                        let arr = diff_col.as_string::<i32>();
                        arr.value(row).to_string()
                    }
                    DataType::LargeUtf8 => {
                        let arr = diff_col.as_string::<i64>();
                        arr.value(row).to_string()
                    }
                    _ => "common".to_string(),
                }
            }
        })
        .collect();

    for s in &diff_strings {
        let marker = match s.as_str() {
            "only_a" => {
                counts.only_a += 1;
                DiffMarker::OnlyA
            }
            "only_b" => {
                counts.only_b += 1;
                DiffMarker::OnlyB
            }
            "changed" => {
                counts.changed += 1;
                DiffMarker::Changed
            }
            _ => {
                counts.common += 1;
                DiffMarker::Common
            }
        };
        markers.push(marker);
    }

    // Build display column list (without _b_* and _diff columns)
    let display_col_indices: Vec<usize> = schema
        .fields()
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let name = f.name();
            name != "_diff" && !name.starts_with("_b_")
        })
        .map(|(i, _)| i)
        .collect();

    // Find _b_ column indices for each diff column
    let b_col_map: Vec<(usize, usize)> = diff_columns
        .iter()
        .filter_map(|col| {
            let b_name = format!("_b_{col}");
            let b_idx = schema.fields().iter().position(|f| f.name() == &b_name)?;
            let a_idx = display_col_indices
                .iter()
                .position(|&i| schema.fields()[i].name() == col)?;
            Some((a_idx, b_idx))
        })
        .collect();

    // Build per-cell changed_cells for Changed rows
    let num_display_cols = display_col_indices.len();
    let formatters: Vec<Option<duckdb::arrow::util::display::ArrayFormatter>> =
        (0..full_batch.num_columns())
            .map(|i| {
                duckdb::arrow::util::display::ArrayFormatter::try_new(
                    full_batch.column(i).as_ref(),
                    &Default::default(),
                )
                .ok()
            })
            .collect();

    let mut changed_cells = Vec::with_capacity(num_rows);
    for row in 0..num_rows {
        let mut row_changes = vec![false; num_display_cols];
        if markers[row] == DiffMarker::Changed {
            for &(display_idx, b_batch_idx) in &b_col_map {
                let a_batch_idx = display_col_indices[display_idx];
                let a_col = full_batch.column(a_batch_idx);
                let b_col = full_batch.column(b_batch_idx);

                let differs = if a_col.is_null(row) != b_col.is_null(row) {
                    true
                } else if a_col.is_null(row) {
                    false
                } else {
                    let a_val = formatters[a_batch_idx]
                        .as_ref()
                        .map(|f| f.value(row).to_string())
                        .unwrap_or_default();
                    let b_val = formatters[b_batch_idx]
                        .as_ref()
                        .map(|f| f.value(row).to_string())
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

    // Strip _b_* and _diff columns from the batch
    let display_columns: Vec<Arc<dyn Array>> = display_col_indices
        .iter()
        .map(|&i| full_batch.column(i).clone())
        .collect();
    let display_fields: Vec<_> = display_col_indices
        .iter()
        .map(|&i| schema.fields()[i].clone())
        .collect();
    let display_schema = Arc::new(Schema::new(display_fields));
    let display_batch =
        RecordBatch::try_new(display_schema.clone(), display_columns)
            .context("Failed to build display batch")?;

    Ok(DiffResultData {
        batch: display_batch,
        schema: display_schema,
        markers,
        changed_cells,
        counts,
    })
}

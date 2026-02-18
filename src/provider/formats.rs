use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use duckdb::arrow::datatypes::{DataType, Schema};
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::Connection;

use super::backend::DuckDbBackend;
use super::FileProvider;

pub fn csv(path: &Path) -> Result<FileProvider> {
    let conn = Connection::open_in_memory()?;
    let path_str = path.to_string_lossy();
    let load_sql =
        format!("CREATE TABLE csv_data AS SELECT * FROM read_csv_auto('{path_str}')");
    let backend = DuckDbBackend::new(conn, &load_sql, "csv_data")?;
    Ok(FileProvider {
        format_name: "CSV",
        backend,
    })
}

pub fn csv_quick(path: &Path) -> Result<FileProvider> {
    let conn = Connection::open_in_memory()?;
    let path_str = path.to_string_lossy();
    let load_sql = format!(
        "CREATE VIEW csv_data AS SELECT * FROM read_csv_auto('{path_str}', sample_size=100)"
    );
    let backend = DuckDbBackend::new_without_count(conn, &load_sql, "csv_data")?;
    Ok(FileProvider {
        format_name: "CSV",
        backend,
    })
}

pub fn json(path: &Path) -> Result<FileProvider> {
    let conn = Connection::open_in_memory()?;
    let path_str = path.to_string_lossy();
    let load_sql =
        format!("CREATE TABLE json_data AS SELECT * FROM read_json_auto('{path_str}')");
    let backend = DuckDbBackend::new(conn, &load_sql, "json_data")?;
    Ok(FileProvider {
        format_name: "JSON",
        backend,
    })
}

pub fn json_quick(path: &Path) -> Result<FileProvider> {
    let conn = Connection::open_in_memory()?;
    let path_str = path.to_string_lossy();
    let load_sql =
        format!("CREATE VIEW json_data AS SELECT * FROM read_json_auto('{path_str}')");
    let backend = DuckDbBackend::new_without_count(conn, &load_sql, "json_data")?;
    Ok(FileProvider {
        format_name: "JSON",
        backend,
    })
}

pub fn parquet(path: &Path) -> Result<FileProvider> {
    let conn = Connection::open_in_memory()?;
    let path_str = path.to_string_lossy();
    let load_sql =
        format!("CREATE TABLE parquet_data AS SELECT * FROM read_parquet('{path_str}')");
    let backend = DuckDbBackend::new(conn, &load_sql, "parquet_data")?;
    Ok(FileProvider {
        format_name: "Parquet",
        backend,
    })
}

pub fn parquet_quick(path: &Path) -> Result<FileProvider> {
    let conn = Connection::open_in_memory()?;
    let path_str = path.to_string_lossy();
    let load_sql =
        format!("CREATE VIEW parquet_data AS SELECT * FROM read_parquet('{path_str}')");
    let backend = DuckDbBackend::new_without_count(conn, &load_sql, "parquet_data")?;
    Ok(FileProvider {
        format_name: "Parquet",
        backend,
    })
}

fn arrow_type_to_duckdb(dt: &DataType) -> &'static str {
    match dt {
        DataType::Boolean => "BOOLEAN",
        DataType::Int8 => "TINYINT",
        DataType::Int16 => "SMALLINT",
        DataType::Int32 => "INTEGER",
        DataType::Int64 => "BIGINT",
        DataType::UInt8 => "UTINYINT",
        DataType::UInt16 => "USMALLINT",
        DataType::UInt32 => "UINTEGER",
        DataType::UInt64 => "UBIGINT",
        DataType::Float32 => "FLOAT",
        DataType::Float64 => "DOUBLE",
        DataType::Utf8 | DataType::LargeUtf8 => "VARCHAR",
        DataType::Date32 | DataType::Date64 => "DATE",
        DataType::Timestamp(_, _) => "TIMESTAMP",
        _ => "VARCHAR",
    }
}

pub fn from_batch(batch: &RecordBatch, schema: &Arc<Schema>, table_name: &str) -> Result<FileProvider> {
    let conn = Connection::open_in_memory()?;

    // Build CREATE TABLE statement from schema
    let col_defs: Vec<String> = schema
        .fields()
        .iter()
        .map(|f| format!("\"{}\" {}", f.name(), arrow_type_to_duckdb(f.data_type())))
        .collect();
    let create_sql = format!("CREATE TABLE {} ({})", table_name, col_defs.join(", "));
    conn.execute_batch(&create_sql)
        .context("Failed to create table for batch")?;

    // Insert data using Appender
    {
        let mut appender = conn.appender(table_name)
            .context("Failed to create appender")?;
        appender
            .append_record_batch(batch.clone())
            .context("Failed to append record batch")?;
    }

    let backend = DuckDbBackend::new_from_table(conn, table_name)?;
    Ok(FileProvider {
        format_name: "Query",
        backend,
    })
}

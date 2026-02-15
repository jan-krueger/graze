use std::path::Path;

use anyhow::Result;
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

use crossterm::event::KeyEvent;
use duckdb::arrow::record_batch::RecordBatch;
use std::path::PathBuf;
use std::sync::Arc;

use duckdb::arrow::datatypes::Schema;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

impl SortOrder {
    pub fn as_sql(&self) -> &'static str {
        match self {
            SortOrder::Ascending => "ASC",
            SortOrder::Descending => "DESC",
        }
    }

    pub fn toggle(&self) -> SortOrder {
        match self {
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Ascending,
        }
    }

    pub fn indicator(&self) -> &'static str {
        match self {
            SortOrder::Ascending => "▲",
            SortOrder::Descending => "▼",
        }
    }
}

/// A single column sort specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    pub column: String,
    pub order: SortOrder,
}

/// Manages an ordered list of sort specifications.
/// The first spec has highest priority in the ORDER BY clause.
#[derive(Debug, Clone, Default)]
pub struct SortState {
    specs: Vec<SortSpec>,
}

impl SortState {
    /// Toggle sorting for a column.
    /// If column not present: add as ASC.
    /// If ASC: change to DESC.
    /// If DESC: remove it.
    pub fn toggle(&mut self, column: &str) {
        if let Some(pos) = self.position(column) {
            match self.specs[pos].order {
                SortOrder::Ascending => {
                    self.specs[pos].order = SortOrder::Descending;
                }
                SortOrder::Descending => {
                    self.specs.remove(pos);
                }
            }
        } else {
            self.specs.push(SortSpec {
                column: column.to_string(),
                order: SortOrder::Ascending,
            });
        }
    }

    /// Remove all sort specs.
    pub fn clear(&mut self) {
        self.specs.clear();
    }

    /// Get the ordered list of sort specifications.
    pub fn specs(&self) -> &[SortSpec] {
        &self.specs
    }

    /// Returns true if there are no sort specifications.
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Find a column's 0-based position in the sort order.
    pub fn position(&self, column: &str) -> Option<usize> {
        self.specs.iter().position(|s| s.column == column)
    }

    /// Get the sort order for a column.
    pub fn order_for(&self, column: &str) -> Option<&SortOrder> {
        self.specs
            .iter()
            .find(|s| s.column == column)
            .map(|s| &s.order)
    }

    /// Build the ORDER BY clause string, e.g. `"col1" ASC, "col2" DESC`.
    /// Returns None if empty.
    pub fn to_sql(&self) -> Option<String> {
        if self.specs.is_empty() {
            return None;
        }
        let parts: Vec<String> = self
            .specs
            .iter()
            .map(|s| format!("\"{}\" {}", s.column, s.order.as_sql()))
            .collect();
        Some(parts.join(", "))
    }
}

#[derive(Debug)]
pub enum Action {
    LoadFile(PathBuf),
    FetchPage { offset: usize, limit: usize },
    ApplySort(SortState),
    Filter(String),
    ResetFilter,
    ExecuteSql(String),
    LoadBatch {
        batch: RecordBatch,
        schema: Arc<Schema>,
        name: String,
    },
    CollectMatches {
        term: String,
        is_regex: bool,
    },
    Quit,
}

#[derive(Debug)]
pub enum DataEvent {
    FileLoaded {
        schema: Arc<Schema>,
        total_rows: usize,
        file_name: String,
        table_name: String,
    },
    PageLoaded {
        offset: usize,
        batch: RecordBatch,
        total_rows: usize,
    },
    SortApplied,
    FilterApplied {
        total_rows: usize,
    },
    FilterReset {
        total_rows: usize,
    },
    SqlResult {
        batch: RecordBatch,
        schema: Arc<Schema>,
        sql: String,
    },
    SqlError {
        error: String,
        sql: String,
    },
    Materialized { total_rows: usize },
    MatchesCollected { rows: Vec<usize> },
    Error(String),
}

#[derive(Debug)]
pub enum TermEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
}

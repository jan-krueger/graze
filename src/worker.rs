use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use std::sync::Arc;

use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;

use crate::event::{Action, DataEvent, SortState};
use crate::provider::{self, DataProvider};

pub struct Worker {
    action_rx: Receiver<Action>,
    event_tx: Sender<DataEvent>,
    provider: Option<Box<dyn DataProvider>>,
    last_page: Option<(usize, usize)>,
    /// Receives the materialized (TABLE-based) provider from the background thread.
    materialize_rx: Option<Receiver<Box<dyn DataProvider>>>,
    /// Tracked so we can re-apply after swapping providers.
    current_sort: SortState,
    current_filter: Option<String>,
}

impl Worker {
    pub fn new(action_rx: Receiver<Action>, event_tx: Sender<DataEvent>) -> Self {
        Self {
            action_rx,
            event_tx,
            provider: None,
            last_page: None,
            materialize_rx: None,
            current_sort: SortState::default(),
            current_filter: None,
        }
    }

    pub fn run(mut self) {
        loop {
            // Use timeout so we periodically check for background materialization.
            let action = if self.materialize_rx.is_some() {
                match self.action_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(a) => Some(a),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match self.action_rx.recv() {
                    Ok(a) => Some(a),
                    Err(_) => break,
                }
            };

            self.check_materialize();

            let Some(action) = action else { continue };
            match action {
                Action::Quit => break,
                Action::LoadFile(path) => self.handle_load_file(path),
                Action::FetchPage { offset, limit } => self.handle_fetch_page(offset, limit),
                Action::ApplySort(state) => self.handle_apply_sort(state),
                Action::Filter(filter) => self.handle_filter(filter),
                Action::ResetFilter => self.handle_reset_filter(),
                Action::CollectMatches { term, is_regex } => {
                    self.handle_collect_matches(term, is_regex)
                }
                Action::ExecuteSql(sql) => self.handle_execute_sql(sql),
                Action::LoadBatch { batch, schema, name } => {
                    self.handle_load_batch(batch, schema, name)
                }
            }
        }
    }

    fn handle_load_file(&mut self, path: PathBuf) {
        // Phase 1: Create VIEW-based provider for instant startup.
        match provider::create_provider_quick(&path) {
            Ok(p) => {
                let schema = p.schema();
                let total_rows = p.total_rows();
                let table_name = p.table_name().to_string();
                let file_name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.provider = Some(p);
                let _ = self.event_tx.send(DataEvent::FileLoaded {
                    schema,
                    total_rows,
                    file_name,
                    table_name,
                });

                // Phase 2: Spawn background thread to materialize TABLE.
                let (mat_tx, mat_rx) = mpsc::channel::<Box<dyn DataProvider>>();
                self.materialize_rx = Some(mat_rx);

                let event_tx2 = self.event_tx.clone();
                thread::spawn(move || {
                    match provider::create_provider(&path) {
                        Ok(p) => {
                            let _ = mat_tx.send(p);
                        }
                        Err(e) => {
                            let _ = event_tx2.send(DataEvent::Error(format!(
                                "Background indexing failed: {e:#}"
                            )));
                        }
                    }
                });
            }
            Err(e) => {
                // VIEW failed too — fall back to full TABLE load.
                match provider::create_provider(&path) {
                    Ok(mut p) => {
                        let _ = p.materialize_cache();
                        let schema = p.schema();
                        let total_rows = p.total_rows();
                        let table_name = p.table_name().to_string();
                        let file_name = path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default();
                        self.provider = Some(p);
                        let _ = self.event_tx.send(DataEvent::FileLoaded {
                            schema,
                            total_rows,
                            file_name,
                            table_name,
                        });
                    }
                    Err(e2) => {
                        let _ = self
                            .event_tx
                            .send(DataEvent::Error(format!("Failed to load file: {e}, then {e2}")));
                    }
                }
            }
        }
    }

    /// Check if background materialization has finished and swap providers.
    fn check_materialize(&mut self) {
        let new_provider = match self.materialize_rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(p) => Some(p),
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.materialize_rx = None;
                    return;
                }
            },
            None => return,
        };

        self.materialize_rx = None;

        if let Some(mut p) = new_provider {
            // Re-apply current sort/filter state to the new provider.
            if !self.current_sort.is_empty() {
                let _ = p.apply_sort_state(&self.current_sort);
            }
            if let Some(ref filter) = self.current_filter {
                let _ = p.apply_filter(filter);
            }
            // Populate the Arrow cache for instant pagination and search.
            let _ = p.materialize_cache();
            let total_rows = p.total_rows();
            self.provider = Some(p);

            let _ = self.event_tx.send(DataEvent::Materialized { total_rows });

            // Refetch current page so the UI stays consistent.
            self.refetch_page();
        }
    }

    fn handle_fetch_page(&mut self, offset: usize, limit: usize) {
        self.last_page = Some((offset, limit));
        if let Some(ref provider) = self.provider {
            match provider.fetch_page(offset, limit) {
                Ok(batch) => {
                    let _ = self.event_tx.send(DataEvent::PageLoaded {
                        offset,
                        batch,
                        total_rows: provider.total_rows(),
                    });
                }
                Err(e) => {
                    let _ = self.event_tx.send(DataEvent::Error(format!(
                        "Failed to fetch page: {e}"
                    )));
                }
            }
        }
    }

    fn handle_apply_sort(&mut self, state: SortState) {
        self.current_sort = state.clone();
        if let Some(ref mut provider) = self.provider {
            match provider.apply_sort_state(&state) {
                Ok(()) => {
                    let _ = self.event_tx.send(DataEvent::SortApplied);
                    self.refetch_page();
                }
                Err(e) => {
                    let _ = self.event_tx.send(DataEvent::Error(format!(
                        "Failed to apply sort: {e}"
                    )));
                }
            }
        }
    }

    fn handle_filter(&mut self, filter: String) {
        self.current_filter = Some(filter.clone());
        if let Some(ref mut provider) = self.provider {
            match provider.apply_filter(&filter) {
                Ok(total_rows) => {
                    let _ = self
                        .event_tx
                        .send(DataEvent::FilterApplied { total_rows });
                    // Reset to offset 0 since the app resets viewport to row 0.
                    if let Some((ref mut offset, _)) = self.last_page {
                        *offset = 0;
                    }
                    self.refetch_page();
                }
                Err(e) => {
                    let _ = self.event_tx.send(DataEvent::Error(format!(
                        "Invalid filter: {e}"
                    )));
                }
            }
        }
    }

    fn handle_reset_filter(&mut self) {
        self.current_filter = None;
        if let Some(ref mut provider) = self.provider {
            match provider.reset_filters() {
                Ok(total_rows) => {
                    let _ = self
                        .event_tx
                        .send(DataEvent::FilterReset { total_rows });
                    // Reset to offset 0 since the app resets viewport to row 0.
                    if let Some((ref mut offset, _)) = self.last_page {
                        *offset = 0;
                    }
                    self.refetch_page();
                }
                Err(e) => {
                    let _ = self.event_tx.send(DataEvent::Error(format!(
                        "Failed to reset filter: {e}"
                    )));
                }
            }
        }
    }

    fn handle_collect_matches(&self, term: String, is_regex: bool) {
        if let Some(ref provider) = self.provider {
            match provider.collect_match_rows(&term, is_regex) {
                Ok(rows) => {
                    let _ = self.event_tx.send(DataEvent::MatchesCollected { rows });
                }
                Err(e) => {
                    let _ = self.event_tx.send(DataEvent::Error(format!(
                        "Search failed: {e}"
                    )));
                }
            }
        }
    }

    fn handle_execute_sql(&self, sql: String) {
        if let Some(ref provider) = self.provider {
            match provider.execute_sql(&sql) {
                Ok(batch) => {
                    let schema = Arc::new(batch.schema().as_ref().clone());
                    let _ = self.event_tx.send(DataEvent::SqlResult {
                        batch,
                        schema,
                        sql,
                    });
                }
                Err(e) => {
                    let _ = self.event_tx.send(DataEvent::SqlError {
                        error: format!("{e}"),
                        sql,
                    });
                }
            }
        }
    }

    fn handle_load_batch(&mut self, batch: RecordBatch, schema: Arc<Schema>, name: String) {
        match provider::formats::from_batch(&batch, &schema, "query_data") {
            Ok(mut p) => {
                let _ = p.materialize_cache();
                let total_rows = p.total_rows();
                let table_name = p.table_name().to_string();
                self.provider = Some(Box::new(p));
                let _ = self.event_tx.send(DataEvent::FileLoaded {
                    schema,
                    total_rows,
                    file_name: name,
                    table_name,
                });
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(DataEvent::Error(format!("Failed to load batch: {e:#}")));
            }
        }
    }

    fn refetch_page(&self) {
        if let (Some(provider), Some((offset, limit))) = (&self.provider, self.last_page) {
            match provider.fetch_page(offset, limit) {
                Ok(batch) => {
                    let _ = self.event_tx.send(DataEvent::PageLoaded {
                        offset,
                        batch,
                        total_rows: provider.total_rows(),
                    });
                }
                Err(e) => {
                    let _ = self
                        .event_tx
                        .send(DataEvent::Error(format!("Failed to refetch page: {e}")));
                }
            }
        }
    }
}

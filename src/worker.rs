use std::sync::mpsc::{Receiver, Sender};

use std::sync::Arc;

use crate::event::{Action, DataEvent};
use crate::provider::{self, DataProvider};

pub struct Worker {
    action_rx: Receiver<Action>,
    event_tx: Sender<DataEvent>,
    provider: Option<Box<dyn DataProvider>>,
    last_page: Option<(usize, usize)>,
}

impl Worker {
    pub fn new(action_rx: Receiver<Action>, event_tx: Sender<DataEvent>) -> Self {
        Self {
            action_rx,
            event_tx,
            provider: None,
            last_page: None,
        }
    }

    pub fn run(mut self) {
        while let Ok(action) = self.action_rx.recv() {
            match action {
                Action::Quit => break,
                Action::LoadFile(path) => {
                    match provider::create_provider(&path) {
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
                        }
                        Err(e) => {
                            let _ = self
                                .event_tx
                                .send(DataEvent::Error(format!("Failed to load file: {e}")));
                        }
                    }
                }
                Action::FetchPage { offset, limit } => {
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
                Action::ApplySort(state) => {
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
                Action::Filter(filter) => {
                    if let Some(ref mut provider) = self.provider {
                        match provider.apply_filter(&filter) {
                            Ok(total_rows) => {
                                let _ = self
                                    .event_tx
                                    .send(DataEvent::FilterApplied { total_rows });
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
                Action::ResetFilter => {
                    if let Some(ref mut provider) = self.provider {
                        match provider.reset_filters() {
                            Ok(total_rows) => {
                                let _ =
                                    self.event_tx.send(DataEvent::FilterReset { total_rows });
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
                Action::ExecuteSql(sql) => {
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

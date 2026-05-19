//! SessionManager — open/close named recording sessions, broadcast boundaries.

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{info, warn};
use veyn_schemas::{Session, SessionBoundary, SessionBoundaryKind, VeynDevice};

use crate::storage;

pub struct SessionManager {
    /// The currently open session, if any.
    pub current: Option<Session>,
    pub boundary_tx: broadcast::Sender<SessionBoundary>,
}

impl SessionManager {
    pub fn new(boundary_tx: broadcast::Sender<SessionBoundary>) -> Self {
        Self {
            current: None,
            boundary_tx,
        }
    }

    /// Open a new session. Closes any currently open session first.
    pub fn open(
        &mut self,
        label: String,
        devices: Vec<VeynDevice>,
        db: Option<&rusqlite::Connection>,
    ) -> Session {
        if let Some(open) = self.current.take() {
            warn!(id = %open.id, "auto-closing previous session before opening new one");
            self.close_internal(open, db);
        }

        let device_ids = devices.iter().map(|d| d.id.clone()).collect();
        let session = Session::new(label, device_ids);

        if let Some(conn) = db {
            if let Err(e) = storage::insert_session(conn, &session) {
                warn!("failed to persist session: {}", e);
            }
        }

        let boundary = SessionBoundary {
            session_id: session.id.clone(),
            kind: SessionBoundaryKind::Start,
            ts: session.started_at,
            label: session.label.clone(),
        };
        let _ = self.boundary_tx.send(boundary);
        info!(id = %session.id, label = %session.label, "session started");

        self.current = Some(session.clone());
        session
    }

    /// Close the current session. Returns it if one was open.
    pub fn close(&mut self, db: Option<&rusqlite::Connection>) -> Option<Session> {
        let open = self.current.take()?;
        let closed = self.close_internal(open, db);
        Some(closed)
    }

    fn close_internal(&self, mut session: Session, db: Option<&rusqlite::Connection>) -> Session {
        let now = chrono::Utc::now().timestamp_millis();
        session.ended_at = Some(now);

        if let Some(conn) = db {
            if let Err(e) = storage::update_session(conn, &session) {
                warn!("failed to update session on close: {}", e);
            }
        }

        let boundary = SessionBoundary {
            session_id: session.id.clone(),
            kind: SessionBoundaryKind::End,
            ts: now,
            label: session.label.clone(),
        };
        let _ = self.boundary_tx.send(boundary);
        info!(id = %session.id, "session ended");
        session
    }

    pub fn annotate(&mut self, notes: String, db: Option<&rusqlite::Connection>) -> bool {
        if let Some(ref mut session) = self.current {
            session.notes = Some(notes);
            if let Some(conn) = db {
                if let Err(e) = storage::update_session(conn, session) {
                    warn!("failed to annotate session: {}", e);
                }
            }
            return true;
        }
        false
    }

    pub fn current_id(&self) -> Option<Arc<String>> {
        self.current.as_ref().map(|s| Arc::new(s.id.clone()))
    }
}

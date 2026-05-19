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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;
    use veyn_schemas::VeynDevice;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        storage::open_connection(&conn).unwrap();
        conn
    }

    #[test]
    fn test_session_manager_open_close() {
        let (tx, mut rx) = broadcast::channel(10);
        let mut manager = SessionManager::new(tx);
        let db = setup_db();

        let d1 = VeynDevice {
            id: "dev_1".to_string(),
            name: "Device 1".to_string(),
            source: "generic".to_string(),
            state: veyn_schemas::DeviceState::Connected,
            last_seen: 0,
        };

        // Open first session
        let s1 = manager.open("test_session_1".to_string(), vec![d1.clone()], Some(&db));
        assert_eq!(s1.label, "test_session_1");
        assert_eq!(manager.current_id().unwrap().as_ref(), &s1.id);

        // Check broadcast
        let boundary = rx.try_recv().unwrap();
        assert_eq!(boundary.session_id, s1.id);
        assert!(matches!(boundary.kind, SessionBoundaryKind::Start));

        // Check DB
        let db_s1 = storage::get_session(&db, &s1.id).unwrap().unwrap();
        assert_eq!(db_s1.id, s1.id);
        assert!(db_s1.ended_at.is_none());

        // Open second session (should auto-close first)
        let s2 = manager.open("test_session_2".to_string(), vec![d1], Some(&db));
        assert_eq!(s2.label, "test_session_2");
        assert_eq!(manager.current_id().unwrap().as_ref(), &s2.id);

        // Check broadcasts
        // 1. End of first session
        let boundary = rx.try_recv().unwrap();
        assert_eq!(boundary.session_id, s1.id);
        assert!(matches!(boundary.kind, SessionBoundaryKind::End));

        // 2. Start of second session
        let boundary = rx.try_recv().unwrap();
        assert_eq!(boundary.session_id, s2.id);
        assert!(matches!(boundary.kind, SessionBoundaryKind::Start));

        // Check DB for first session auto-close
        let db_s1_closed = storage::get_session(&db, &s1.id).unwrap().unwrap();
        assert!(db_s1_closed.ended_at.is_some());

        // Close second session explicitly
        let closed_s2 = manager.close(Some(&db)).unwrap();
        assert_eq!(closed_s2.id, s2.id);
        assert!(manager.current_id().is_none());

        // Check broadcast
        let boundary = rx.try_recv().unwrap();
        assert_eq!(boundary.session_id, s2.id);
        assert!(matches!(boundary.kind, SessionBoundaryKind::End));

        // Check DB
        let db_s2_closed = storage::get_session(&db, &s2.id).unwrap().unwrap();
        assert!(db_s2_closed.ended_at.is_some());
    }

    #[test]
    fn test_session_manager_annotate() {
        let (tx, _rx) = broadcast::channel(10);
        let mut manager = SessionManager::new(tx);
        let db = setup_db();

        assert!(!manager.annotate("test note".to_string(), Some(&db)));

        let s1 = manager.open("test_session".to_string(), vec![], Some(&db));

        assert!(manager.annotate("test note".to_string(), Some(&db)));

        let db_s1 = storage::get_session(&db, &s1.id).unwrap().unwrap();
        assert_eq!(db_s1.notes.unwrap(), "test note");
    }
}

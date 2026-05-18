//! SQLite persistence layer for sessions and recorded events.

use anyhow::Result;
use rusqlite::{params, Connection};
use veyn_schemas::{Session, VeynEvent};

pub fn open(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id          TEXT PRIMARY KEY,
            label       TEXT NOT NULL,
            started_at  INTEGER NOT NULL,
            ended_at    INTEGER,
            device_ids  TEXT NOT NULL DEFAULT '[]',
            notes       TEXT
        );

        CREATE TABLE IF NOT EXISTS events (
            id          TEXT PRIMARY KEY,
            session_id  TEXT REFERENCES sessions(id),
            ts          INTEGER NOT NULL,
            device_id   TEXT NOT NULL,
            source      TEXT NOT NULL,
            metric      TEXT NOT NULL,
            value       REAL NOT NULL,
            unit        TEXT NOT NULL,
            meta        TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
        CREATE INDEX IF NOT EXISTS idx_events_ts      ON events(ts);

        CREATE TABLE IF NOT EXISTS baseline_samples (
            device_id   TEXT NOT NULL,
            metric      TEXT NOT NULL,
            ts          INTEGER NOT NULL,
            value       REAL NOT NULL,
            PRIMARY KEY (device_id, metric, ts)
        );

        CREATE INDEX IF NOT EXISTS idx_baseline_lookup
            ON baseline_samples(device_id, metric, ts);
        ",
    )?;
    Ok(())
}

// ── Session CRUD ──────────────────────────────────────────────────────────────

pub fn insert_session(conn: &Connection, session: &Session) -> Result<()> {
    let device_ids = serde_json::to_string(&session.active_device_ids)?;
    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, label, started_at, ended_at, device_ids, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session.id,
            session.label,
            session.started_at,
            session.ended_at,
            device_ids,
            session.notes,
        ],
    )?;
    Ok(())
}

pub fn update_session(conn: &Connection, session: &Session) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET ended_at=?1, notes=?2 WHERE id=?3",
        params![session.ended_at, session.notes, session.id],
    )?;
    Ok(())
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Option<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, started_at, ended_at, device_ids, notes FROM sessions WHERE id=?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row_to_session(row)?))
    } else {
        Ok(None)
    }
}

pub fn list_sessions(conn: &Connection, limit: usize, offset: usize) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, started_at, ended_at, device_ids, notes FROM sessions
         ORDER BY started_at DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
        Ok(row_to_session_sync(row))
    })?;
    let mut sessions = Vec::new();
    for r in rows {
        sessions.push(r??);
    }
    Ok(sessions)
}

fn row_to_session(row: &rusqlite::Row) -> Result<Session> {
    let device_ids_json: String = row.get(4)?;
    let device_ids: Vec<String> = serde_json::from_str(&device_ids_json).unwrap_or_default();
    Ok(Session {
        id: row.get(0)?,
        label: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        active_device_ids: device_ids,
        notes: row.get(5)?,
    })
}

fn row_to_session_sync(row: &rusqlite::Row) -> Result<Session, rusqlite::Error> {
    let device_ids_json: String = row.get(4)?;
    let device_ids: Vec<String> = serde_json::from_str(&device_ids_json).unwrap_or_default();
    Ok(Session {
        id: row.get(0)?,
        label: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        active_device_ids: device_ids,
        notes: row.get(5)?,
    })
}

// ── Event persistence ─────────────────────────────────────────────────────────

pub fn insert_event(conn: &Connection, event: &VeynEvent, session_id: Option<&str>) -> Result<()> {
    let meta = serde_json::to_string(&event.meta).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT OR IGNORE INTO events
         (id, session_id, ts, device_id, source, metric, value, unit, meta)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event.id,
            session_id,
            event.ts,
            event.device_id,
            event.source,
            event.metric,
            event.value,
            event.unit,
            meta,
        ],
    )?;
    Ok(())
}

pub fn replay_session_events(conn: &Connection, session_id: &str) -> Result<Vec<VeynEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, device_id, source, metric, value, unit, meta
         FROM events WHERE session_id=?1 ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        let meta_str: String = row.get(7)?;
        let meta = serde_json::from_str(&meta_str).unwrap_or_default();
        Ok(VeynEvent {
            id: row.get(0)?,
            ts: row.get(1)?,
            device_id: row.get(2)?,
            source: row.get(3)?,
            metric: row.get(4)?,
            value: row.get(5)?,
            unit: row.get(6)?,
            meta,
        })
    })?;
    let mut events = Vec::new();
    for r in rows {
        events.push(r?);
    }
    Ok(events)
}

// ── Baseline sample persistence ───────────────────────────────────────────────

pub fn insert_baseline_sample(
    conn: &Connection,
    device_id: &str,
    metric: &str,
    ts: i64,
    value: f64,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO baseline_samples (device_id, metric, ts, value)
         VALUES (?1, ?2, ?3, ?4)",
        params![device_id, metric, ts, value],
    )?;
    Ok(())
}

/// Load all samples for a (device_id, metric) pair within the last `window_days`.
pub fn load_baseline_samples(
    conn: &Connection,
    device_id: &str,
    metric: &str,
    window_days: u32,
) -> Result<Vec<f64>> {
    let cutoff =
        chrono::Utc::now().timestamp_millis() - (window_days as i64) * 24 * 60 * 60 * 1_000;
    let mut stmt = conn.prepare(
        "SELECT value FROM baseline_samples
         WHERE device_id=?1 AND metric=?2 AND ts>=?3
         ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map(params![device_id, metric, cutoff], |row| row.get(0))?;
    let mut values = Vec::new();
    for r in rows {
        values.push(r?);
    }
    Ok(values)
}

//! Biometric memory layer — persistent per-topic summaries with physiological state.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use tracing::{info, warn};
use uuid::Uuid;
use veyn_schemas::{ContextSnapshot, MemoryKind, MemoryQuery, MemoryRecord};

use crate::config::Config;

// ── MemoryStore ───────────────────────────────────────────────────────────────

/// Wraps the shared SQLite connection with memory-layer read/write operations.
pub struct MemoryStore {
    db: Arc<Mutex<Connection>>,
    max_records: usize,
}

impl MemoryStore {
    pub fn new(db: Arc<Mutex<Connection>>, max_records: usize) -> Self {
        Self { db, max_records }
    }

    pub fn write(&self, record: MemoryRecord) -> Result<()> {
        let conn = self.db.lock().unwrap();
        write_memory(&conn, &record)?;

        // Prune oldest ambient records when the cap is exceeded; semantic records are never pruned.
        let ambient_count = count_ambient_memory(&conn)?;
        if ambient_count > self.max_records {
            let excess = ambient_count - self.max_records;
            prune_oldest_ambient(&conn, excess)?;
            info!(
                pruned = excess,
                remaining = self.max_records,
                "ambient memory pruned"
            );
        }
        Ok(())
    }

    pub fn query(&self, q: &MemoryQuery) -> Result<Vec<MemoryRecord>> {
        let conn = self.db.lock().unwrap();
        query_memory(&conn, q)
    }

    pub fn get(&self, id: &str) -> Result<Option<MemoryRecord>> {
        let conn = self.db.lock().unwrap();
        get_memory_by_id(&conn, id)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.db.lock().unwrap();
        delete_memory_by_id(&conn, id)
    }

    /// Collapse existing memory records within a time window into a single summary string.
    #[allow(dead_code)]
    pub fn summarize_window(&self, since_ms: i64, until_ms: i64) -> Result<String> {
        let q = MemoryQuery {
            since_ms: Some(since_ms),
            until_ms: Some(until_ms),
            limit: Some(10_000),
            ..Default::default()
        };
        let records = self.query(&q)?;
        Ok(summarize_records(&records, since_ms, until_ms))
    }
}

// ── SQL helpers ───────────────────────────────────────────────────────────────

pub fn write_memory(conn: &Connection, record: &MemoryRecord) -> Result<()> {
    let kind_str = kind_to_str(&record.kind);
    let ctx_json = record
        .context_snapshot
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    conn.execute(
        "INSERT OR REPLACE INTO veyn_memory
         (id, timestamp_ms, session_id, kind, topic, summary,
          intent_at_time, confidence_at_time, hrv_at_time, hr_at_time, context_snapshot)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            record.id,
            record.timestamp_ms,
            record.session_id,
            kind_str,
            record.topic,
            record.summary,
            record.intent_at_time,
            record.confidence_at_time,
            record.hrv_at_time,
            record.hr_at_time,
            ctx_json,
        ],
    )?;
    Ok(())
}

pub fn get_memory_by_id(conn: &Connection, id: &str) -> Result<Option<MemoryRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp_ms, session_id, kind, topic, summary,
                intent_at_time, confidence_at_time, hrv_at_time, hr_at_time, context_snapshot
         FROM veyn_memory
         WHERE id = ?1",
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
        let kind_raw: String = row.get(3)?;
        let ctx_raw: Option<String> = row.get(10)?;
        Ok(MemoryRecord {
            id: row.get(0)?,
            timestamp_ms: row.get(1)?,
            session_id: row.get(2)?,
            kind: str_to_kind(&kind_raw),
            topic: row.get(4)?,
            summary: row.get(5)?,
            intent_at_time: row.get(6)?,
            confidence_at_time: row.get(7)?,
            hrv_at_time: row.get(8)?,
            hr_at_time: row.get(9)?,
            context_snapshot: ctx_raw.and_then(|s| serde_json::from_str(&s).ok()),
        })
    })?;

    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
}

pub fn delete_memory_by_id(conn: &Connection, id: &str) -> Result<bool> {
    let rows_affected = conn.execute("DELETE FROM veyn_memory WHERE id = ?1", params![id])?;
    Ok(rows_affected > 0)
}

pub fn query_memory(conn: &Connection, q: &MemoryQuery) -> Result<Vec<MemoryRecord>> {
    let since_ms = q.since_ms.unwrap_or(i64::MIN);
    let until_ms = q.until_ms.unwrap_or(i64::MAX);
    let limit = q.limit.unwrap_or(100) as i64;
    let kind_str: Option<&str> = q.kind.as_ref().map(|k| kind_to_str(k));

    let mut stmt = conn.prepare(
        "SELECT id, timestamp_ms, session_id, kind, topic, summary,
                intent_at_time, confidence_at_time, hrv_at_time, hr_at_time, context_snapshot
         FROM veyn_memory
         WHERE (?1 IS NULL OR topic = ?1)
           AND (?2 IS NULL OR kind  = ?2)
           AND timestamp_ms >= ?3
           AND timestamp_ms <= ?4
         ORDER BY timestamp_ms DESC
         LIMIT ?5",
    )?;

    let rows = stmt.query_map(
        params![q.topic.as_deref(), kind_str, since_ms, until_ms, limit],
        |row| {
            let kind_raw: String = row.get(3)?;
            let ctx_raw: Option<String> = row.get(10)?;
            Ok(MemoryRecord {
                id: row.get(0)?,
                timestamp_ms: row.get(1)?,
                session_id: row.get(2)?,
                kind: str_to_kind(&kind_raw),
                topic: row.get(4)?,
                summary: row.get(5)?,
                intent_at_time: row.get(6)?,
                confidence_at_time: row.get(7)?,
                hrv_at_time: row.get(8)?,
                hr_at_time: row.get(9)?,
                context_snapshot: ctx_raw.and_then(|s| serde_json::from_str(&s).ok()),
            })
        },
    )?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn count_ambient_memory(conn: &Connection) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM veyn_memory WHERE kind='ambient'",
        [],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

fn prune_oldest_ambient(conn: &Connection, excess: usize) -> Result<()> {
    conn.execute(
        "DELETE FROM veyn_memory WHERE kind='ambient' AND id IN (
             SELECT id FROM veyn_memory WHERE kind='ambient'
             ORDER BY timestamp_ms ASC LIMIT ?1
         )",
        params![excess as i64],
    )?;
    Ok(())
}

fn kind_to_str(k: &MemoryKind) -> &'static str {
    match k {
        MemoryKind::Ambient => "ambient",
        MemoryKind::Semantic => "semantic",
    }
}

fn str_to_kind(s: &str) -> MemoryKind {
    if s == "ambient" {
        MemoryKind::Ambient
    } else {
        MemoryKind::Semantic
    }
}

// ── Snapshot summarisation ────────────────────────────────────────────────────

/// Collapse a slice of ContextSnapshots into a human-readable ambient summary string.
pub fn summarize_snapshots(snapshots: &[ContextSnapshot]) -> String {
    if snapshots.is_empty() {
        return "no data in window".to_string();
    }

    let first_ts = snapshots.first().map(|s| s.timestamp_ms).unwrap_or(0);
    let last_ts = snapshots.last().map(|s| s.timestamp_ms).unwrap_or(0);
    let duration_mins = (last_ts - first_ts).max(0) / 60_000;

    let hrv_vals: Vec<f64> = snapshots
        .iter()
        .flat_map(|s| s.state_deltas.iter())
        .filter(|d| d.metric == "hrv")
        .map(|d| d.value)
        .collect();

    let hr_vals: Vec<f64> = snapshots
        .iter()
        .flat_map(|s| s.state_deltas.iter())
        .filter(|d| d.metric == "heart_rate")
        .map(|d| d.value)
        .collect();

    let avg_hrv = mean(&hrv_vals);
    let avg_hr = mean(&hr_vals);

    let mut intent_counts: HashMap<String, usize> = HashMap::new();
    for s in snapshots {
        *intent_counts.entry(s.intent.clone()).or_insert(0) += 1;
    }
    let dominant_intent = intent_counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(i, _)| i)
        .unwrap_or_else(|| "neutral".to_string());

    let mut parts = Vec::new();
    parts.push(format!("snapshots={}", snapshots.len()));
    parts.push(format!("duration={}min", duration_mins));
    parts.push(format!("dominant_intent={}", dominant_intent));
    if let Some(hr) = avg_hr {
        parts.push(format!("avg_hr={:.0}bpm", hr));
    }
    if let Some(hrv) = avg_hrv {
        parts.push(format!("avg_hrv={:.1}ms", hrv));
    }
    parts.join("; ")
}

/// Collapse a slice of MemoryRecords into a human-readable summary string.
#[allow(dead_code)]
fn summarize_records(records: &[MemoryRecord], since_ms: i64, until_ms: i64) -> String {
    if records.is_empty() {
        return "no memory records in window".to_string();
    }

    let duration_mins = (until_ms - since_ms).max(0) / 60_000;
    let hrv_vals: Vec<f64> = records.iter().filter_map(|r| r.hrv_at_time).collect();
    let hr_vals: Vec<f64> = records.iter().filter_map(|r| r.hr_at_time).collect();

    let mut intent_counts: HashMap<String, usize> = HashMap::new();
    for r in records {
        if let Some(ref intent) = r.intent_at_time {
            *intent_counts.entry(intent.clone()).or_insert(0) += 1;
        }
    }
    let dominant_intent = intent_counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(i, _)| i)
        .unwrap_or_else(|| "neutral".to_string());

    let mut parts = Vec::new();
    parts.push(format!("records={}", records.len()));
    parts.push(format!("duration={}min", duration_mins));
    parts.push(format!("dominant_intent={}", dominant_intent));
    if let Some(hr) = mean(&hr_vals) {
        parts.push(format!("avg_hr={:.0}bpm", hr));
    }
    if let Some(hrv) = mean(&hrv_vals) {
        parts.push(format!("avg_hrv={:.1}ms", hrv));
    }
    parts.join("; ")
}

fn mean(vals: &[f64]) -> Option<f64> {
    if vals.is_empty() {
        None
    } else {
        Some(vals.iter().sum::<f64>() / vals.len() as f64)
    }
}

// ── Ambient writer task ───────────────────────────────────────────────────────

/// Background task that writes one Ambient MemoryRecord per interval tick.
/// Skips ticks where no device was active during the window.
pub async fn ambient_writer(
    db: Arc<Mutex<Connection>>,
    config: Arc<Config>,
    context_history: Arc<Mutex<VecDeque<ContextSnapshot>>>,
    session_id: Arc<String>,
) {
    if !config.memory_enabled {
        return;
    }

    let store = Arc::new(MemoryStore::new(db, config.memory_max_records));
    let interval_secs = config.memory_ambient_interval_secs;

    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    ticker.tick().await; // discard the immediate first tick

    loop {
        ticker.tick().await;

        let snapshots: Vec<ContextSnapshot> =
            context_history.lock().unwrap().iter().cloned().collect();

        // Skip idle windows with no active devices.
        if snapshots.is_empty() || !snapshots.iter().any(|s| !s.active_devices.is_empty()) {
            continue;
        }

        let summary = summarize_snapshots(&snapshots);
        let last = snapshots.last().unwrap();

        let hrv = last
            .state_deltas
            .iter()
            .find(|d| d.metric == "hrv")
            .map(|d| d.value);
        let hr = last
            .state_deltas
            .iter()
            .find(|d| d.metric == "heart_rate")
            .map(|d| d.value);

        let record = MemoryRecord {
            id: Uuid::new_v4().to_string(),
            timestamp_ms: Utc::now().timestamp_millis(),
            session_id: (*session_id).clone(),
            kind: MemoryKind::Ambient,
            topic: "ambient".to_string(),
            summary,
            intent_at_time: Some(last.intent.clone()),
            confidence_at_time: Some(last.confidence),
            hrv_at_time: hrv,
            hr_at_time: hr,
            context_snapshot: None,
        };

        match store.write(record) {
            Ok(()) => info!("ambient memory record written"),
            Err(e) => warn!("ambient memory write failed: {}", e),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use veyn_schemas::{IntentCode, StateDelta};

    fn make_snapshot(ts: i64, intent: &str, hr: f64, hrv: f64) -> ContextSnapshot {
        ContextSnapshot {
            timestamp_ms: ts,
            session_id: "test-session".to_string(),
            intent: intent.to_string(),
            intent_code: IntentCode::Neutral,
            confidence: 0.8,
            intent_confidence: 0.8,
            active_devices: vec!["device-1".to_string()],
            state_deltas: vec![
                StateDelta {
                    device_id: "device-1".to_string(),
                    metric: "heart_rate".to_string(),
                    value: hr,
                    unit: "bpm".to_string(),
                    ts,
                    source_class: "mock".to_string(),
                },
                StateDelta {
                    device_id: "device-1".to_string(),
                    metric: "hrv".to_string(),
                    value: hrv,
                    unit: "ms".to_string(),
                    ts,
                    source_class: "mock".to_string(),
                },
            ],
            baseline_delta: None,
            recording_session_id: None,
            temporal_patterns: vec![],
        }
    }

    #[test]
    fn summarize_window_empty() {
        let s = summarize_snapshots(&[]);
        assert_eq!(s, "no data in window");
    }

    #[test]
    fn summarize_window_stats() {
        let snaps = vec![
            make_snapshot(0, "neutral", 65.0, 50.0),
            make_snapshot(60_000, "recovery", 62.0, 55.0),
            make_snapshot(120_000, "recovery", 63.0, 52.0),
        ];
        let s = summarize_snapshots(&snaps);
        assert!(s.contains("snapshots=3"), "got: {s}");
        assert!(s.contains("dominant_intent=recovery"), "got: {s}");
        assert!(s.contains("avg_hr="), "got: {s}");
        assert!(s.contains("avg_hrv="), "got: {s}");
        assert!(s.contains("duration=2min"), "got: {s}");
    }

    #[test]
    fn summarize_window_avg_hr_correct() {
        let snaps = vec![
            make_snapshot(0, "neutral", 60.0, 40.0),
            make_snapshot(30_000, "neutral", 80.0, 60.0),
        ];
        let s = summarize_snapshots(&snaps);
        assert!(s.contains("avg_hr=70bpm"), "got: {s}");
        assert!(s.contains("avg_hrv=50.0ms"), "got: {s}");
    }

    #[test]
    fn write_and_query_roundtrip() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::open_connection(&conn).unwrap();

        let record = MemoryRecord {
            id: "rec-1".to_string(),
            timestamp_ms: 1_000_000,
            session_id: "s1".to_string(),
            kind: MemoryKind::Semantic,
            topic: "deep-work".to_string(),
            summary: "finished auth module".to_string(),
            intent_at_time: Some("cognitive_load".to_string()),
            confidence_at_time: Some(0.85),
            hrv_at_time: Some(42.3),
            hr_at_time: Some(71.0),
            context_snapshot: None,
        };

        write_memory(&conn, &record).unwrap();

        let results = query_memory(
            &conn,
            &MemoryQuery {
                topic: Some("deep-work".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.id, "rec-1");
        assert_eq!(r.kind, MemoryKind::Semantic);
        assert_eq!(r.topic, "deep-work");
        assert_eq!(r.hrv_at_time, Some(42.3));
    }

    #[test]
    fn query_kind_filter() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::open_connection(&conn).unwrap();

        for (id, kind) in [("a1", MemoryKind::Ambient), ("s1", MemoryKind::Semantic)] {
            write_memory(
                &conn,
                &MemoryRecord {
                    id: id.to_string(),
                    timestamp_ms: 1_000,
                    session_id: "s".to_string(),
                    kind,
                    topic: "t".to_string(),
                    summary: "x".to_string(),
                    intent_at_time: None,
                    confidence_at_time: None,
                    hrv_at_time: None,
                    hr_at_time: None,
                    context_snapshot: None,
                },
            )
            .unwrap();
        }

        let semantic = query_memory(
            &conn,
            &MemoryQuery {
                kind: Some(MemoryKind::Semantic),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(semantic.len(), 1);
        assert_eq!(semantic[0].id, "s1");

        let ambient = query_memory(
            &conn,
            &MemoryQuery {
                kind: Some(MemoryKind::Ambient),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ambient.len(), 1);
        assert_eq!(ambient[0].id, "a1");
    }

    /// Test that the ambient writer fires and writes a record when given a very short interval.
    #[tokio::test]
    async fn ambient_writer_fires_and_writes_record() {
        use std::sync::Arc;

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::open_connection(&conn).unwrap();
        let db = Arc::new(Mutex::new(conn));

        // Build a minimal Config with a 1-second ambient interval.
        let cfg = crate::config::Config {
            memory_enabled: true,
            memory_ambient_interval_secs: 1,
            memory_max_records: 100,
            ..Default::default()
        };
        let config = Arc::new(cfg);

        // Pre-populate the context history with one snapshot that has active devices.
        let snap = make_snapshot(chrono::Utc::now().timestamp_millis(), "neutral", 70.0, 45.0);
        let history: Arc<Mutex<VecDeque<ContextSnapshot>>> =
            Arc::new(Mutex::new(VecDeque::from(vec![snap])));
        let session_id = Arc::new("test-daemon-session".to_string());

        // Spawn the ambient writer; let it fire once (after ~1 s).
        let db2 = db.clone();
        let cfg2 = config.clone();
        let hist2 = history.clone();
        let sid2 = session_id.clone();
        tokio::spawn(async move {
            ambient_writer(db2, cfg2, hist2, sid2).await;
        });

        // Wait long enough for one tick (interval is 1 s; add generous buffer).
        tokio::time::sleep(std::time::Duration::from_millis(2_200)).await;

        let records = query_memory(
            &db.lock().unwrap(),
            &MemoryQuery {
                kind: Some(MemoryKind::Ambient),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(
            !records.is_empty(),
            "ambient writer should have written at least one record"
        );
        let r = &records[0];
        assert_eq!(r.kind, MemoryKind::Ambient);
        assert_eq!(r.topic, "ambient");
        assert!(
            r.summary.contains("dominant_intent=neutral"),
            "got: {}",
            r.summary
        );
    }

    #[test]
    fn prune_oldest_ambient_leaves_semantic() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::open_connection(&conn).unwrap();

        for i in 0..5i64 {
            write_memory(
                &conn,
                &MemoryRecord {
                    id: format!("a{i}"),
                    timestamp_ms: i * 1_000,
                    session_id: "s".to_string(),
                    kind: MemoryKind::Ambient,
                    topic: "ambient".to_string(),
                    summary: "".to_string(),
                    intent_at_time: None,
                    confidence_at_time: None,
                    hrv_at_time: None,
                    hr_at_time: None,
                    context_snapshot: None,
                },
            )
            .unwrap();
        }
        write_memory(
            &conn,
            &MemoryRecord {
                id: "sem1".to_string(),
                timestamp_ms: 0,
                session_id: "s".to_string(),
                kind: MemoryKind::Semantic,
                topic: "decision".to_string(),
                summary: "kept".to_string(),
                intent_at_time: None,
                confidence_at_time: None,
                hrv_at_time: None,
                hr_at_time: None,
                context_snapshot: None,
            },
        )
        .unwrap();

        prune_oldest_ambient(&conn, 3).unwrap();

        let ambient = query_memory(
            &conn,
            &MemoryQuery {
                kind: Some(MemoryKind::Ambient),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ambient.len(), 2);

        let semantic = query_memory(
            &conn,
            &MemoryQuery {
                kind: Some(MemoryKind::Semantic),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(semantic.len(), 1, "semantic records must not be pruned");
    }
}

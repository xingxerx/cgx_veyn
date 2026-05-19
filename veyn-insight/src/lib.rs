//! Pattern detection and physiological correlation analysis for VEYN memory.

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::json;
use veyn_schemas::PatternRecord;

/// Analyse memory records grouped by topic, computing physiological statistics.
///
/// Topics with fewer than `min_samples` records are excluded (default 1).
pub fn analyze_patterns(conn: &Connection, min_samples: Option<u32>) -> Result<Vec<PatternRecord>> {
    let min = min_samples.unwrap_or(1) as i64;
    let now = Utc::now().timestamp_millis();

    let topics: Vec<(String, u32, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT topic, COUNT(*) AS cnt, MAX(timestamp_ms) AS last_seen
             FROM veyn_memory
             GROUP BY topic
             HAVING cnt >= ?1
             ORDER BY last_seen DESC",
        )?;
        let x: Vec<_> = stmt
            .query_map(params![min], |row| {
                Ok((row.get(0)?, row.get::<_, u32>(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        x
    };

    let mut patterns = Vec::with_capacity(topics.len());

    for (topic, sample_count, last_seen_ms) in topics {
        let avg_hr: Option<f64> = conn
            .query_row(
                "SELECT AVG(hr_at_time) FROM veyn_memory \
                 WHERE topic=?1 AND hr_at_time IS NOT NULL",
                params![&topic],
                |r| r.get(0),
            )
            .unwrap_or(None);

        let avg_hrv: Option<f64> = conn
            .query_row(
                "SELECT AVG(hrv_at_time) FROM veyn_memory \
                 WHERE topic=?1 AND hrv_at_time IS NOT NULL",
                params![&topic],
                |r| r.get(0),
            )
            .unwrap_or(None);

        let intent_rows: Vec<(String, u32)> = {
            let mut stmt = conn.prepare(
                "SELECT intent_at_time, COUNT(*) AS cnt
                 FROM veyn_memory
                 WHERE topic=?1 AND intent_at_time IS NOT NULL
                 GROUP BY intent_at_time
                 ORDER BY cnt DESC",
            )?;
            let x: Vec<_> = stmt
                .query_map(params![&topic], |row| {
                    Ok((row.get(0)?, row.get::<_, u32>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            x
        };

        let total_intent: u32 = intent_rows.iter().map(|(_, c)| c).sum();
        let mut distribution = serde_json::Map::new();
        let mut dominant_intent: Option<String> = None;
        for (intent, count) in &intent_rows {
            let freq = if total_intent > 0 {
                *count as f64 / total_intent as f64
            } else {
                0.0
            };
            distribution.insert(intent.clone(), json!(freq));
            if dominant_intent.is_none() {
                dominant_intent = Some(intent.clone());
            }
        }

        let peak_hour: Option<u8> = {
            let mut stmt = conn.prepare(
                "SELECT (timestamp_ms / 3600000) % 24 AS hour, COUNT(*) AS cnt
                 FROM veyn_memory
                 WHERE topic=?1
                 GROUP BY hour
                 ORDER BY cnt DESC
                 LIMIT 1",
            )?;
            stmt.query_row(params![&topic], |r| r.get::<_, i64>(0).map(|h| h as u8))
                .ok()
        };

        patterns.push(PatternRecord {
            topic,
            sample_count,
            avg_hr,
            avg_hrv,
            dominant_intent,
            intent_distribution: serde_json::Value::Object(distribution),
            peak_hour,
            last_seen_ms,
            computed_at_ms: now,
        });
    }

    Ok(patterns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_patterns_empty_db() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Minimal veyn_memory table without migration dependency.
        conn.execute_batch(
            "CREATE TABLE veyn_memory (
                id TEXT PRIMARY KEY, timestamp_ms INTEGER NOT NULL, session_id TEXT NOT NULL,
                kind TEXT NOT NULL, topic TEXT NOT NULL, summary TEXT NOT NULL,
                intent_at_time TEXT, confidence_at_time REAL, hrv_at_time REAL,
                hr_at_time REAL, context_snapshot TEXT,
                outcome_rating TEXT, outcome_notes TEXT, outcome_at_ms INTEGER
            );",
        )
        .unwrap();
        let patterns = analyze_patterns(&conn, None).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn analyze_patterns_aggregates_correctly() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE veyn_memory (
                id TEXT PRIMARY KEY, timestamp_ms INTEGER NOT NULL, session_id TEXT NOT NULL,
                kind TEXT NOT NULL, topic TEXT NOT NULL, summary TEXT NOT NULL,
                intent_at_time TEXT, confidence_at_time REAL, hrv_at_time REAL,
                hr_at_time REAL, context_snapshot TEXT,
                outcome_rating TEXT, outcome_notes TEXT, outcome_at_ms INTEGER
            );
            INSERT INTO veyn_memory VALUES
              ('r1',1000,'s','semantic','deep-work','x','cognitive_load',0.9,45.0,70.0,NULL,NULL,NULL,NULL),
              ('r2',2000,'s','semantic','deep-work','y','cognitive_load',0.8,50.0,72.0,NULL,NULL,NULL,NULL),
              ('r3',3000,'s','semantic','review','z','neutral',0.5,NULL,65.0,NULL,NULL,NULL,NULL);",
        )
        .unwrap();
        let patterns = analyze_patterns(&conn, None).unwrap();
        assert_eq!(patterns.len(), 2);
        let dw = patterns.iter().find(|p| p.topic == "deep-work").unwrap();
        assert_eq!(dw.sample_count, 2);
        assert_eq!(dw.dominant_intent.as_deref(), Some("cognitive_load"));
        assert!((dw.avg_hr.unwrap() - 71.0).abs() < 0.01);
        assert!((dw.avg_hrv.unwrap() - 47.5).abs() < 0.01);
    }
}

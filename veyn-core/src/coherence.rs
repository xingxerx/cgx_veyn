//! 13.3 — Cryptographic Invariant Enforcement (DGK-IES)
//!
//! Continuously audits the semantic memory log to compute a Coherence (κ)
//! density score.  If κ drops below 0.92, a Mandatory Logical Reset (MLR) is
//! triggered: agent execution privileges are revoked and an MLR event is
//! broadcast on the context channel.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use tracing::{info, warn};
use veyn_schemas::ContextSnapshot;

// ── Constants ──────────────────────────────────────────────────────────────────

const KAPPA_THRESHOLD: f64 = 0.92;
/// How many recent semantic records to evaluate.
const AUDIT_WINDOW: usize = 50;
/// How often the audit loop runs.
const AUDIT_INTERVAL_SECS: u64 = 60;

// ── MLR state ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct CoherenceState {
    pub kappa: f64,
    pub mlr_triggered: bool,
    pub mlr_count: u64,
    pub last_audit_ms: i64,
}

// ── Coherence calculation ─────────────────────────────────────────────────────

/// Compute κ from a window of semantic memory records.
///
/// A record is *coherent* when it carries both a non-null intent and a
/// confidence_at_time ≥ 0.5 — i.e. it reflects a meaningful, grounded
/// physiological state rather than noise or a hallucinated summary.
///
/// κ = coherent_records / total_records  (over the last AUDIT_WINDOW entries)
fn compute_kappa(conn: &Connection) -> anyhow::Result<f64> {
    let mut stmt = conn.prepare(
        "SELECT confidence_at_time, intent_at_time
         FROM veyn_memory
         WHERE kind = 'semantic'
         ORDER BY timestamp_ms DESC
         LIMIT ?1",
    )?;

    let rows: Vec<(Option<f64>, Option<String>)> = stmt
        .query_map([AUDIT_WINDOW as i64], |row| {
            Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        return Ok(1.0); // no data → assume coherent
    }

    let coherent = rows.iter().filter(|(conf, intent)| {
        intent.as_deref().map(|i| !i.is_empty()).unwrap_or(false)
            && conf.map(|c| c >= 0.5).unwrap_or(false)
    }).count();

    Ok(coherent as f64 / rows.len() as f64)
}

// ── MLR broadcast ─────────────────────────────────────────────────────────────

fn broadcast_mlr(tx: &tokio::sync::broadcast::Sender<ContextSnapshot>, kappa: f64) {
    use chrono::Utc;
    use veyn_schemas::IntentCode;

    // Synthesise a special snapshot that signals the MLR to all subscribers.
    let snap = ContextSnapshot {
        timestamp_ms: Utc::now().timestamp_millis(),
        session_id: "MLR".to_string(),
        intent: format!("MANDATORY LOGICAL RESET — coherence κ={kappa:.3} < 0.92"),
        intent_code: IntentCode::StressResponse,
        confidence: 0.0,
        intent_confidence: 0.0,
        active_devices: vec![],
        state_deltas: vec![],
        baseline_delta: None,
        recording_session_id: None,
        temporal_patterns: vec![],
    };
    let _ = tx.send(snap);
}

// ── Background task ───────────────────────────────────────────────────────────

pub async fn run_coherence_audit(
    db: Arc<Mutex<Connection>>,
    state: Arc<Mutex<CoherenceState>>,
    context_tx: tokio::sync::broadcast::Sender<ContextSnapshot>,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(AUDIT_INTERVAL_SECS));
    ticker.tick().await; // discard immediate first tick

    loop {
        ticker.tick().await;

        let kappa = {
            let conn = db.lock().unwrap();
            match compute_kappa(&conn) {
                Ok(k) => k,
                Err(e) => {
                    warn!("coherence audit query failed: {e}");
                    continue;
                }
            }
        };

        let mlr = kappa < KAPPA_THRESHOLD;

        {
            let mut s = state.lock().unwrap();
            s.kappa = kappa;
            s.last_audit_ms = chrono::Utc::now().timestamp_millis();
            if mlr {
                s.mlr_triggered = true;
                s.mlr_count += 1;
                warn!(
                    kappa = kappa,
                    threshold = KAPPA_THRESHOLD,
                    mlr_count = s.mlr_count,
                    "DGK-IES: coherence below threshold — triggering MLR"
                );
                broadcast_mlr(&context_tx, kappa);
            } else {
                s.mlr_triggered = false;
                info!(kappa = kappa, "coherence audit passed");
            }
        }
    }
}

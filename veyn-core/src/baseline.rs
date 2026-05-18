//! BaselineEngine — rolling-window personal baseline statistics.
//!
//! Maintains an in-memory bounded deque of samples per (device_id, metric).
//! Computes mean, stddev, p10, and p90.  Minimum 7 days of data (or
//! MIN_SAMPLES) required before z-scores are returned.

use std::collections::{HashMap, VecDeque};

use tracing::debug;
use veyn_schemas::BaselineStats;

/// Rolling window length in days.
pub const WINDOW_DAYS: u32 = 30;

/// Samples per key kept in-memory (≈ 1 reading/min × 30 days).
const MAX_SAMPLES: usize = 60 * 24 * 30;

/// Minimum samples before baseline is considered valid.
const MIN_SAMPLES: usize = 60 * 24 * 7; // ≈ 7 days at 1/min

type Key = (String, String); // (device_id, metric)

pub struct BaselineEngine {
    samples: HashMap<Key, VecDeque<f64>>,
}

impl BaselineEngine {
    pub fn new() -> Self {
        Self {
            samples: HashMap::new(),
        }
    }

    /// Ingest a new sample.
    pub fn update(&mut self, device_id: &str, metric: &str, value: f64) {
        let key = (device_id.to_string(), metric.to_string());
        let deque = self.samples.entry(key).or_default();
        deque.push_back(value);
        if deque.len() > MAX_SAMPLES {
            deque.pop_front();
        }
    }

    /// Restore samples from SQLite (called at startup).
    pub fn load_samples(&mut self, device_id: &str, metric: &str, values: Vec<f64>) {
        let key = (device_id.to_string(), metric.to_string());
        let deque = self.samples.entry(key).or_default();
        for v in values {
            deque.push_back(v);
            if deque.len() > MAX_SAMPLES {
                deque.pop_front();
            }
        }
        debug!(
            device_id,
            metric,
            count = deque.len(),
            "baseline samples restored"
        );
    }

    /// Compute summary stats for a given key. Returns None when insufficient data.
    pub fn get_stats(&self, device_id: &str, metric: &str) -> Option<BaselineStats> {
        let key = (device_id.to_string(), metric.to_string());
        let deque = self.samples.get(&key)?;
        if deque.len() < MIN_SAMPLES {
            return None;
        }
        let stats = compute_stats(deque);
        Some(BaselineStats {
            device_id: device_id.to_string(),
            metric: metric.to_string(),
            mean: stats.mean,
            stddev: stats.stddev,
            p10: stats.p10,
            p90: stats.p90,
            sample_count: deque.len(),
            window_days: WINDOW_DAYS,
            updated_at: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Compute z-scores for all current metrics.  Only includes keys that have
    /// sufficient baseline data (MIN_SAMPLES).
    pub fn z_scores(&self, metric_state: &HashMap<String, f64>) -> HashMap<String, f64> {
        let mut out = HashMap::new();
        for (metric, &value) in metric_state {
            // Find any key that matches this metric (device_id may vary).
            for ((dev, met), deque) in &self.samples {
                if met == metric && deque.len() >= MIN_SAMPLES {
                    let stats = compute_stats(deque);
                    if stats.stddev > 0.0 {
                        let z = (value - stats.mean) / stats.stddev;
                        out.insert(format!("{dev}:{met}"), z);
                        // Also insert bare metric name for easy rule matching.
                        out.entry(metric.clone()).or_insert(z);
                    }
                    break;
                }
            }
        }
        out
    }
}

impl Default for BaselineEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Statistics helpers ────────────────────────────────────────────────────────

struct Stats {
    mean: f64,
    stddev: f64,
    p10: f64,
    p90: f64,
}

fn compute_stats(deque: &VecDeque<f64>) -> Stats {
    let n = deque.len() as f64;
    let mean = deque.iter().sum::<f64>() / n;
    let variance = deque.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let stddev = variance.sqrt();

    // Sort a copy for quantiles.
    let mut sorted: Vec<f64> = deque.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let p10 = percentile(&sorted, 10.0);
    let p90 = percentile(&sorted, 90.0);

    Stats {
        mean,
        stddev,
        p10,
        p90,
    }
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (pct / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with_samples(n: usize) -> BaselineEngine {
        let mut eng = BaselineEngine::new();
        // Feed n samples in [60, 100] range with mean ≈ 80.
        for i in 0..n {
            let v = 60.0 + (i % 41) as f64;
            eng.update("dev", "heart_rate", v);
        }
        eng
    }

    #[test]
    fn no_stats_below_min_samples() {
        let eng = engine_with_samples(100);
        assert!(eng.get_stats("dev", "heart_rate").is_none());
    }

    #[test]
    fn stats_available_above_min_samples() {
        let eng = engine_with_samples(MIN_SAMPLES + 10);
        let stats = eng.get_stats("dev", "heart_rate");
        assert!(stats.is_some());
        let s = stats.unwrap();
        assert!(s.mean > 0.0);
        assert!(s.stddev > 0.0);
        assert!(s.p10 <= s.p90);
    }

    #[test]
    fn z_scores_empty_when_insufficient() {
        let eng = engine_with_samples(100);
        let state: HashMap<String, f64> = [("heart_rate".to_string(), 90.0)].into();
        let z = eng.z_scores(&state);
        assert!(z.is_empty());
    }
}

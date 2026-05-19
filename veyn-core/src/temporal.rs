use std::collections::{HashMap, VecDeque};

use veyn_schemas::{TemporalSignal, TemporalTrend};

/// Sliding-window size for trend analysis (20 minutes).
const WINDOW_MS: u64 = 20 * 60 * 1_000;

/// Minimum samples required before computing a trend.
const MIN_SAMPLES: usize = 5;

/// Slope (units/min) below which a signal is considered Stable.
const STABLE_THRESHOLD: f64 = 0.05;

/// R² below which a linear fit is considered too noisy to assert a trend.
const R2_STABLE_THRESHOLD: f64 = 0.25;

/// Fraction of the window (from the end) used to detect spikes.
const SPIKE_RECENCY_FRAC: f64 = 0.25;

/// How many standard deviations the recent mean must differ from the older mean
/// to qualify as a spike or sharp decline.
const SPIKE_SIGMA: f64 = 1.8;

// ── TemporalEngine ────────────────────────────────────────────────────────────

pub struct TemporalEngine {
    samples: HashMap<String, VecDeque<(i64, f64)>>,
    window_ms: u64,
}

impl TemporalEngine {
    pub fn new() -> Self {
        Self {
            samples: HashMap::new(),
            window_ms: WINDOW_MS,
        }
    }

    /// Record a new sample for the given metric.
    pub fn push(&mut self, metric: &str, ts_ms: i64, value: f64) {
        let window = self.samples.entry(metric.to_string()).or_default();
        window.push_back((ts_ms, value));
        let cutoff = ts_ms - self.window_ms as i64;
        while window.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
            window.pop_front();
        }
    }

    /// Return a trend signal for every metric that has enough samples.
    pub fn get_patterns(&self) -> Vec<TemporalSignal> {
        self.samples
            .iter()
            .filter(|(_, s)| s.len() >= MIN_SAMPLES)
            .filter_map(|(metric, samples)| compute_signal(metric, samples))
            .collect()
    }
}

// ── Trend computation ─────────────────────────────────────────────────────────

fn compute_signal(metric: &str, samples: &VecDeque<(i64, f64)>) -> Option<TemporalSignal> {
    let n = samples.len() as f64;
    let t0 = samples.front()?.0;

    let xs: Vec<f64> = samples
        .iter()
        .map(|(t, _)| (*t - t0) as f64 / 1_000.0)
        .collect();
    let ys: Vec<f64> = samples.iter().map(|(_, v)| *v).collect();

    let x_mean = xs.iter().sum::<f64>() / n;
    let y_mean = ys.iter().sum::<f64>() / n;
    let y_std = (ys.iter().map(|y| (y - y_mean).powi(2)).sum::<f64>() / n).sqrt();

    let sxy: f64 = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| (x - x_mean) * (y - y_mean))
        .sum();
    let sxx: f64 = xs.iter().map(|x| (x - x_mean).powi(2)).sum();

    if sxx == 0.0 {
        return None;
    }

    let slope_per_sec = sxy / sxx;
    let slope_per_min = slope_per_sec * 60.0;

    let y_var: f64 = ys.iter().map(|y| (y - y_mean).powi(2)).sum();
    let r_squared = if y_var == 0.0 {
        1.0_f64
    } else {
        (sxy.powi(2) / (sxx * y_var)).min(1.0)
    };

    // Spike / sharp decline: compare mean of the recent SPIKE_RECENCY_FRAC of samples
    // against the mean of everything before that.
    let recent_count = ((samples.len() as f64 * SPIKE_RECENCY_FRAC).ceil() as usize).max(1);
    let older_count = samples.len() - recent_count;

    let trend = if older_count >= 2 {
        let older_mean = ys[..older_count].iter().sum::<f64>() / older_count as f64;
        let recent_mean = ys[older_count..].iter().sum::<f64>() / recent_count as f64;
        let delta = recent_mean - older_mean;
        let spike = y_std > 0.0 && delta.abs() / y_std >= SPIKE_SIGMA;

        if spike && delta > 0.0 {
            TemporalTrend::Spiking
        } else if spike && delta < 0.0 {
            TemporalTrend::Declining
        } else if r_squared < R2_STABLE_THRESHOLD || slope_per_min.abs() < STABLE_THRESHOLD {
            TemporalTrend::Stable
        } else if slope_per_min > 0.0 {
            // Distinguish Rising from Recovering: if the older half had a negative slope
            // and we're now rising, the signal is rebounding.
            let n_older = older_count as f64;
            let x_older_mean = xs[..older_count].iter().sum::<f64>() / n_older;
            let y_older_mean = older_mean;
            let older_sxy: f64 = xs[..older_count]
                .iter()
                .zip(ys[..older_count].iter())
                .map(|(x, y)| (x - x_older_mean) * (y - y_older_mean))
                .sum();
            let older_sxx: f64 = xs[..older_count]
                .iter()
                .map(|x| (x - x_older_mean).powi(2))
                .sum();
            let older_slope = if older_sxx > 0.0 {
                older_sxy / older_sxx
            } else {
                0.0
            };
            if older_slope < -STABLE_THRESHOLD / 60.0 {
                TemporalTrend::Recovering
            } else {
                TemporalTrend::Rising
            }
        } else {
            TemporalTrend::Falling
        }
    } else if r_squared < R2_STABLE_THRESHOLD || slope_per_min.abs() < STABLE_THRESHOLD {
        TemporalTrend::Stable
    } else if slope_per_min > 0.0 {
        TemporalTrend::Rising
    } else {
        TemporalTrend::Falling
    };

    let window_secs = samples
        .back()
        .map(|(t, _)| ((t - t0) / 1_000) as u32)
        .unwrap_or(0);

    Some(TemporalSignal {
        metric: metric.to_string(),
        trend,
        slope_per_min,
        window_secs,
        confidence: (r_squared as f32).clamp(0.05, 0.95),
        samples: samples.len(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn push_linear(engine: &mut TemporalEngine, metric: &str, start_ts: i64, n: usize, slope: f64) {
        for i in 0..n {
            let ts = start_ts + (i as i64) * 60_000; // 1-minute intervals
            let value = 70.0 + slope * i as f64;
            engine.push(metric, ts, value);
        }
    }

    #[test]
    fn no_patterns_below_min_samples() {
        let mut engine = TemporalEngine::new();
        engine.push("heart_rate", 0, 70.0);
        engine.push("heart_rate", 60_000, 71.0);
        assert!(engine.get_patterns().is_empty());
    }

    #[test]
    fn detects_rising_trend() -> anyhow::Result<()> {
        let mut engine = TemporalEngine::new();
        push_linear(&mut engine, "heart_rate", 0, 20, 0.5); // +0.5 bpm/min
        let patterns = engine.get_patterns();
        let sig = patterns
            .iter()
            .find(|s| s.metric == "heart_rate")
            .ok_or_else(|| anyhow::anyhow!("heart_rate signal not found"))?;
        assert_eq!(sig.trend, TemporalTrend::Rising);
        assert!(sig.slope_per_min > 0.0);
        Ok(())
    }

    #[test]
    fn detects_falling_trend() -> anyhow::Result<()> {
        let mut engine = TemporalEngine::new();
        push_linear(&mut engine, "hrv", 0, 20, -1.0); // -1 ms/min
        let patterns = engine.get_patterns();
        let sig = patterns
            .iter()
            .find(|s| s.metric == "hrv")
            .ok_or_else(|| anyhow::anyhow!("hrv signal not found"))?;
        assert_eq!(sig.trend, TemporalTrend::Falling);
        assert!(sig.slope_per_min < 0.0);
        Ok(())
    }

    #[test]
    fn detects_stable() -> anyhow::Result<()> {
        let mut engine = TemporalEngine::new();
        for i in 0..20 {
            engine.push("heart_rate", i * 60_000, 72.0); // perfectly flat
        }
        let patterns = engine.get_patterns();
        let sig = patterns
            .iter()
            .find(|s| s.metric == "heart_rate")
            .ok_or_else(|| anyhow::anyhow!("heart_rate signal not found"))?;
        assert_eq!(sig.trend, TemporalTrend::Stable);
        Ok(())
    }

    #[test]
    fn detects_spike() -> anyhow::Result<()> {
        let mut engine = TemporalEngine::new();
        let base_ts: i64 = 0;
        // 15 samples at ~70, then a sharp jump to ~110 in the last 4 samples
        for i in 0..15_i64 {
            engine.push("heart_rate", base_ts + i * 60_000, 70.0);
        }
        for i in 0..4_i64 {
            engine.push("heart_rate", base_ts + (15 + i) * 60_000, 115.0);
        }
        let patterns = engine.get_patterns();
        let sig = patterns
            .iter()
            .find(|s| s.metric == "heart_rate")
            .ok_or_else(|| anyhow::anyhow!("heart_rate signal not found"))?;
        assert_eq!(sig.trend, TemporalTrend::Spiking);
        Ok(())
    }

    #[test]
    fn window_evicts_old_samples() -> anyhow::Result<()> {
        let mut engine = TemporalEngine::new();
        // Push 30 samples spaced 2 minutes apart (60 minutes total — exceeds 20-min window)
        for i in 0..30_i64 {
            engine.push("heart_rate", i * 2 * 60_000, 70.0 + i as f64);
        }
        let patterns = engine.get_patterns();
        let sig = patterns
            .iter()
            .find(|s| s.metric == "heart_rate")
            .ok_or_else(|| anyhow::anyhow!("heart_rate signal not found"))?;
        // Only samples within the 20-min window should survive
        assert!(sig.samples <= 11); // 20 min / 2 min intervals + 1
        Ok(())
    }
}

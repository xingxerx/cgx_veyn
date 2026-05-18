use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::info;

use veyn_schemas::{IntentCode, VeynEvent};

const DEFAULT_DEBOUNCE_MS: u64 = 200;
const DEFAULT_EPSILON: f64 = 0.5;
const RULES_RELOAD_SECS: u64 = 30;

// ── Rule schema ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuleCondition {
    pub metric: String,
    /// "above" | "below" | "equals"
    pub op: String,
    pub threshold: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemanticRule {
    pub name: String,
    pub intent: String,
    /// Optional machine-readable code; defaults to Neutral if absent.
    #[serde(default)]
    pub intent_code: Option<IntentCode>,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub conditions: Vec<RuleCondition>,
}

fn default_confidence() -> f64 {
    0.7
}

#[derive(Debug, Deserialize, Default)]
struct RulesFile {
    #[serde(default)]
    rules: Vec<SemanticRule>,
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct CompressionEngine {
    last_values: HashMap<(String, String), f64>,
    last_emitted: HashMap<(String, String), Instant>,
    debounce_ms: HashMap<String, u64>,
    epsilons: HashMap<String, f64>,
    rules: Vec<SemanticRule>,
    rules_path: String,
    last_reload: Instant,
    pub raw_count: u64,
    pub passed_count: u64,
}

impl CompressionEngine {
    pub fn new(
        rules_path: String,
        debounce_ms: HashMap<String, u64>,
        epsilons: HashMap<String, f64>,
    ) -> Self {
        let rules = Self::load_rules(&rules_path).unwrap_or_default();
        info!(count = rules.len(), path = %rules_path, "semantic rules loaded");
        Self {
            last_values: HashMap::new(),
            last_emitted: HashMap::new(),
            debounce_ms,
            epsilons,
            rules,
            rules_path,
            last_reload: Instant::now(),
            raw_count: 0,
            passed_count: 0,
        }
    }

    fn load_rules(path: &str) -> Option<Vec<SemanticRule>> {
        let content = std::fs::read_to_string(path).ok()?;
        let file: RulesFile = toml::from_str(&content)
            .map_err(|e| tracing::warn!("rules.toml parse error: {}", e))
            .ok()?;
        Some(file.rules)
    }

    fn maybe_reload(&mut self) {
        if self.last_reload.elapsed() < Duration::from_secs(RULES_RELOAD_SECS) {
            return;
        }
        self.last_reload = Instant::now();
        if let Some(rules) = Self::load_rules(&self.rules_path) {
            info!(count = rules.len(), "hot-reloaded semantic rules");
            self.rules = rules;
        }
    }

    /// Returns `true` if the event is significant enough to pass through.
    ///
    /// Applies in order:
    /// 1. Magnitude threshold — skip micro-jitter below epsilon.
    /// 2. Temporal debounce — skip if the same device/metric was emitted recently.
    pub fn should_emit(&mut self, event: &VeynEvent) -> bool {
        self.raw_count += 1;
        self.maybe_reload();

        let key = (event.device_id.clone(), event.metric.clone());
        let now = Instant::now();

        let epsilon = self
            .epsilons
            .get(&event.metric)
            .copied()
            .unwrap_or(DEFAULT_EPSILON);

        if let Some(&prev) = self.last_values.get(&key) {
            if (event.value - prev).abs() < epsilon {
                return false;
            }
        }

        let debounce = Duration::from_millis(
            self.debounce_ms
                .get(&event.metric)
                .copied()
                .unwrap_or(DEFAULT_DEBOUNCE_MS),
        );

        if let Some(&prev_ts) = self.last_emitted.get(&key) {
            if prev_ts.elapsed() < debounce {
                return false;
            }
        }

        self.last_values.insert(key.clone(), event.value);
        self.last_emitted.insert(key, now);
        self.passed_count += 1;
        true
    }

    /// Synthesize intent from the current metric state and optional baseline z-scores.
    ///
    /// Z-score based Intero classification runs first when baseline data is available.
    /// Rule-based classification follows as fallback.
    ///
    /// Returns `(intent_string, intent_code, confidence_f32)`.
    pub fn synthesize(
        &self,
        state: &HashMap<String, f64>,
        z_scores: &HashMap<String, f64>,
    ) -> (String, IntentCode, f32) {
        // Intero physiological classification from z-scores (requires baseline).
        if !z_scores.is_empty() {
            if let Some(result) = classify_intero(state, z_scores) {
                return result;
            }
        }

        // Rule-based classification.
        for rule in &self.rules {
            if rule.conditions.iter().all(|c| {
                state.get(&c.metric).is_some_and(|&v| match c.op.as_str() {
                    "above" => v > c.threshold,
                    "below" => v < c.threshold,
                    "equals" => (v - c.threshold).abs() < 0.01,
                    _ => false,
                })
            }) {
                let code = rule.intent_code.clone().unwrap_or(IntentCode::Neutral);
                return (rule.intent.clone(), code, rule.confidence as f32);
            }
        }

        ("neutral".to_string(), IntentCode::Neutral, 0.5)
    }

    /// Fraction of raw events that passed the filter (0.0–1.0).
    pub fn compression_ratio(&self) -> f64 {
        if self.raw_count == 0 {
            1.0
        } else {
            self.passed_count as f64 / self.raw_count as f64
        }
    }
}

// ── Intero z-score classification ─────────────────────────────────────────────

/// Classify intent from physiological z-scores relative to personal baseline.
///
/// Uses multi-signal fusion: the core two-signal condition (HR + HRV) anchors
/// each state; every additional confirming signal (EEG bands, SpO2, RR) adds
/// a +0.05 confidence bonus, capped at the per-state ceiling.
///
/// Returns None if no pattern is confidently matched.
fn classify_intero(
    _state: &HashMap<String, f64>,
    z: &HashMap<String, f64>,
) -> Option<(String, IntentCode, f32)> {
    let hr_z = z.get("heart_rate").copied();
    let hrv_z = z.get("hrv").copied();
    let rr_z = z.get("respiratory_rate").copied();
    let spo2_z = z.get("spo2").copied();
    // EEG frequency band z-scores from the EEG/OSC adapter (Mind Monitor metrics).
    let beta_z = z.get("eeg_beta").copied();
    let alpha_z = z.get("eeg_alpha").copied();
    let theta_z = z.get("eeg_theta").copied();

    // +5 % per confirming signal beyond the anchor pair (HR + HRV).
    let bonus = |extra: u32| -> f64 { extra as f64 * 0.05 };

    // StressResponse: HR↑↑ + HRV↓↓ — optional EEG beta↑ and/or RR↑ confirm.
    if let (Some(hr), Some(hrv)) = (hr_z, hrv_z) {
        if hr > 1.5 && hrv < -1.0 {
            let mut extra: u32 = 0;
            if beta_z.is_some_and(|b| b > 1.0) {
                extra += 1;
            }
            if rr_z.is_some_and(|r| r > 0.5) {
                extra += 1;
            }
            if spo2_z.is_some_and(|s| s < -0.5) {
                extra += 1;
            }
            let base = ((hr - 1.5) + hrv.abs()) / 6.0;
            let conf = (base + bonus(extra)).clamp(0.55, 0.95) as f32;
            return Some((
                "stress_response".to_string(),
                IntentCode::StressResponse,
                conf,
            ));
        }
    }

    // CognitiveLoad: HR↑ (mild) + HRV↓ — EEG beta↑ and/or theta↑ (focused effort) confirm.
    if let (Some(hr), Some(hrv)) = (hr_z, hrv_z) {
        if (0.5..1.5).contains(&hr) && hrv < -0.5 {
            let rr_ok = rr_z.map(|r| r > -1.0).unwrap_or(true);
            if rr_ok {
                let mut extra: u32 = 0;
                if beta_z.is_some_and(|b| b > 0.5) {
                    extra += 1;
                }
                if theta_z.is_some_and(|t| t > 0.5) {
                    extra += 1;
                }
                let conf = (0.65 + bonus(extra)).clamp(0.5, 0.92) as f32;
                return Some((
                    "cognitive_load".to_string(),
                    IntentCode::CognitiveLoad,
                    conf,
                ));
            }
        }
    }

    // Fatigue: HR↓ + HRV↓ — EEG beta↓ (low arousal) and/or alpha↑ (drowsy) confirm.
    if let (Some(hr), Some(hrv)) = (hr_z, hrv_z) {
        if hr < -0.5 && hrv < -0.5 {
            let spo2_ok = spo2_z.map(|s| s > -1.0).unwrap_or(true);
            if spo2_ok {
                let mut extra: u32 = 0;
                if beta_z.is_some_and(|b| b < -0.5) {
                    extra += 1;
                }
                if alpha_z.is_some_and(|a| a > 0.5) {
                    extra += 1;
                }
                let conf = (0.65 + bonus(extra)).clamp(0.5, 0.90) as f32;
                return Some(("fatigue".to_string(), IntentCode::Fatigue, conf));
            }
        }
    }

    // Recovery: HR near-baseline + HRV↑ — EEG alpha↑ (relaxation) confirms.
    if let (Some(hr), Some(hrv)) = (hr_z, hrv_z) {
        if (-0.5..0.5).contains(&hr) && hrv > 0.5 {
            let mut extra: u32 = 0;
            if alpha_z.is_some_and(|a| a > 0.5) {
                extra += 1;
            }
            let conf = (0.60 + bonus(extra)).clamp(0.5, 0.90) as f32;
            return Some(("recovery".to_string(), IntentCode::Recovery, conf));
        }
    }

    // Approach: HR↑ + HRV↑ (positive engagement) — suppressed alpha (alert) confirms.
    if let (Some(hr), Some(hrv)) = (hr_z, hrv_z) {
        if hr > 0.5 && hrv > 0.5 {
            let mut extra: u32 = 0;
            if alpha_z.is_some_and(|a| a < -0.5) {
                extra += 1;
            }
            let conf = (0.60 + bonus(extra)).clamp(0.5, 0.90) as f32;
            return Some(("approach".to_string(), IntentCode::Approach, conf));
        }
    }

    // Avoidance: HR↓↓ + HRV↓↓ — elevated theta (emotional processing) confirms.
    if let (Some(hr), Some(hrv)) = (hr_z, hrv_z) {
        if hr < -1.0 && hrv < -1.5 {
            let mut extra: u32 = 0;
            if theta_z.is_some_and(|t| t > 1.0) {
                extra += 1;
            }
            let conf = (0.60 + bonus(extra)).clamp(0.5, 0.90) as f32;
            return Some(("avoidance".to_string(), IntentCode::Avoidance, conf));
        }
    }

    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use veyn_schemas::VeynEvent;

    fn make_engine() -> CompressionEngine {
        // Set debounce to 0 so tests are deterministic without sleeps.
        let debounce = [("heart_rate", 0u64)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let epsilons = [("heart_rate", 1.0f64)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        CompressionEngine::new("/nonexistent/rules.toml".to_string(), debounce, epsilons)
    }

    fn make_event(device: &str, metric: &str, value: f64) -> VeynEvent {
        VeynEvent::new(device, "mock", metric, value, "bpm")
    }

    #[test]
    fn should_emit_first_event_always() {
        let mut eng = make_engine();
        let ev = make_event("dev1", "heart_rate", 70.0);
        assert!(eng.should_emit(&ev));
    }

    #[test]
    fn should_drop_below_epsilon() {
        let mut eng = make_engine();
        assert!(eng.should_emit(&make_event("dev1", "heart_rate", 70.0)));
        // delta = 0.5 < epsilon 1.0 → drop
        assert!(!eng.should_emit(&make_event("dev1", "heart_rate", 70.5)));
    }

    #[test]
    fn should_pass_above_epsilon() {
        let mut eng = make_engine();
        assert!(eng.should_emit(&make_event("dev1", "heart_rate", 70.0)));
        // delta = 2.0 > epsilon 1.0 → pass
        assert!(eng.should_emit(&make_event("dev1", "heart_rate", 72.0)));
    }

    #[test]
    fn compression_ratio_reflects_drops() {
        let mut eng = make_engine();
        eng.should_emit(&make_event("dev1", "heart_rate", 70.0));
        eng.should_emit(&make_event("dev1", "heart_rate", 70.1));
        eng.should_emit(&make_event("dev1", "heart_rate", 70.2));
        // 1 pass, 2 drops → ratio 0.333…
        assert!((eng.compression_ratio() - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn synthesize_no_rules_returns_neutral() {
        let eng = make_engine();
        let state = HashMap::new();
        let (intent, code, conf) = eng.synthesize(&state, &HashMap::new());
        assert_eq!(intent, "neutral");
        assert_eq!(code, IntentCode::Neutral);
        assert!((conf - 0.5).abs() < 0.01);
    }

    #[test]
    fn synthesize_with_rules_matches_first() {
        use std::io::Write;
        // Write a temp rules file
        let dir = std::env::temp_dir();
        let path = dir.join("test_rules.toml");
        let content = r#"
[[rules]]
name = "high_hr"
intent = "stress_response"
intent_code = "stress_response"
confidence = 0.9
[[rules.conditions]]
metric = "heart_rate"
op = "above"
threshold = 100.0
"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();

        let eng = CompressionEngine::new(
            path.to_string_lossy().to_string(),
            HashMap::new(),
            HashMap::new(),
        );

        let mut state = HashMap::new();
        state.insert("heart_rate".to_string(), 110.0);
        let (intent, code, conf) = eng.synthesize(&state, &HashMap::new());
        assert_eq!(intent, "stress_response");
        assert_eq!(code, IntentCode::StressResponse);
        assert!((conf - 0.9).abs() < 0.01);
    }

    #[test]
    fn classify_stress_response_from_z_scores() {
        let eng = make_engine();
        let state = HashMap::new();
        let mut z = HashMap::new();
        z.insert("heart_rate".to_string(), 2.0);
        z.insert("hrv".to_string(), -1.5);
        let (_, code, conf) = eng.synthesize(&state, &z);
        assert_eq!(code, IntentCode::StressResponse);
        assert!(conf > 0.5);
    }

    #[test]
    fn classify_recovery_from_z_scores() {
        let eng = make_engine();
        let state = HashMap::new();
        let mut z = HashMap::new();
        z.insert("heart_rate".to_string(), 0.1);
        z.insert("hrv".to_string(), 1.2);
        let (_, code, _) = eng.synthesize(&state, &z);
        assert_eq!(code, IntentCode::Recovery);
    }

    #[test]
    fn classify_cognitive_load_from_z_scores() {
        let eng = make_engine();
        let state = HashMap::new();
        let mut z = HashMap::new();
        z.insert("heart_rate".to_string(), 1.0);
        z.insert("hrv".to_string(), -0.8);
        let (_, code, _) = eng.synthesize(&state, &z);
        assert_eq!(code, IntentCode::CognitiveLoad);
    }

    #[test]
    fn classify_fatigue_from_z_scores() {
        let eng = make_engine();
        let state = HashMap::new();
        let mut z = HashMap::new();
        z.insert("heart_rate".to_string(), -1.0);
        z.insert("hrv".to_string(), -1.2);
        let (_, code, _) = eng.synthesize(&state, &z);
        assert_eq!(code, IntentCode::Fatigue);
    }

    #[test]
    fn custom_serde_other_roundtrip() {
        let code = IntentCode::Other("custom_state".to_string());
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, r#""custom_state""#);
        let decoded: IntentCode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, IntentCode::Other("custom_state".to_string()));
    }

    #[test]
    fn known_variants_serde() {
        let pairs = [
            (IntentCode::Neutral, "neutral"),
            (IntentCode::CognitiveLoad, "cognitive_load"),
            (IntentCode::StressResponse, "stress_response"),
            (IntentCode::Approach, "approach"),
            (IntentCode::Avoidance, "avoidance"),
            (IntentCode::Fatigue, "fatigue"),
            (IntentCode::Recovery, "recovery"),
        ];
        for (variant, expected) in pairs {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            let decoded: IntentCode = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, variant);
        }
    }
}

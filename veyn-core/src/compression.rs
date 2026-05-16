use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::info;

use veyn_schemas::VeynEvent;

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

    /// Synthesize a human-readable intent string from the current metric state.
    /// Returns `(intent, confidence)`.
    pub fn synthesize(&self, state: &HashMap<String, f64>) -> (String, f64) {
        for rule in &self.rules {
            if rule.conditions.iter().all(|c| {
                state.get(&c.metric).is_some_and(|&v| match c.op.as_str() {
                    "above" => v > c.threshold,
                    "below" => v < c.threshold,
                    "equals" => (v - c.threshold).abs() < 0.01,
                    _ => false,
                })
            }) {
                return (rule.intent.clone(), rule.confidence);
            }
        }
        ("observing".to_string(), 0.5)
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

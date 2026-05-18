/// Integration tests for VEYN.
///
/// Exercises cross-crate public APIs:
/// - `veyn_schemas`: serialisation / deserialisation of all public types
/// - `veyn_plugins`: manifest loading and sha256 utility
use veyn_schemas::{
    BaselineStats, IntentCode, Session, SessionBoundary, SessionBoundaryKind, VeynEvent,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn mock_event(device_id: &str, metric: &str, value: f64, unit: &str) -> VeynEvent {
    VeynEvent::new(device_id, "mock", metric, value, unit)
}

// ── IntentCode serialisation ──────────────────────────────────────────────────

#[test]
fn intent_code_serialises_to_snake_case() {
    let cases: &[(IntentCode, &str)] = &[
        (IntentCode::Neutral, "neutral"),
        (IntentCode::CognitiveLoad, "cognitive_load"),
        (IntentCode::StressResponse, "stress_response"),
        (IntentCode::Approach, "approach"),
        (IntentCode::Avoidance, "avoidance"),
        (IntentCode::Fatigue, "fatigue"),
        (IntentCode::Recovery, "recovery"),
        (IntentCode::Other("custom_state".into()), "custom_state"),
    ];
    for (code, expected) in cases {
        let serialised = serde_json::to_string(code).unwrap();
        assert_eq!(
            serialised,
            format!("\"{}\"", expected),
            "wrong serialisation for {:?}",
            code
        );
    }
}

#[test]
fn intent_code_roundtrips_through_json() {
    let codes = [
        IntentCode::Neutral,
        IntentCode::CognitiveLoad,
        IntentCode::StressResponse,
        IntentCode::Approach,
        IntentCode::Avoidance,
        IntentCode::Fatigue,
        IntentCode::Recovery,
        IntentCode::Other("novel_state".into()),
    ];
    for code in &codes {
        let json = serde_json::to_string(code).unwrap();
        let decoded: IntentCode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, *code, "roundtrip failed for {:?}", code);
    }
}

#[test]
fn intent_code_deserialises_unknown_as_other() {
    let json = "\"unknown_future_variant\"";
    let decoded: IntentCode = serde_json::from_str(json).unwrap();
    assert_eq!(decoded, IntentCode::Other("unknown_future_variant".into()));
}

// ── VeynEvent ─────────────────────────────────────────────────────────────────

#[test]
fn veyn_event_roundtrips() {
    let ev = mock_event("ble:AA:BB", "hrv", 45.2, "ms");
    let json = serde_json::to_string(&ev).unwrap();
    let decoded: VeynEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.device_id, ev.device_id);
    assert_eq!(decoded.metric, ev.metric);
    assert!((decoded.value - ev.value).abs() < f64::EPSILON);
}

#[test]
fn veyn_event_with_meta_serialises() {
    let ev =
        mock_event("dev1", "heart_rate", 72.0, "bpm").with_meta("quality", serde_json::json!(0.95));
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("quality"));
    assert!(json.contains("0.95"));
}

// ── BaselineStats ─────────────────────────────────────────────────────────────

#[test]
fn baseline_stats_roundtrips() {
    let stats = BaselineStats {
        device_id: "dev1".into(),
        metric: "heart_rate".into(),
        mean: 62.5,
        stddev: 5.1,
        p10: 55.0,
        p90: 71.0,
        sample_count: 14400,
        window_days: 30,
        updated_at: chrono::Utc::now().timestamp_millis(),
    };
    let json = serde_json::to_string(&stats).unwrap();
    let decoded: BaselineStats = serde_json::from_str(&json).unwrap();
    assert!((decoded.mean - stats.mean).abs() < 0.001);
    assert_eq!(decoded.device_id, stats.device_id);
    assert_eq!(decoded.sample_count, stats.sample_count);
}

// ── Session / SessionBoundary ─────────────────────────────────────────────────

#[test]
fn session_new_creates_valid_session() {
    let s = Session::new("Morning HRV", vec!["ble:AA".into(), "osc:1".into()]);
    assert!(!s.id.is_empty());
    assert_eq!(s.label, "Morning HRV");
    assert!(s.ended_at.is_none());
    assert_eq!(s.active_device_ids.len(), 2);
}

#[test]
fn session_roundtrips_through_json() {
    let s = Session::new("Test session", vec!["dev1".into()]);
    let json = serde_json::to_string(&s).unwrap();
    let decoded: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, s.id);
    assert_eq!(decoded.label, s.label);
    assert!(decoded.ended_at.is_none());
}

#[test]
fn session_boundary_start_serialises() {
    let b = SessionBoundary {
        session_id: "sess-1".into(),
        kind: SessionBoundaryKind::Start,
        ts: 0,
        label: "test".into(),
    };
    let json = serde_json::to_string(&b).unwrap();
    assert!(json.contains("start"));
    assert!(json.contains("sess-1"));
}

#[test]
fn session_boundary_end_serialises() {
    let b = SessionBoundary {
        session_id: "sess-1".into(),
        kind: SessionBoundaryKind::End,
        ts: 1,
        label: "test".into(),
    };
    let json = serde_json::to_string(&b).unwrap();
    assert!(json.contains("end"));
}

// ── Plugin manifest / SHA-256 ─────────────────────────────────────────────────

#[test]
fn plugin_manifest_load_missing_returns_error() {
    let result = veyn_plugins::load_manifest(std::path::Path::new("/nonexistent/plugin.toml"));
    assert!(result.is_err(), "should error on missing manifest");
}

#[test]
fn plugin_sha256_file_missing_returns_error() {
    let result = veyn_plugins::sha256_file(std::path::Path::new("/nonexistent/plugin.wasm"));
    assert!(result.is_err(), "should error on missing file");
}

#[test]
fn plugin_sha256_matches_known_content() {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(b"hello wasm").unwrap();
    let result = veyn_plugins::sha256_file(tmp.path()).unwrap();
    // SHA-256("hello wasm") = 6f...
    assert_eq!(result.len(), 64, "SHA-256 hex should be 64 chars");
    assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
}

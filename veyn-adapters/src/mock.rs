use anyhow::Result;
use async_trait::async_trait;
use rand::Rng;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::info;
use veyn_schemas::VeynEvent;

use crate::VeynAdapter;

/// Generates synthetic biometric events at ~2 Hz for development and testing.
pub struct MockAdapter;

const DEVICE_ID: &str = "mock-device-001";

#[async_trait]
impl VeynAdapter for MockAdapter {
    fn name(&self) -> &str {
        "mock"
    }

    async fn start(&self, tx: mpsc::Sender<VeynEvent>) -> Result<()> {
        info!("mock adapter started — emitting synthetic biometric events");

        let metrics: &[(&str, f64, f64, &str)] = &[
            ("heart_rate", 55.0, 100.0, "bpm"),
            ("hrv", 20.0, 80.0, "ms"),
            ("spo2", 95.0, 100.0, "%"),
            ("steps", 0.0, 500.0, "steps"),
            ("respiratory_rate", 12.0, 20.0, "brpm"),
            ("skin_temperature", 35.5, 37.5, "°C"),
            ("active_energy", 0.0, 50.0, "kcal"),
        ];

        loop {
            let event = {
                let mut rng = rand::thread_rng();
                let (metric, min, max, unit) = metrics[rng.gen_range(0..metrics.len())];
                let value = rng.gen_range(min..=max);
                VeynEvent::new(DEVICE_ID, "mock", metric, value, unit)
            };

            if tx.send(event).await.is_err() {
                break;
            }

            sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }
}

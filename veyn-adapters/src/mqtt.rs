use anyhow::{Context, Result};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio::sync::broadcast;
use tokio::time::Duration;
use tracing::{error, info, warn};
use veyn_schemas::VeynEvent;

/// Parse an `mqtt://host:port` URL into (host, port).
fn parse_url(url: &str) -> Result<(String, u16)> {
    let stripped = url.strip_prefix("mqtt://").unwrap_or(url);
    let (host, port_str) = stripped.rsplit_once(':').unwrap_or((stripped, "1883"));
    let port: u16 = port_str
        .parse()
        .with_context(|| format!("invalid MQTT port in URL: {}", url))?;
    Ok((host.to_string(), port))
}

/// Subscribe to the event broadcast and publish each event to the MQTT broker
/// at topic `veyn/<device_id>/<metric>` with a JSON payload.
///
/// Reconnects automatically when the broker is unavailable.
pub async fn run(mut rx: broadcast::Receiver<VeynEvent>, url: String) -> Result<()> {
    let (host, port) = parse_url(&url)?;
    info!(broker = %url, "MQTT bridge starting");

    let mut opts = MqttOptions::new("veyn-daemon", &host, port);
    opts.set_keep_alive(Duration::from_secs(30));

    let (client, mut eventloop) = AsyncClient::new(opts, 64);

    // Drive the rumqttc event loop in a background task; it handles reconnection.
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(_) => {}
                Err(e) => {
                    error!("MQTT event loop: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });

    info!(broker = %url, "MQTT bridge connected");

    loop {
        match rx.recv().await {
            Ok(event) => {
                let topic = format!("veyn/{}/{}", event.device_id, event.metric);
                let payload = match serde_json::to_string(&event) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("MQTT serialize error: {}", e);
                        continue;
                    }
                };
                if let Err(e) = client
                    .publish(&topic, QoS::AtMostOnce, false, payload)
                    .await
                {
                    error!("MQTT publish error: {}", e);
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("MQTT subscriber lagged {} events", n);
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

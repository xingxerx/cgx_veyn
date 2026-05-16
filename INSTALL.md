# VEYN Installation Guide

This guide covers installing and configuring VEYN on your system.

## Prerequisites

### Required

- **Rust toolchain** (version 1.70 or later)
  - Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
  - Verify: `rustc --version`

- **Operating System**: Linux, macOS, or Windows 10+

### Optional (Feature-Specific)

#### BLE Adapter (for wearable devices)
- **Linux**: `libudev-dev`, `libdbus-1-dev`
  ```bash
  sudo apt-get install libudev-dev libdbus-1-dev  # Debian/Ubuntu
  sudo dnf install libudev-devel dbus-devel       # Fedora/RHEL
  ```
- **macOS**: Native IOKit support (no extra deps)
- **Windows**: Bluetooth radio with drivers installed

#### EEG/OSC Adapter
- OSC-compatible BCI device (e.g., OpenBCI, NeuroSky)
- Network connectivity to device

#### MQTT Bridge
- MQTT broker (e.g., Mosquitto, EMQX)
  ```bash
  sudo apt-get install mosquitto  # Debian/Ubuntu
  ```

#### WASM Plugins
- WASM runtime is bundled; no extra installation needed
- For building plugins: `rustup target add wasm32-unknown-unknown`

## Installation Methods

### Method 1: From Source (Recommended for Development)

```bash
# Clone repository
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn

# Build release binary
cargo build --release

# Binary location
./target/release/veyn-core
```

### Method 2: Cargo Install (When Published)

```bash
cargo install veyn-core
```

### Method 3: Pre-built Binaries (When Available)

Download from GitHub Releases for your platform.

## Configuration

### Environment Variables

Copy `.env.example` to `.env` and customize:

```bash
cp .env.example .env
```

Key variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `VEYN_PORT` | 7700 | API server port |
| `VEYN_HK_PORT` | 7701 | HealthKit relay port |
| `VEYN_MOCK` | false | Enable mock data generator |
| `VEYN_BLE` | false | Enable BLE adapter |
| `VEYN_EEG` | false | Enable EEG/OSC adapter |
| `VEYN_LOG` | veyn-events.jsonl | Event log path |
| `VEYN_PLUGINS_DIR` | plugins | WASM plugins directory |
| `VEYN_MQTT_URL` | (unset) | MQTT broker URL |
| `VEYN_PRESENCE_TIMEOUT` | 30 | Presence timeout (seconds) |

### Systemd Service (Linux)

Create `/etc/systemd/system/veyn.service`:

```ini
[Unit]
Description=VEYN Daemon
After=network.target

[Service]
Type=simple
User=veyn
ExecStart=/opt/veyn/veyn-core
Environment=VEYN_BLE=true
Environment=RUST_LOG=info
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable veyn
sudo systemctl start veyn
```

### Launchd Service (macOS)

Create `~/Library/LaunchAgents/com.cgx.veyn.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.cgx.veyn</string>
    <key>ProgramArguments</key>
    <array>
        <string>/opt/veyn/veyn-core</string>
        <string>--config</string>
        <string>/Users/yourname/.veyn/config.toml</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Load:

```bash
launchctl load ~/Library/LaunchAgents/com.cgx.veyn.plist
```

## Permissions

### Linux: Device Access

For BLE and HID access, add user to groups:

```bash
sudo usermod -aG bluetooth,input $USER
```

Log out and back in for group changes to take effect.

### macOS: Privacy Permissions

VEYN may require permissions for:
- **Bluetooth**: System Preferences → Security & Privacy → Privacy → Bluetooth
- **Network**: Automatically granted for localhost connections

### Windows: Device Access

Run as Administrator or grant specific device permissions via Device Manager.

## Verification

### Check Installation

```bash
# Run with mock data
VEYN_MOCK=true veyn-core &

# Test health endpoint
curl http://localhost:7700/health

# Expected response:
# {"status":"ok","version":"0.1.0","uptime_seconds":5,"events_total":10}
```

### Check Logs

```bash
# With default logging
tail -f veyn-events.jsonl

# Or check stderr output if running in foreground
```

## Troubleshooting

### Port Already in Use

```bash
# Find process using port 7700
lsof -i :7700  # Linux/macOS
netstat -ano | findstr :7700  # Windows

# Change port via environment variable
export VEYN_PORT=7701
```

### BLE Not Working

- Ensure Bluetooth is enabled on your system
- Check that you're in the `bluetooth` group (Linux)
- Try restarting the Bluetooth service:
  ```bash
  sudo systemctl restart bluetooth  # Linux
  ```

### Permission Denied on Device Files

```bash
# Linux: Check device permissions
ls -la /dev/input/event*

# Add udev rules if needed
echo 'KERNEL=="event*", SUBSYSTEM=="input", MODE="0666"' | sudo tee /etc/udev/rules.d/99-input.rules
sudo udevadm control --reload-rules
```

### High Memory Usage

- Reduce event buffer size if needed (requires code change)
- Limit presence detection frequency
- Check for memory leaks with `valgrind` or similar tools

## Next Steps

- Read the [README.md](./README.md) for usage examples
- Explore the [CONTRIBUTING.md](./CONTRIBUTING.md) if you want to contribute
- Check the [CHANGELOG.md](./CHANGELOG.md) for recent updates
- Join the community discussions on GitHub

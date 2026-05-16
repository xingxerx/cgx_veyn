# VEYN — Installation Guide

## Prerequisites

### Rust toolchain

VEYN requires **Rust 1.75 or later** (stable channel).

Install via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
```

Verify:

```bash
rustc --version   # >= 1.75.0
cargo --version
```

### System libraries (Linux)

```bash
# Ubuntu / Debian
sudo apt install libdbus-1-dev pkg-config

# Fedora / RHEL
sudo dnf install dbus-devel pkgconf-pkg-config

# Arch
sudo pacman -S dbus pkgconf
```

If you plan to use the **BLE adapter** (VEYN_BLE=true), also install:

```bash
# Ubuntu / Debian
sudo apt install libbluetooth-dev

# Fedora
sudo dnf install bluez-libs-devel
```

### System libraries (macOS)

No extra steps — Homebrew's `libdbus` and `bluez` equivalents are handled via
the macOS system frameworks used by btleplug.

### System libraries (Windows)

Install the [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
with the "Desktop development with C++" workload. The BLE adapter requires
Windows 10 version 1903 or later.

---

## Building

```bash
# Clone
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn

# Debug build (fast, for development)
cargo build -p veyn-core

# Release build (optimized, for production)
cargo build -p veyn-core --release
```

The binary is at `target/debug/veyn-core` or `target/release/veyn-core`.

---

## Running

```bash
# Quickstart with synthetic mock data (no hardware required)
VEYN_MOCK=true cargo run -p veyn-core

# Or with a release build
VEYN_MOCK=true ./target/release/veyn-core

# With a config file
./target/release/veyn-core --config veyn.toml

# Override port
./target/release/veyn-core --port 8080

# Disable auth (development only)
./target/release/veyn-core --no-auth
```

On first run, VEYN generates an auth token at `~/.local/share/veyn/token`
(chmod 600). Include it in all API requests:

```bash
TOKEN=$(cat ~/.local/share/veyn/token)
curl -H "Authorization: Bearer $TOKEN" http://localhost:7700/v1/health
```

---

## Building WASM Plugins

WASM plugins require the `wasm32-unknown-unknown` target:

```bash
rustup target add wasm32-unknown-unknown

# Build an example plugin
cargo build --target wasm32-unknown-unknown --release -p garmin-connect
```

Copy the resulting `.wasm` file next to the plugin's `plugin.toml`.

---

## Environment Variables

See [`.env.example`](.env.example) for the full list. All variables can also
be set in `veyn.toml` — see [`veyn.toml.example`](veyn.toml.example).

---

## Verifying the Installation

```bash
TOKEN=$(cat ~/.local/share/veyn/token)
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:7700/v1/health | jq .
# Expected: { "status": "ok", ... }

curl -s -H "Authorization: Bearer $TOKEN" http://localhost:7700/v1/context/current | jq .
# Expected: context snapshot with intent + confidence
```

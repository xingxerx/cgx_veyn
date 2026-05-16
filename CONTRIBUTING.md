# Contributing to VEYN

Thank you for your interest in contributing to VEYN! This document provides guidelines and instructions for contributing.

## Code of Conduct

- Be respectful and inclusive
- Focus on constructive feedback
- Welcome newcomers and help them learn

## How to Contribute

### Reporting Bugs

1. Check existing issues first
2. Create a new issue with:
   - Clear description
   - Steps to reproduce
   - Expected vs actual behavior
   - Environment details (OS, Rust version, etc.)

### Suggesting Features

1. Open an issue describing the feature
2. Explain the use case
3. Discuss implementation approach if you have ideas

### Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Write or update tests
5. Ensure `cargo fmt` and `cargo clippy` pass
6. Commit with clear messages
7. Open a PR with description of changes

## Development Setup

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Linux, macOS, or Windows
- For BLE: Bluetooth adapter and system libraries
- For EEG: OSC-compatible BCI device

### Building

```bash
# Clone and enter directory
git clone https://github.com/xingxerx/cgx-veyn
cd cgx-veyn

# Build all workspace members
cargo build

# Run with mock data for testing
VEYN_MOCK=true cargo run -p veyn-core
```

### Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p veyn-core

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Format code
cargo fmt

# Lint code
cargo clippy --all-targets --all-features -- -D warnings
```

## Architecture Overview

VEYN is organized as a Cargo workspace with these crates:

- **veyn-schemas**: Shared data types and event definitions
- **veyn-adapters**: Device integration layers (BLE, HealthKit, EEG, Mock)
- **veyn-core**: Main daemon with API server, dispatcher, presence detection
- **veyn-plugins**: WASM runtime and plugin management
- **sdk**: Client libraries for integrating with VEYN

## Adding a New Adapter

1. Create new module in `veyn-adapters/src/`
2. Implement the `VeynAdapter` trait:
   ```rust
   #[async_trait]
   pub trait VeynAdapter {
       fn name(&self) -> &str;
       async fn start(&self, tx: Sender<VeynEvent>) -> Result<()>;
   }
   ```
3. Register in `veyn-adapters/src/lib.rs`
4. Add configuration option in `veyn-core/src/config.rs`
5. Update documentation

## Adding a WASM Plugin

1. Create plugin crate with `veyn-sdk` dependency
2. Define plugin manifest (`plugin.toml`)
3. Implement required exports (`veyn_init`, `veyn_poll`, etc.)
4. Build for `wasm32-unknown-unknown` target
5. Place in plugins directory

## Documentation

- Update README.md for user-facing changes
- Add inline comments for complex logic
- Update API documentation if endpoints change
- Include examples where helpful

## Release Process

1. Update CHANGELOG.md
2. Bump version in Cargo.toml files
3. Create git tag
4. Publish to crates.io (if applicable)
5. Create GitHub release

## Questions?

Open an issue or reach out to the maintainers. We're happy to help!

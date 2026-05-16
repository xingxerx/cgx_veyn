# VEYN Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Presence detection system with device polling and state tracking
- WebSocket event streaming with history replay
- HealthKit TCP relay for bidirectional health data
- MQTT bridge for smart-home integration
- WASM plugin runtime for extensible device adapters
- Live dashboard at `/` endpoint
- REST API endpoints: `/health`, `/events/recent`, `/metrics/:metric`, `/devices`, `/plugins`, `/presence`, `/gestures/recent`
- Notification system via `/notify` POST endpoint
- Configuration via environment variables

### Changed
- Unified event schema across all adapter types
- Improved error handling in adapter lifecycle

### Fixed
- Device hot-plug detection stability
- WebSocket subscriber lag handling

## [0.1.0] - 2024-01-01

### Added
- Initial release
- Core daemon with REST + WebSocket API
- Mock adapter for testing
- BLE adapter for wearable devices
- EEG/OSC adapter for BCI input
- HealthKit adapter for iOS health data
- Basic presence detection
- JSONL event logging

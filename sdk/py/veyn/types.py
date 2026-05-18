"""Dataclass types mirroring the Rust schema in veyn-schemas/src/lib.rs."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


# ── IntentCode ────────────────────────────────────────────────────────────────

# Serialised as plain strings; known variants are constants below.
INTENT_NEUTRAL = "neutral"
INTENT_COGNITIVE_LOAD = "cognitive_load"
INTENT_STRESS_RESPONSE = "stress_response"
INTENT_APPROACH = "approach"
INTENT_AVOIDANCE = "avoidance"
INTENT_FATIGUE = "fatigue"
INTENT_RECOVERY = "recovery"

# Type alias — any string is valid (Other pass-through)
IntentCode = str


# ── StateDelta ────────────────────────────────────────────────────────────────

@dataclass
class StateDelta:
    """One metric observation included in a context snapshot."""

    device_id: str
    metric: str
    value: float
    unit: str
    ts: int
    source_class: str = ""

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "StateDelta":
        return cls(
            device_id=d["device_id"],
            metric=d["metric"],
            value=float(d["value"]),
            unit=d["unit"],
            ts=int(d["ts"]),
            source_class=d.get("source_class", ""),
        )


# ── ContextSnapshot ───────────────────────────────────────────────────────────

@dataclass
class ContextSnapshot:
    """Semantic context snapshot — the AI-ready world-state summary."""

    timestamp_ms: int
    session_id: str
    intent: str
    intent_code: IntentCode
    confidence: float
    intent_confidence: float
    active_devices: list[str]
    state_deltas: list[StateDelta]
    baseline_delta: dict[str, float] | None = None
    recording_session_id: str | None = None

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "ContextSnapshot":
        return cls(
            timestamp_ms=int(d["timestamp_ms"]),
            session_id=d["session_id"],
            intent=d["intent"],
            intent_code=d.get("intent_code", INTENT_NEUTRAL),
            confidence=float(d["confidence"]),
            intent_confidence=float(d.get("intent_confidence", 0.0)),
            active_devices=list(d.get("active_devices", [])),
            state_deltas=[StateDelta.from_dict(sd) for sd in d.get("state_deltas", [])],
            baseline_delta=d.get("baseline_delta"),
            recording_session_id=d.get("recording_session_id"),
        )


# ── Session ───────────────────────────────────────────────────────────────────

@dataclass
class Session:
    """A named recording session with optional annotations."""

    id: str
    label: str
    started_at: int
    ended_at: int | None
    active_device_ids: list[str]
    notes: str | None = None

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "Session":
        return cls(
            id=d["id"],
            label=d["label"],
            started_at=int(d["started_at"]),
            ended_at=int(d["ended_at"]) if d.get("ended_at") is not None else None,
            active_device_ids=list(d.get("active_device_ids", [])),
            notes=d.get("notes"),
        )


# ── BaselineStats ─────────────────────────────────────────────────────────────

@dataclass
class BaselineStats:
    """Rolling-window baseline statistics for a single (device_id, metric) pair."""

    device_id: str
    metric: str
    mean: float
    stddev: float
    p10: float
    p90: float
    sample_count: int
    window_days: int
    updated_at: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "BaselineStats":
        return cls(
            device_id=d["device_id"],
            metric=d["metric"],
            mean=float(d["mean"]),
            stddev=float(d["stddev"]),
            p10=float(d["p10"]),
            p90=float(d["p90"]),
            sample_count=int(d["sample_count"]),
            window_days=int(d["window_days"]),
            updated_at=int(d["updated_at"]),
        )


# ── VeynEvent ─────────────────────────────────────────────────────────────────

@dataclass
class VeynEvent:
    """Unified event emitted by every adapter, regardless of source."""

    id: str
    ts: int
    device_id: str
    source: str
    metric: str
    value: float
    unit: str
    meta: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "VeynEvent":
        return cls(
            id=d["id"],
            ts=int(d["ts"]),
            device_id=d["device_id"],
            source=d["source"],
            metric=d["metric"],
            value=float(d["value"]),
            unit=d["unit"],
            meta=d.get("meta", {}),
        )


# ── VeynDevice ────────────────────────────────────────────────────────────────

@dataclass
class VeynDevice:
    """Registered device with connection state."""

    id: str
    name: str
    source: str
    state: str  # "connected" | "disconnected" | "scanning"
    last_seen: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "VeynDevice":
        return cls(
            id=d["id"],
            name=d["name"],
            source=d["source"],
            state=d["state"],
            last_seen=int(d["last_seen"]),
        )


# ── ContextTier ───────────────────────────────────────────────────────────────

# Context tier constants — controls which data layer a token exposes.
# Configure daemon default in veyn.toml: context_tier = "semantic"
# Set token ceiling via scope "tier:semantic" in tokens.json.
TIER_RAW      = "raw"       # full VeynEvent stream, unfiltered
TIER_FILTERED = "filtered"  # compression-filtered events only
TIER_SEMANTIC = "semantic"  # ContextSnapshot only; no raw events (AI agents)

# Type alias
ContextTier = str


# ── SessionFrame ──────────────────────────────────────────────────────────────

@dataclass
class SessionFrame:
    """Wraps a VeynEvent when a recording session is active (WS Raw/Filtered tier)."""

    session_id: str
    channel: str
    event: VeynEvent

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "SessionFrame":
        return cls(
            session_id=d["session_id"],
            channel=d["channel"],
            event=VeynEvent.from_dict(d["event"]),
        )


# ── BaselineHistoryPoint ──────────────────────────────────────────────────────

@dataclass
class BaselineDailyPoint:
    """One UTC-day mean value from the baseline history endpoint."""

    ts: int
    mean: float

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "BaselineDailyPoint":
        return cls(ts=int(d["ts"]), mean=float(d["mean"]))

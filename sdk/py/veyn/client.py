"""Async VEYN daemon client backed by aiohttp (HTTP/SSE) and websockets (WS)."""

from __future__ import annotations

import asyncio
import json
from typing import Any, Callable

import aiohttp
import websockets
import websockets.exceptions

from .types import (
    BaselineStats,
    ContextSnapshot,
    MemoryQuery,
    MemoryRecord,
    OutcomeRating,
    PatternRecord,
    Session,
    TemporalSignal,
    VeynDevice,
    VeynEvent,
)


class VeynClient:
    """Async client for the VEYN daemon REST, SSE, and WebSocket APIs.

    Can be used as an async context manager::

        async with VeynClient("http://localhost:8888", "mytoken") as client:
            snap = await client.get_context()
    """

    def __init__(self, base_url: str, token: str) -> None:
        self._base_url = base_url.rstrip("/")
        self._token = token
        self._session: aiohttp.ClientSession | None = None

    # ── Lifecycle ──────────────────────────────────────────────────────────────

    async def __aenter__(self) -> "VeynClient":
        self._session = aiohttp.ClientSession(
            headers={"Authorization": f"Bearer {self._token}"},
        )
        return self

    async def __aexit__(self, *_: Any) -> None:
        if self._session is not None:
            await self._session.close()
            self._session = None

    def _get_session(self) -> aiohttp.ClientSession:
        if self._session is None:
            self._session = aiohttp.ClientSession(
                headers={"Authorization": f"Bearer {self._token}"},
            )
        return self._session

    def _url(self, path: str) -> str:
        return f"{self._base_url}{path}"

    def _ws_url(self, path: str) -> str:
        base = self._base_url.replace("https://", "wss://").replace("http://", "ws://")
        return f"{base}{path}"

    # ── Health ─────────────────────────────────────────────────────────────────

    async def get_health(self) -> dict[str, Any]:
        async with self._get_session().get(self._url("/v1/health")) as resp:
            resp.raise_for_status()
            return await resp.json()  # type: ignore[no-any-return]

    # ── Context ────────────────────────────────────────────────────────────────

    async def get_context(self) -> ContextSnapshot:
        async with self._get_session().get(self._url("/v1/context/current")) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return ContextSnapshot.from_dict(data)

    async def get_context_history(self, n: int = 10) -> list[ContextSnapshot]:
        async with self._get_session().get(
            self._url("/v1/context/history"), params={"n": n}
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return [ContextSnapshot.from_dict(item) for item in data]

    async def subscribe(
        self,
        callback: Callable[[ContextSnapshot], Any],
        intents: list[str] | None = None,
        min_confidence: float | None = None,
        source_class: list[str] | None = None,
    ) -> None:
        """Stream SSE context snapshots, calling *callback* for each one.

        Runs until the current task is cancelled.  Auto-reconnects with a 1 s
        backoff on connection errors.
        """
        params: dict[str, str] = {}
        if intents:
            params["intents"] = ",".join(intents)
        if min_confidence is not None:
            params["min_confidence"] = str(min_confidence)
        if source_class:
            params["source_class"] = ",".join(source_class)

        url = self._url("/v1/context/subscribe")
        headers = {
            "Accept": "text/event-stream",
            "Cache-Control": "no-cache",
            "Authorization": f"Bearer {self._token}",
        }

        while True:
            try:
                async with aiohttp.ClientSession() as session:
                    async with session.get(
                        url, headers=headers, params=params
                    ) as resp:
                        resp.raise_for_status()
                        data_buf = ""
                        async for line_bytes in resp.content:
                            line = line_bytes.decode("utf-8").rstrip("\r\n")
                            if line.startswith("data:"):
                                data_buf = line[5:].strip()
                            elif line == "" and data_buf:
                                try:
                                    snap = ContextSnapshot.from_dict(
                                        json.loads(data_buf)
                                    )
                                    result = callback(snap)
                                    if asyncio.iscoroutine(result):
                                        await result
                                except (json.JSONDecodeError, KeyError):
                                    pass
                                data_buf = ""
            except asyncio.CancelledError:
                raise
            except Exception:
                await asyncio.sleep(1.0)

    # ── Events ─────────────────────────────────────────────────────────────────

    async def get_events(self, limit: int = 100) -> list[VeynEvent]:
        async with self._get_session().get(
            self._url("/v1/events/recent"), params={"limit": limit}
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return [VeynEvent.from_dict(item) for item in data]

    # ── Devices ────────────────────────────────────────────────────────────────

    async def get_devices(self) -> list[VeynDevice]:
        async with self._get_session().get(self._url("/v1/devices")) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return [VeynDevice.from_dict(item) for item in data]

    # ── Sessions ───────────────────────────────────────────────────────────────

    async def start_session(
        self, label: str, annotation: str | None = None
    ) -> Session:
        body: dict[str, str] = {"label": label}
        if annotation is not None:
            body["annotation"] = annotation
        async with self._get_session().post(
            self._url("/v1/session/start"), json=body
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return Session.from_dict(data)

    async def stop_session(self) -> Session:
        async with self._get_session().post(
            self._url("/v1/session/stop"), json={}
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return Session.from_dict(data)

    async def get_session(self, session_id: str) -> Session:
        async with self._get_session().get(
            self._url(f"/v1/session/{session_id}")
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return Session.from_dict(data)

    async def replay_session(self, session_id: str) -> list[VeynEvent]:
        async with self._get_session().get(
            self._url(f"/v1/session/{session_id}/replay")
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return [VeynEvent.from_dict(item) for item in data]

    # ── Baseline ───────────────────────────────────────────────────────────────

    async def get_baseline(self, device_id: str, metric: str) -> BaselineStats:
        async with self._get_session().get(
            self._url(f"/v1/baseline/{device_id}/{metric}")
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return BaselineStats.from_dict(data)

    # ── Memory layer ──────────────────────────────────────────────────────────

    async def write_memory(self, topic: str, summary: str) -> MemoryRecord:
        """Write a semantic memory record; the daemon attaches current biometric state."""
        async with self._get_session().post(
            self._url("/v1/memory"), json={"topic": topic, "summary": summary}
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return MemoryRecord.from_dict(data)

    async def get_memory(self, query: MemoryQuery | None = None) -> list[MemoryRecord]:
        """Query memory records with optional filters."""
        params: dict[str, str | int] = {}
        if query:
            if query.topic is not None:
                params["topic"] = query.topic
            if query.since is not None:
                params["since"] = query.since
            if query.until is not None:
                params["until"] = query.until
            if query.kind is not None:
                params["kind"] = query.kind
            if query.limit is not None:
                params["limit"] = query.limit
        async with self._get_session().get(
            self._url("/v1/memory"), params=params
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return [MemoryRecord.from_dict(r) for r in data.get("records", [])]

    async def anchor_outcome(
        self,
        record_id: str,
        outcome_rating: OutcomeRating,
        notes: str | None = None,
    ) -> dict[str, object]:
        """Attach an outcome rating to a past memory record."""
        body: dict[str, str] = {"outcome_rating": outcome_rating}
        if notes is not None:
            body["notes"] = notes
        async with self._get_session().patch(
            self._url(f"/v1/memory/{record_id}/outcome"), json=body
        ) as resp:
            resp.raise_for_status()
            return await resp.json()  # type: ignore[no-any-return]

    # ── Pattern detection ──────────────────────────────────────────────────────

    async def get_patterns(self, min_samples: int | None = None) -> list[PatternRecord]:
        """Return topic-level physiological patterns from veyn-insight."""
        params: dict[str, int] = {}
        if min_samples is not None:
            params["min_samples"] = min_samples
        async with self._get_session().get(
            self._url("/v1/patterns"), params=params
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return [PatternRecord.from_dict(p) for p in data.get("patterns", [])]

    # ── Temporal patterns ──────────────────────────────────────────────────────

    async def get_temporal_patterns(self) -> list[TemporalSignal]:
        """Return trend signals for all metrics with sufficient data in the
        sliding 20-minute window."""
        async with self._get_session().get(
            self._url("/v1/temporal/patterns")
        ) as resp:
            resp.raise_for_status()
            data = await resp.json()
        return [TemporalSignal.from_dict(tp) for tp in data.get("patterns", [])]

    # ── WebSocket stream ───────────────────────────────────────────────────────

    async def ws_subscribe(
        self,
        callback: Callable[[VeynEvent], Any],
        device_class: list[str] | None = None,
        metrics: list[str] | None = None,
    ) -> None:
        """Stream raw VeynEvents over WebSocket, calling *callback* for each.

        Runs until the current task is cancelled.  Auto-reconnects with a 1 s
        backoff on disconnection or error.
        """
        params: dict[str, str] = {}
        if device_class:
            params["device_class"] = ",".join(device_class)
        if metrics:
            params["metrics"] = ",".join(metrics)

        qs = ("?" + "&".join(f"{k}={v}" for k, v in params.items())) if params else ""
        uri = self._ws_url(f"/v1/stream{qs}")
        extra_headers = {"Authorization": f"Bearer {self._token}"}

        while True:
            try:
                async with websockets.connect(  # type: ignore[attr-defined]
                    uri, additional_headers=extra_headers
                ) as ws:
                    async for message in ws:
                        try:
                            ev = VeynEvent.from_dict(json.loads(message))
                            result = callback(ev)
                            if asyncio.iscoroutine(result):
                                await result
                        except (json.JSONDecodeError, KeyError):
                            pass
            except asyncio.CancelledError:
                raise
            except (
                websockets.exceptions.ConnectionClosed,
                OSError,
            ):
                await asyncio.sleep(1.0)
            except Exception:
                await asyncio.sleep(1.0)

    async def ws_subscribe_context(
        self,
        callback: Callable[[ContextSnapshot], Any],
        intents: list[str] | None = None,
        min_confidence: float | None = None,
        source_class: list[str] | None = None,
        exclude_neutral: bool = False,
    ) -> None:
        """Stream ContextSnapshot objects over WebSocket (Semantic tier).

        Sends a runtime filter to the server immediately after connecting so
        only the requested intent codes / confidence levels are forwarded.
        Runs until the current task is cancelled.  Auto-reconnects with a 1 s
        backoff on disconnection or error.
        """
        uri = self._ws_url("/v1/stream")
        extra_headers = {"Authorization": f"Bearer {self._token}"}

        context_filter: dict[str, Any] = {"exclude_neutral": exclude_neutral}
        if intents:
            context_filter["intents"] = intents
        if min_confidence is not None:
            context_filter["min_confidence"] = min_confidence
        if source_class:
            context_filter["source_class"] = source_class

        subscribe_msg = json.dumps({"type": "subscribe", "context_filter": context_filter})

        while True:
            try:
                async with websockets.connect(  # type: ignore[attr-defined]
                    uri, additional_headers=extra_headers
                ) as ws:
                    await ws.send(subscribe_msg)
                    async for message in ws:
                        try:
                            snap = ContextSnapshot.from_dict(json.loads(message))
                            result = callback(snap)
                            if asyncio.iscoroutine(result):
                                await result
                        except (json.JSONDecodeError, KeyError):
                            pass
            except asyncio.CancelledError:
                raise
            except (
                websockets.exceptions.ConnectionClosed,
                OSError,
            ):
                await asyncio.sleep(1.0)
            except Exception:
                await asyncio.sleep(1.0)

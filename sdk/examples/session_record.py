"""session_record.py — Start a named session, wait 5 s, stop it, print replay events.

Usage::

    python session_record.py [--url http://localhost:8888] [--token <api_token>]
                             [--label "my session"] [--annotation "optional note"]
"""

from __future__ import annotations

import argparse
import asyncio

from veyn import VeynClient, Session, VeynEvent


# ── Formatting helpers ────────────────────────────────────────────────────────

def print_session(session: Session, heading: str) -> None:
    print(f"\n{heading}")
    print(f"  id              : {session.id}")
    print(f"  label           : {session.label}")
    print(f"  started_at      : {session.started_at} ms")
    if session.ended_at is not None:
        duration_s = (session.ended_at - session.started_at) / 1000.0
        print(f"  ended_at        : {session.ended_at} ms  ({duration_s:.1f} s)")
    else:
        print("  ended_at        : (still running)")
    print(f"  active_devices  : {session.active_device_ids}")
    if session.notes:
        print(f"  notes           : {session.notes}")


def print_events(events: list[VeynEvent]) -> None:
    if not events:
        print("\nNo events captured in this session.")
        return
    print(f"\nReplay — {len(events)} event(s):")
    for ev in events:
        print(
            f"  [{ev.ts}]  {ev.device_id}/{ev.metric}"
            f" = {ev.value} {ev.unit}"
            f"  (source={ev.source})"
        )
        if ev.meta:
            for k, v in ev.meta.items():
                print(f"             meta.{k} = {v!r}")


# ── Main ──────────────────────────────────────────────────────────────────────

async def main(base_url: str, token: str, label: str, annotation: str | None) -> None:
    print(f"Connecting to VEYN at {base_url} …")

    async with VeynClient(base_url, token) as client:
        health = await client.get_health()
        print(f"Daemon health: {health}")

        # Start session
        print(f"\nStarting session '{label}' …")
        session = await client.start_session(label, annotation)
        print_session(session, "Session started:")

        # Record for 5 seconds
        print("\nRecording for 5 seconds …")
        await asyncio.sleep(5)

        # Stop session
        print("Stopping session …")
        stopped = await client.stop_session()
        print_session(stopped, "Session stopped:")

        # Fetch and print replay
        print("\nFetching replay …")
        events = await client.replay_session(stopped.id)
        print_events(events)

    print("\nDone.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="VEYN session recorder example")
    parser.add_argument(
        "--url", default="http://localhost:8888", help="VEYN daemon base URL"
    )
    parser.add_argument("--token", default="dev-token", help="API bearer token")
    parser.add_argument(
        "--label", default="example-session", help="Session label"
    )
    parser.add_argument(
        "--annotation", default=None, help="Optional session annotation"
    )
    args = parser.parse_args()

    asyncio.run(main(args.url, args.token, args.label, args.annotation))

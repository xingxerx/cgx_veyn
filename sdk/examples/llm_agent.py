"""llm_agent.py — Subscribe to VEYN context snapshots and print stress recommendations.

This example demonstrates how an LLM agent integration can react to physiological
intent codes produced by the VEYN daemon.  When the daemon detects a StressResponse
intent the agent prints a grounding recommendation for the user.

Usage::

    python llm_agent.py [--url http://localhost:8888] [--token <api_token>]
"""

from __future__ import annotations

import argparse
import asyncio

from veyn import VeynClient, ContextSnapshot, INTENT_STRESS_RESPONSE


# ── Recommendation logic ──────────────────────────────────────────────────────

RECOMMENDATIONS: list[str] = [
    "Take three slow, deep breaths — inhale for 4 s, hold for 4 s, exhale for 6 s.",
    "Step away from the screen for two minutes and focus on something 20 feet away.",
    "Do a quick body scan: release tension in your shoulders, jaw, and hands.",
    "Drink a glass of water and notice the sensation — a micro mindfulness reset.",
    "Write down the single most important task remaining today and ignore the rest.",
]

_recommendation_index = 0


def next_recommendation() -> str:
    global _recommendation_index
    rec = RECOMMENDATIONS[_recommendation_index % len(RECOMMENDATIONS)]
    _recommendation_index += 1
    return rec


# ── Callback ──────────────────────────────────────────────────────────────────

def on_snapshot(snap: ContextSnapshot) -> None:
    intent = snap.intent_code
    confidence = snap.confidence

    print(
        f"[veyn] intent={intent!r}  confidence={confidence:.2f}"
        f"  devices={snap.active_devices}"
    )

    if intent == INTENT_STRESS_RESPONSE:
        print()
        print("  *** Stress response detected ***")
        print(f"  Recommendation: {next_recommendation()}")
        if snap.baseline_delta:
            elevated = [
                f"{metric}={z:+.2f}σ"
                for metric, z in snap.baseline_delta.items()
                if z > 1.0
            ]
            if elevated:
                print(f"  Elevated metrics: {', '.join(elevated)}")
        print()


# ── Entry point ───────────────────────────────────────────────────────────────

async def main(base_url: str, token: str) -> None:
    print(f"Connecting to VEYN at {base_url} …")

    client = VeynClient(base_url, token)

    health = await client.get_health()
    print(f"Daemon health: {health}")
    print("Listening for context snapshots (Ctrl-C to stop) …\n")

    # subscribe() runs until cancelled
    await client.subscribe(
        on_snapshot,
        intents=[INTENT_STRESS_RESPONSE],  # server-side filter hint
        min_confidence=0.5,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="VEYN LLM agent example")
    parser.add_argument(
        "--url", default="http://localhost:8888", help="VEYN daemon base URL"
    )
    parser.add_argument("--token", default="dev-token", help="API bearer token")
    args = parser.parse_args()

    try:
        asyncio.run(main(args.url, args.token))
    except KeyboardInterrupt:
        print("\nStopped.")

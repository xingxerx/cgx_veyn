# Ollama (local LLM) Integration

Run a fully local AI agent that reads your physiological context from VEYN and
a local Ollama model — no cloud required.

## Prerequisites

- VEYN daemon running (`cargo run -p veyn-core -- --mock`)
- [Ollama](https://ollama.ai) installed (`ollama pull llama3`)
- Python 3.10+ with `pip install veyn-sdk ollama`

## Python agent example

```python
import asyncio
import json
import ollama
from veyn import VeynClient

VEYN_HOST  = "http://localhost:7700"
VEYN_TOKEN = open("/home/.local/share/veyn/token").read().strip()
MODEL      = "llama3"

SYSTEM_PROMPT = """You are a decision-support assistant.
You receive a physiological context snapshot from the user's wearable sensors.
Use the intent_code and baseline_delta fields to give grounded, empathetic advice.
If intent_confidence < 0.5, acknowledge the uncertainty.
"""

async def main():
    async with VeynClient(VEYN_HOST, token=VEYN_TOKEN) as client:
        ctx = await client.get_context()
        user_message = (
            f"Current state:\n"
            f"  intent_code:       {ctx.intent_code}\n"
            f"  intent_confidence: {ctx.intent_confidence:.2f}\n"
            f"  baseline_delta:    {json.dumps(ctx.baseline_delta or {}, indent=4)}\n\n"
            "I'm weighing whether to accept this job offer today. What do you think?"
        )

        response = ollama.chat(
            model=MODEL,
            messages=[
                {"role": "system",  "content": SYSTEM_PROMPT},
                {"role": "user",    "content": user_message},
            ],
        )
        print(response["message"]["content"])

asyncio.run(main())
```

## Streaming context to Ollama

For a continuous conversation that updates as your state changes:

```python
async def stream_agent():
    async with VeynClient(VEYN_HOST, token=VEYN_TOKEN) as client:
        async for snapshot in client.subscribe(min_confidence=0.6):
            prompt = f"State changed to {snapshot.intent_code}. Anything I should know?"
            for chunk in ollama.chat(model=MODEL, messages=[
                {"role": "system",  "content": SYSTEM_PROMPT},
                {"role": "user",    "content": prompt},
            ], stream=True):
                print(chunk["message"]["content"], end="", flush=True)
            print()
```

## Recommended models

| Use case              | Model                    | Notes                             |
|-----------------------|--------------------------|-----------------------------------|
| General decision help | `llama3`                 | Good balance of speed and quality |
| Concise status alerts | `phi3:mini`              | Fast, low memory                  |
| Medical framing       | `medllama2` (community)  | Healthcare-aware vocabulary       |

## Context tier for privacy

Set `VEYN_CONTEXT_TIER=semantic` (or in `veyn.toml`) to ensure Ollama only
receives intent codes and z-scores — never raw keystrokes or HID reports.

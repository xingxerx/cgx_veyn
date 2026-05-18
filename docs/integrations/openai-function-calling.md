# OpenAI Function Calling Integration

Expose VEYN's context to a GPT-4 / GPT-4o agent using OpenAI's function-calling
(tool use) API so the model can query physiological state on demand.

## Prerequisites

- VEYN daemon running (`cargo run -p veyn-core -- --mock`)
- Python 3.10+: `pip install veyn-sdk openai`

## Tool definitions

```python
VEYN_TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "veyn_get_context",
            "description": (
                "Returns the current physiological context snapshot from the user's "
                "wearable sensors. Includes intent_code (e.g. 'stress_response', "
                "'cognitive_load'), intent_confidence (0–1), and per-metric z-scores "
                "relative to the user's personal 30-day baseline."
            ),
            "parameters": {"type": "object", "properties": {}, "required": []},
        },
    },
    {
        "type": "function",
        "function": {
            "name": "veyn_get_baseline",
            "description": (
                "Returns rolling-window baseline statistics (mean, stddev, p10, p90) "
                "for a specific device and metric. Use this to give the user concrete "
                "numbers (e.g. 'your resting HR is typically 62 bpm')."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "device_id": {"type": "string", "description": "Device identifier"},
                    "metric":    {"type": "string", "description": "Metric name, e.g. 'heart_rate'"},
                },
                "required": ["device_id", "metric"],
            },
        },
    },
]
```

## Agent loop

```python
import asyncio, json, openai
from veyn import VeynClient

VEYN_HOST  = "http://localhost:7700"
VEYN_TOKEN = open("/home/.local/share/veyn/token").read().strip()

async def dispatch_tool(client: VeynClient, name: str, args: dict) -> str:
    if name == "veyn_get_context":
        ctx = await client.get_context()
        return json.dumps({
            "intent_code":       ctx.intent_code,
            "intent_confidence": ctx.intent_confidence,
            "baseline_delta":    ctx.baseline_delta or {},
        })
    if name == "veyn_get_baseline":
        stats = await client.get_baseline(args["device_id"], args["metric"])
        return json.dumps(stats.__dict__ if stats else {"error": "insufficient data"})
    return json.dumps({"error": f"unknown tool {name}"})

async def run_agent(user_query: str):
    oai = openai.AsyncOpenAI()
    async with VeynClient(VEYN_HOST, token=VEYN_TOKEN) as client:
        messages = [
            {
                "role": "system",
                "content": (
                    "You are a physiological decision-support assistant. "
                    "When the user asks about decisions or how they are feeling, "
                    "call veyn_get_context first to read their current state, then "
                    "give grounded, empathetic advice."
                ),
            },
            {"role": "user", "content": user_query},
        ]

        while True:
            resp = await oai.chat.completions.create(
                model="gpt-4o",
                messages=messages,
                tools=VEYN_TOOLS,
                tool_choice="auto",
            )
            msg = resp.choices[0].message
            messages.append(msg)

            if not msg.tool_calls:
                print(msg.content)
                break

            for call in msg.tool_calls:
                result = await dispatch_tool(
                    client,
                    call.function.name,
                    json.loads(call.function.arguments or "{}"),
                )
                messages.append({
                    "role":         "tool",
                    "tool_call_id": call.id,
                    "content":      result,
                })

asyncio.run(run_agent("Should I make this big decision right now?"))
```

## TypeScript / Node.js variant

```typescript
import OpenAI from "openai";
import { VeynClient } from "veyn-sdk";

const client = new VeynClient("http://localhost:7700", { token: process.env.VEYN_TOKEN! });
const oai    = new OpenAI();

const ctx     = await client.getContext();
const message = `Current physiological state: ${JSON.stringify(ctx, null, 2)}\n\nUser: Should I send that email now?`;

const resp = await oai.chat.completions.create({
  model:    "gpt-4o",
  messages: [{ role: "user", content: message }],
});

console.log(resp.choices[0].message.content);
await client.close();
```

## Privacy note

Use `tier:semantic` token scope so the function-calling layer only receives
`ContextSnapshot` objects — never raw HID input. This ensures OpenAI's API
never receives keystroke-level data.

```bash
# Generate a semantic-tier token
cat >> ~/.local/share/veyn/tokens.json << 'EOF'
[{"token": "gpt-agent-token", "label": "openai-agent", "scopes": ["read", "tier:semantic"]}]
EOF
```

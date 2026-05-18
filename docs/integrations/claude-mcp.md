# Claude via veyn-mcp

VEYN ships `veyn-mcp`, a Model Context Protocol server that exposes the daemon's
context stream to Claude and other MCP-compatible AI agents.

## What it provides

| MCP Tool              | Description                                                  |
|-----------------------|--------------------------------------------------------------|
| `veyn_get_context`    | Fetch the current `ContextSnapshot` (intent, z-scores, …)   |
| `veyn_get_history`    | Last N context snapshots from the ring buffer                |
| `veyn_start_session`  | Open a named recording session                               |
| `veyn_stop_session`   | Close the active session and return its metadata             |
| `veyn_get_session`    | Retrieve a session by ID (metadata + event timeline)         |
| `veyn_list_devices`   | List connected adapters and their state                      |
| `veyn_get_baseline`   | Fetch rolling-window `BaselineStats` for a metric            |

## Quick start

### 1. Run the daemon

```bash
cargo run --release -p veyn-core -- --mock
```

The first run generates `~/.local/share/veyn/token`. Note the token.

### 2. Run the MCP server

```bash
VEYN_TOKEN=<token> cargo run --release -p veyn-mcp
```

By default the MCP server connects to `http://localhost:7700`.

### 3. Add to Claude Desktop (`~/.config/claude/claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "veyn": {
      "command": "/path/to/veyn-mcp",
      "env": {
        "VEYN_TOKEN": "<your-token>",
        "VEYN_HOST": "http://localhost:7700"
      }
    }
  }
}
```

Restart Claude Desktop. VEYN tools will appear in the toolbox.

## Example agent prompt

```
You have access to the user's physiological context via the veyn_* tools.

Before answering any decision or reasoning task:
1. Call veyn_get_context to read the current intent_code and baseline_delta.
2. If intent_confidence < 0.5 call veyn_get_history to see the trend.
3. Include a one-sentence physiological note in your response when the data
   is informative (e.g. "Your HRV z-score is −1.4, indicating elevated stress —
   you may want to revisit this decision when calmer.").
```

## Context tier

Tokens can be minted with a `tier:semantic` scope to restrict the MCP server
to `ContextSnapshot` output only (no raw HID events):

```json
[
  {
    "token": "<generated-token>",
    "label": "claude-agent",
    "scopes": ["read", "tier:semantic"]
  }
]
```

Save to `~/.local/share/veyn/tokens.json`.

## intent_code reference

| Code             | Physiological signature                             |
|------------------|-----------------------------------------------------|
| `neutral`        | All metrics within baseline                         |
| `stress_response`| HR z > 1.5 AND HRV z < −1.0                        |
| `cognitive_load` | EEG beta z > 1.0 AND alpha z < −0.5                |
| `approach`       | HR z 0.5–1.5 AND HRV z > 0.0 (positive engagement) |
| `avoidance`      | HR z > 0.5 AND HRV z < −0.5 AND skin_temp z > 0.5  |
| `fatigue`        | HR z < −0.5 AND EEG theta z > 1.0                  |
| `recovery`       | HRV z > 1.0 AND HR z < 0.0                         |

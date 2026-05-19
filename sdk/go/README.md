# VEYN Go SDK

Idiomatic Go client for the VEYN physiological context daemon.

## Installation

```bash
go get github.com/veyn-io/veyn/sdk/go
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "log"
    "os"
    "strings"
    "time"

    "veyn"
)

func main() {
    // Read bearer token from ~/.local/share/veyn/token
    tokenBytes, err := os.ReadFile(os.Getenv("HOME") + "/.local/share/veyn/token")
    if err != nil {
        log.Fatal(err)
    }
    token := strings.TrimSpace(string(tokenBytes))

    // Create client
    client := veyn.NewClient("http://localhost:7357", token)

    // Health check
    ctx := context.Background()
    health, err := client.Health(ctx)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("VEYN daemon %s is running (uptime: %ds)\n", health.Version, health.UptimeSec)

    // Get current context snapshot
    snapshot, err := client.CurrentContext(ctx)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Current intent: %s (confidence: %.2f)\n", 
        snapshot.IntentCode, snapshot.IntentConfidence)

    // Print baseline deltas if available
    if snapshot.BaselineDelta != nil {
        fmt.Println("\nBaseline z-scores:")
        for metric, zscore := range snapshot.BaselineDelta {
            fmt.Printf("  %s: %.2fσ\n", metric, zscore)
        }
    }
}
```

## Streaming Updates (SSE)

```go
// Subscribe to real-time context updates
ch := make(chan veyn.ContextSnapshot)
ctx, cancel := context.WithCancel(context.Background())
defer cancel()

go func() {
    for snapshot := range ch {
        fmt.Printf("[%d] Intent: %s → %s (%.2f)\n",
            snapshot.TimestampMs,
            snapshot.Intent,
            snapshot.IntentCode,
            snapshot.IntentConfidence)
    }
}()

// Optional: filter by device IDs or minimum confidence
filter := &veyn.SubscribeFilter{
    MinConfidence: 0.5,
    IntentCodes:   []veyn.IntentCode{veyn.IntentApproach, veyn.IntentAvoidance},
}

err := client.SubscribeSSE(ctx, ch, filter)
if err != nil {
    log.Fatal(err)
}
```

## Sessions

```go
// Start a recording session
session, err := client.StartSession(ctx, &veyn.StartSessionOptions{
    Label: "Product review decision",
    Notes: "Evaluating feature tradeoffs",
})
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Session started: %s\n", session.ID)

// ... do work while session records ...

// Stop session
session, err = client.StopSession(ctx, session.ID)
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Session ended after %d ms\n", *session.EndedAt - session.StartedAt)
```

## Memory

```go
// Write a memory record
record := &veyn.MemoryRecord{
    Topic:   "Q4 planning",
    Summary: "Felt calm and focused during strategic discussion",
    Kind:    veyn.MemoryKindSemantic,
}
written, err := client.WriteMemory(ctx, record)
if err != nil {
    log.Fatal(err)
}

// Query memories by topic
query := &veyn.MemoryQuery{
    Topic: "planning",
    Limit: ptr(10),
}
memories, err := client.ReadMemory(ctx, query)
if err != nil {
    log.Fatal(err)
}
for _, m := range memories {
    fmt.Printf("%s: %s\n", time.UnixMilli(m.TimestampMs), m.Summary)
}
```

## API Reference

### Client

- `NewClient(baseURL, token string) *Client` - Create new client
- `Health(ctx) (*HealthCheckResponse, error)` - Daemon health status
- `CurrentContext(ctx) (*ContextSnapshot, error)` - Latest semantic snapshot
- `ContextHistory(ctx, opts) ([]ContextSnapshot, error)` - Last N snapshots
- `SubscribeSSE(ctx, ch, filter) error` - Stream snapshots via SSE
- `StartSession(ctx, opts) (*Session, error)` - Begin recording session
- `StopSession(ctx, id) (*Session, error)` - End recording session
- `GetSession(ctx, id) (*Session, error)` - Fetch session details
- `GetBaseline(ctx, deviceID, metric) (*BaselineStats, error)` - Baseline stats
- `WriteMemory(ctx, record) (*MemoryRecord, error)` - Store memory
- `ReadMemory(ctx, query) ([]MemoryRecord, error)` - Query memories

### Types

- `ContextSnapshot` - Semantic context with intent, confidence, baseline deltas
- `IntentCode` - Machine-readable intent classification
- `StateDelta` - Single metric observation
- `TemporalSignal` - Trend analysis for a metric
- `Session` - Named recording session
- `MemoryRecord` - Persistent biometric memory entry
- `BaselineStats` - Rolling-window statistics

## License

MIT

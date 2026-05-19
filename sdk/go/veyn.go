// Package veyn provides a Go client for the VEYN physiological context daemon.
//
// VEYN exposes biometric data and semantic context snapshots via HTTP REST,
// Server-Sent Events (SSE), and WebSocket APIs. This package provides idiomatic
// Go bindings for interacting with these interfaces.
//
// Basic usage:
//
//	client := veyn.NewClient("http://localhost:7357", "your-bearer-token")
//	snapshot, err := client.CurrentContext(ctx)
//	if err != nil {
//	    log.Fatal(err)
//	}
//	fmt.Printf("Intent: %s (confidence: %.2f)\n", snapshot.IntentCode, snapshot.IntentConfidence)
//
// For streaming updates, use SubscribeSSE or SubscribeWebSocket:
//
//	ch := make(chan veyn.ContextSnapshot)
//	go func() {
//	    for snapshot := range ch {
//	        fmt.Printf("Update: %+v\n", snapshot)
//	    }
//	}()
//	err := client.SubscribeSSE(ctx, ch, nil)
package veyn

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Client is a VEYN daemon API client.
type Client struct {
	baseURL    string
	httpClient *http.Client
	token      string
}

// NewClient creates a new VEYN API client.
// baseURL is typically "http://localhost:7357".
// token is the Bearer token from ~/.local/share/veyn/token.
func NewClient(baseURL, token string) *Client {
	return &Client{
		baseURL: strings.TrimSuffix(baseURL, "/"),
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
		token: token,
	}
}

// SetTimeout sets the HTTP client timeout.
func (c *Client) SetTimeout(timeout time.Duration) {
	c.httpClient.Timeout = timeout
}

// doRequest performs an HTTP request with authentication.
func (c *Client) doRequest(req *http.Request) (*http.Response, error) {
	req.Header.Set("Authorization", "Bearer "+c.token)
	req.Header.Set("Accept", "application/json")
	return c.httpClient.Do(req)
}

// HealthCheckResponse is the response from GET /v1/health.
type HealthCheckResponse struct {
	Status    string `json:"status"`
	Version   string `json:"version"`
	SessionID string `json:"session_id"`
	UptimeSec uint64 `json:"uptime_sec"`
}

// Health checks the daemon health status.
func (c *Client) Health(ctx context.Context) (*HealthCheckResponse, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", c.baseURL+"/v1/health", nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("health check failed: %s", resp.Status)
	}

	var health HealthCheckResponse
	if err := json.NewDecoder(resp.Body).Decode(&health); err != nil {
		return nil, err
	}

	return &health, nil
}

// IntentCode represents a machine-readable intent classification.
type IntentCode string

const (
	IntentNeutral         IntentCode = "neutral"
	IntentCognitiveLoad   IntentCode = "cognitive_load"
	IntentStressResponse  IntentCode = "stress_response"
	IntentApproach        IntentCode = "approach"
	IntentAvoidance       IntentCode = "avoidance"
	IntentFatigue         IntentCode = "fatigue"
	IntentRecovery        IntentCode = "recovery"
)

// ContextSnapshot is a semantic context snapshot from the compression engine.
type ContextSnapshot struct {
	TimestampMs       int64             `json:"timestamp_ms"`
	SessionID         string            `json:"session_id"`
	Intent            string            `json:"intent"`
	IntentCode        IntentCode        `json:"intent_code"`
	Confidence        float64           `json:"confidence"`
	IntentConfidence  float32           `json:"intent_confidence"`
	ActiveDevices     []string          `json:"active_devices"`
	StateDeltas       []StateDelta      `json:"state_deltas"`
	BaselineDelta     map[string]float64 `json:"baseline_delta,omitempty"`
	RecordingSessionID *string          `json:"recording_session_id,omitempty"`
	TemporalPatterns  []TemporalSignal  `json:"temporal_patterns,omitempty"`
}

// StateDelta is one metric observation in a context snapshot.
type StateDelta struct {
	DeviceID     string  `json:"device_id"`
	Metric       string  `json:"metric"`
	Value        float64 `json:"value"`
	Unit         string  `json:"unit"`
	Ts           int64   `json:"ts"`
	SourceClass  string  `json:"source_class"`
}

// TemporalSignal is trend analysis for a single metric.
type TemporalSignal struct {
	Metric      string         `json:"metric"`
	Trend       TemporalTrend  `json:"trend"`
	SlopePerMin float64        `json:"slope_per_min"`
	WindowSecs  uint32         `json:"window_secs"`
	Confidence  float32        `json:"confidence"`
	Samples     int            `json:"samples"`
}

// TemporalTrend represents the direction of change.
type TemporalTrend string

const (
	TrendStable     TemporalTrend = "stable"
	TrendRising     TemporalTrend = "rising"
	TrendFalling    TemporalTrend = "falling"
	TrendSpiking    TemporalTrend = "spiking"
	TrendDeclining  TemporalTrend = "declining"
	TrendRecovering TemporalTrend = "recovering"
)

// CurrentContext fetches the current semantic context snapshot.
func (c *Client) CurrentContext(ctx context.Context) (*ContextSnapshot, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", c.baseURL+"/v1/context/current", nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("context fetch failed: %s", resp.Status)
	}

	var snapshot ContextSnapshot
	if err := json.NewDecoder(resp.Body).Decode(&snapshot); err != nil {
		return nil, err
	}

	return &snapshot, nil
}

// ContextHistoryOptions are options for fetching context history.
type ContextHistoryOptions struct {
	N int // Number of snapshots to fetch (default: 10, max: 32)
}

// ContextHistory fetches the last N context snapshots.
func (c *Client) ContextHistory(ctx context.Context, opts *ContextHistoryOptions) ([]ContextSnapshot, error) {
	u, _ := url.Parse(c.baseURL + "/v1/context/history")
	q := u.Query()
	if opts != nil && opts.N > 0 {
		q.Set("n", fmt.Sprintf("%d", opts.N))
	}
	u.RawQuery = q.Encode()

	req, err := http.NewRequestWithContext(ctx, "GET", u.String(), nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("history fetch failed: %s", resp.Status)
	}

	var snapshots []ContextSnapshot
	if err := json.NewDecoder(resp.Body).Decode(&snapshots); err != nil {
		return nil, err
	}

	return snapshots, nil
}

// SubscribeFilter is a declarative filter for SSE/WebSocket subscriptions.
type SubscribeFilter struct {
	DeviceIDs   []string `json:"device_ids,omitempty"`
	Metrics     []string `json:"metrics,omitempty"`
	MinConfidence float32 `json:"min_confidence,omitempty"`
	IntentCodes []IntentCode `json:"intent_codes,omitempty"`
}

// SubscribeSSE subscribes to context snapshots via Server-Sent Events.
// Snapshots are sent on the provided channel. The function blocks until
// the context is cancelled or an error occurs.
func (c *Client) SubscribeSSE(ctx context.Context, ch chan<- ContextSnapshot, filter *SubscribeFilter) error {
	u, _ := url.Parse(c.baseURL + "/v1/context/subscribe")
	
	if filter != nil {
		q := u.Query()
		if len(filter.DeviceIDs) > 0 {
			q.Set("device_ids", strings.Join(filter.DeviceIDs, ","))
		}
		if len(filter.Metrics) > 0 {
			q.Set("metrics", strings.Join(filter.Metrics, ","))
		}
		if filter.MinConfidence > 0 {
			q.Set("min_confidence", fmt.Sprintf("%f", filter.MinConfidence))
		}
		if len(filter.IntentCodes) > 0 {
			codes := make([]string, len(filter.IntentCodes))
			for i, code := range filter.IntentCodes {
				codes[i] = string(code)
			}
			q.Set("intent_codes", strings.Join(codes, ","))
		}
		u.RawQuery = q.Encode()
	}

	req, err := http.NewRequestWithContext(ctx, "GET", u.String(), nil)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+c.token)
	req.Header.Set("Accept", "text/event-stream")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("SSE subscription failed: %s", resp.Status)
	}

	reader := bufio.NewReader(resp.Body)
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		line, err := reader.ReadString('\n')
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}

		if !strings.HasPrefix(line, "data: ") {
			continue
		}

		data := strings.TrimPrefix(line, "data: ")
		data = strings.TrimSpace(data)

		var snapshot ContextSnapshot
		if err := json.Unmarshal([]byte(data), &snapshot); err != nil {
			continue
		}

		select {
		case ch <- snapshot:
		case <-ctx.Done():
			return ctx.Err()
		}
	}
}

// VeynEvent is a raw biometric event from an adapter.
type VeynEvent struct {
	ID        string                 `json:"id"`
	Ts        int64                  `json:"ts"`
	DeviceID  string                 `json:"device_id"`
	Source    string                 `json:"source"`
	Metric    string                 `json:"metric"`
	Value     float64                `json:"value"`
	Unit      string                 `json:"unit"`
	Meta      map[string]interface{} `json:"meta"`
}

// Session represents a named recording session.
type Session struct {
	ID              string   `json:"id"`
	Label           string   `json:"label"`
	StartedAt       int64    `json:"started_at"`
	EndedAt         *int64   `json:"ended_at,omitempty"`
	ActiveDeviceIDs []string `json:"active_device_ids"`
	Notes           *string  `json:"notes,omitempty"`
}

// StartSessionOptions are options for starting a session.
type StartSessionOptions struct {
	Label   string
	Notes   string
}

// StartSession starts a new recording session.
func (c *Client) StartSession(ctx context.Context, opts *StartSessionOptions) (*Session, error) {
	body := struct {
		Label string `json:"label"`
		Notes string `json:"notes,omitempty"`
	}{
		Label: opts.Label,
		Notes: opts.Notes,
	}

	payload, err := json.Marshal(body)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", c.baseURL+"/v1/session/start", 
		strings.NewReader(string(payload)))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("start session failed: %s", resp.Status)
	}

	var session Session
	if err := json.NewDecoder(resp.Body).Decode(&session); err != nil {
		return nil, err
	}

	return &session, nil
}

// StopSession stops an active recording session.
func (c *Client) StopSession(ctx context.Context, sessionID string) (*Session, error) {
	req, err := http.NewRequestWithContext(ctx, "POST", 
		fmt.Sprintf("%s/v1/session/%s/stop", c.baseURL, sessionID), nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("stop session failed: %s", resp.Status)
	}

	var session Session
	if err := json.NewDecoder(resp.Body).Decode(&session); err != nil {
		return nil, err
	}

	return &session, nil
}

// GetSession fetches a session by ID.
func (c *Client) GetSession(ctx context.Context, sessionID string) (*Session, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", 
		fmt.Sprintf("%s/v1/session/%s", c.baseURL, sessionID), nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("get session failed: %s", resp.Status)
	}

	var session Session
	if err := json.NewDecoder(resp.Body).Decode(&session); err != nil {
		return nil, err
	}

	return &session, nil
}

// BaselineStats represents rolling-window baseline statistics.
type BaselineStats struct {
	DeviceID    string  `json:"device_id"`
	Metric      string  `json:"metric"`
	Mean        float64 `json:"mean"`
	Stddev      float64 `json:"stddev"`
	P10         float64 `json:"p10"`
	P90         float64 `json:"p90"`
	SampleCount int     `json:"sample_count"`
	WindowDays  uint32  `json:"window_days"`
	UpdatedAt   int64   `json:"updated_at"`
}

// GetBaseline fetches baseline statistics for a device/metric pair.
func (c *Client) GetBaseline(ctx context.Context, deviceID, metric string) (*BaselineStats, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", 
		fmt.Sprintf("%s/v1/baseline/%s/%s", c.baseURL, deviceID, metric), nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("get baseline failed: %s", resp.Status)
	}

	var stats BaselineStats
	if err := json.NewDecoder(resp.Body).Decode(&stats); err != nil {
		return nil, err
	}

	return &stats, nil
}

// MemoryKind classifies a memory record's origin.
type MemoryKind string

const (
	MemoryKindAmbient  MemoryKind = "ambient"
	MemoryKindSemantic MemoryKind = "semantic"
)

// MemoryRecord is a persistent biometric memory entry.
type MemoryRecord struct {
	ID             string          `json:"id"`
	TimestampMs    int64           `json:"timestamp_ms"`
	SessionID      string          `json:"session_id"`
	Kind           MemoryKind      `json:"kind"`
	Topic          string          `json:"topic"`
	Summary        string          `json:"summary"`
	IntentAtTime   *string         `json:"intent_at_time,omitempty"`
	ConfidenceAtTime *float64      `json:"confidence_at_time,omitempty"`
	HrvAtTime      *float64        `json:"hrv_at_time,omitempty"`
	HrAtTime       *float64        `json:"hr_at_time,omitempty"`
	ContextSnapshot json.RawMessage `json:"context_snapshot,omitempty"`
	OutcomeRating  *string         `json:"outcome_rating,omitempty"`
	OutcomeNotes   *string         `json:"outcome_notes,omitempty"`
	OutcomeAtMs    *int64          `json:"outcome_at_ms,omitempty"`
}

// WriteMemory writes a memory record.
func (c *Client) WriteMemory(ctx context.Context, record *MemoryRecord) (*MemoryRecord, error) {
	payload, err := json.Marshal(record)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", c.baseURL+"/v1/memory",
		strings.NewReader(string(payload)))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("write memory failed: %s", resp.Status)
	}

	var result MemoryRecord
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}

	return &result, nil
}

// MemoryQuery filters memory records.
type MemoryQuery struct {
	Topic    string     `json:"topic,omitempty"`
	SinceMs  *int64     `json:"since_ms,omitempty"`
	UntilMs  *int64     `json:"until_ms,omitempty"`
	Kind     *MemoryKind `json:"kind,omitempty"`
	Limit    *int       `json:"limit,omitempty"`
}

// ReadMemory queries memory records.
func (c *Client) ReadMemory(ctx context.Context, query *MemoryQuery) ([]MemoryRecord, error) {
	u, _ := url.Parse(c.baseURL + "/v1/memory")
	
	if query != nil {
		q := u.Query()
		if query.Topic != "" {
			q.Set("topic", query.Topic)
		}
		if query.SinceMs != nil {
			q.Set("since_ms", fmt.Sprintf("%d", *query.SinceMs))
		}
		if query.UntilMs != nil {
			q.Set("until_ms", fmt.Sprintf("%d", *query.UntilMs))
		}
		if query.Kind != nil {
			q.Set("kind", string(*query.Kind))
		}
		if query.Limit != nil {
			q.Set("limit", fmt.Sprintf("%d", *query.Limit))
		}
		u.RawQuery = q.Encode()
	}

	req, err := http.NewRequestWithContext(ctx, "GET", u.String(), nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.doRequest(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("read memory failed: %s", resp.Status)
	}

	var records []MemoryRecord
	if err := json.NewDecoder(resp.Body).Decode(&records); err != nil {
		return nil, err
	}

	return records, nil
}

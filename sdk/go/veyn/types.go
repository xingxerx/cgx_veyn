package veyn

// VeynEvent represents a raw or filtered sensory event.
type VeynEvent struct {
	Timestamp  int64   `json:"ts"`
	DeviceID   string  `json:"device_id"`
	Source     string  `json:"source"`
	Metric     string  `json:"metric"`
	Value      float64 `json:"value"`
	Unit       string  `json:"unit"`
	SessionID  *string `json:"session_id,omitempty"`
}

// StateDelta represents a delta compression entry.
type StateDelta struct {
	DeviceID    string  `json:"device_id"`
	Metric      string  `json:"metric"`
	Value       float64 `json:"value"`
	Unit        string  `json:"unit"`
	Timestamp   int64   `json:"ts"`
	SourceClass string  `json:"source_class"`
}

// Pattern represents a temporal coherence pattern.
type Pattern struct {
	Metric    string  `json:"metric"`
	Frequency float64 `json:"frequency"`
	Phase     float64 `json:"phase"`
	Amplitude float64 `json:"amplitude"`
	UptimeSec int64   `json:"uptime_sec"`
}

// ContextSnapshot represents the synthesized ambient biometric context.
type ContextSnapshot struct {
	TimestampMS         int64                `json:"timestamp_ms"`
	SessionID           string               `json:"session_id"`
	Intent              string               `json:"intent"`
	IntentCode          string               `json:"intent_code"`
	Confidence          float64              `json:"confidence"`
	ActiveDevices       []string             `json:"active_devices"`
	StateDeltas         []StateDelta         `json:"state_deltas"`
	BaselineDelta       map[string]float64   `json:"baseline_delta,omitempty"`
	RecordingSessionID  *string              `json:"recording_session_id,omitempty"`
	TemporalPatterns    map[string][]Pattern `json:"temporal_patterns,omitempty"`
}

// ClientInfo represents metadata for an active subscriber to the context bus.
type ClientInfo struct {
	ClientID      string   `json:"client_id"`
	ConnectedAt   string   `json:"connected_at"`
	ConnectedAtMS int64    `json:"connected_at_ms"`
	Tier          string   `json:"tier"`
	Transport     string   `json:"transport"`
	SourceFilter  []string `json:"source_filter,omitempty"`
}

// MemoryRecord represents a persistent semantic context entry.
type MemoryRecord struct {
	ID                 string   `json:"id"`
	TimestampMS        int64    `json:"timestamp_ms"`
	Topic              string   `json:"topic"`
	Summary            string   `json:"summary"`
	IntentAtTime       string   `json:"intent_at_time"`
	HRAtTime           *float64 `json:"hr_at_time,omitempty"`
	HRVAtTime          *float64 `json:"hrv_at_time,omitempty"`
	ConfidenceAtTime   float64  `json:"confidence_at_time"`
	SessionID          *string  `json:"session_id,omitempty"`
	OutcomeRating      *int     `json:"outcome_rating,omitempty"`
	OutcomeNotes       *string  `json:"outcome_notes,omitempty"`
}

// Session represents a demarcated user session.
type Session struct {
	ID        string `json:"id"`
	Label     string `json:"label"`
	StartedAt string `json:"started_at"`
	EndedAt   *string `json:"ended_at,omitempty"`
}

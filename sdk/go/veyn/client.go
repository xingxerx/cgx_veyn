package veyn

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/gorilla/websocket"
)

// Client handles communication with the VEYN Daemon.
type Client struct {
	BaseURL    string
	Token      string
	HTTPClient *http.Client
}

// NewClient initializes a new VEYN client.
func NewClient(baseURL string, token string) *Client {
	return &Client{
		BaseURL:    strings.TrimSuffix(baseURL, "/"),
		Token:      token,
		HTTPClient: &http.Client{Timeout: 10 * time.Second},
	}
}

func (c *Client) newRequest(method, path string, body interface{}) (*http.Request, error) {
	var bodyReader *bytes.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		bodyReader = bytes.NewReader(data)
	}

	reqURL := fmt.Sprintf("%s%s", c.BaseURL, path)
	var req *http.Request
	var err error
	if bodyReader != nil {
		req, err = http.NewRequest(method, reqURL, bodyReader)
	} else {
		req, err = http.NewRequest(method, reqURL, nil)
	}
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	if c.Token != "" {
		req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", c.Token))
	}

	return req, nil
}

func (c *Client) do(req *http.Request, v interface{}) error {
	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("veyn api error: status %d", resp.StatusCode)
	}

	if v != nil {
		return json.NewDecoder(resp.Body).Decode(v)
	}
	return nil
}

// GetHealth queries the /v1/health status endpoint.
func (c *Client) GetHealth() (map[string]interface{}, error) {
	req, err := c.newRequest("GET", "/v1/health", nil)
	if err != nil {
		return nil, err
	}
	var res map[string]interface{}
	err = c.do(req, &res)
	return res, err
}

// StartSession begins a new named recording session.
func (c *Client) StartSession(label string) (*Session, error) {
	body := map[string]string{"label": label}
	req, err := c.newRequest("POST", "/v1/session/start", body)
	if err != nil {
		return nil, err
	}
	var res Session
	err = c.do(req, &res)
	return &res, err
}

// StopSession stops the active recording session.
func (c *Client) StopSession() (*Session, error) {
	req, err := c.newRequest("POST", "/v1/session/stop", nil)
	if err != nil {
		return nil, err
	}
	var res Session
	err = c.do(req, &res)
	return &res, err
}

// WriteMemory persists a new biometric memory record.
func (c *Client) WriteMemory(topic, summary string) (*MemoryRecord, error) {
	body := map[string]string{
		"topic":   topic,
		"summary": summary,
	}
	req, err := c.newRequest("POST", "/v1/memory", body)
	if err != nil {
		return nil, err
	}
	var res MemoryRecord
	err = c.do(req, &res)
	return &res, err
}

// GetMemory retrieves a specific memory record.
func (c *Client) GetMemory(id string) (*MemoryRecord, error) {
	req, err := c.newRequest("GET", fmt.Sprintf("/v1/memory/%s", id), nil)
	if err != nil {
		return nil, err
	}
	var res MemoryRecord
	err = c.do(req, &res)
	return &res, err
}

// DeleteMemory deletes a specific memory record.
func (c *Client) DeleteMemory(id string) error {
	req, err := c.newRequest("DELETE", fmt.Sprintf("/v1/memory/%s", id), nil)
	if err != nil {
		return err
	}
	return c.do(req, nil)
}

// GetClients lists currently active context bus subscribers.
func (c *Client) GetClients() ([]ClientInfo, error) {
	req, err := c.newRequest("GET", "/v1/clients", nil)
	if err != nil {
		return nil, err
	}
	var res []ClientInfo
	err = c.do(req, &res)
	return res, err
}

// SubscribeEvents establishes a WebSocket stream to receive raw/filtered events.
func (c *Client) SubscribeEvents(handler func(VeynEvent)) error {
	u, err := url.Parse(c.BaseURL)
	if err != nil {
		return err
	}
	scheme := "ws"
	if u.Scheme == "https" {
		scheme = "wss"
	}
	wsURL := fmt.Sprintf("%s://%s/v1/stream", scheme, u.Host)

	headers := make(http.Header)
	if c.Token != "" {
		headers.Set("Authorization", fmt.Sprintf("Bearer %s", c.Token))
	}

	conn, _, err := websocket.DefaultDialer.Dial(wsURL, headers)
	if err != nil {
		return err
	}
	defer conn.Close()

	for {
		_, message, err := conn.ReadMessage()
		if err != nil {
			return err
		}
		var ev VeynEvent
		if err := json.Unmarshal(message, &ev); err == nil {
			handler(ev)
		}
	}
}

// SubscribeContext establishes an SSE subscription to the live context bus.
func (c *Client) SubscribeContext(handler func(ContextSnapshot)) error {
	reqURL := fmt.Sprintf("%s/v1/context/subscribe", c.BaseURL)
	req, err := http.NewRequest("GET", reqURL, nil)
	if err != nil {
		return err
	}

	req.Header.Set("Accept", "text/event-stream")
	if c.Token != "" {
		req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", c.Token))
	}

	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("sse connection failed with status %d", resp.StatusCode)
	}

	reader := bufio.NewReader(resp.Body)
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			return err
		}
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "data:") {
			dataStr := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
			var snapshot ContextSnapshot
			if err := json.Unmarshal([]byte(dataStr), &snapshot); err == nil {
				handler(snapshot)
			}
		}
	}
}

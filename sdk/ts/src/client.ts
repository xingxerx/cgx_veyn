import * as http from "http";
import * as https from "https";
import * as url from "url";
import * as net from "net";

import type {
  ContextSnapshot,
  MemoryQuery,
  MemoryRecord,
  OutcomeRating,
  PatternRecord,
  VeynEvent,
  VeynDevice,
  Session,
  BaselineStats,
  HealthResponse,
  SubscribeFilter,
  TemporalSignal,
  WsFilter,
  WsContextFilter,
} from "./types";

// ── Helpers ───────────────────────────────────────────────────────────────────

function buildQuery(params: Record<string, string | number | undefined>): string {
  const parts: string[] = [];
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined) {
      parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
    }
  }
  return parts.length > 0 ? `?${parts.join("&")}` : "";
}

/** Minimal JSON-over-http fetch that works without any runtime deps. */
function fetchJson<T>(
  requestUrl: string,
  options: {
    method?: string;
    headers?: Record<string, string>;
    body?: string;
  } = {}
): Promise<T> {
  return new Promise((resolve, reject) => {
    const parsed = new url.URL(requestUrl);
    const isHttps = parsed.protocol === "https:";
    const lib = isHttps ? https : http;

    const reqOptions: http.RequestOptions = {
      hostname: parsed.hostname,
      port: parsed.port || (isHttps ? 443 : 80),
      path: parsed.pathname + parsed.search,
      method: options.method ?? "GET",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
        ...options.headers,
      },
    };

    const req = lib.request(reqOptions, (res) => {
      let raw = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => (raw += chunk));
      res.on("end", () => {
        if (res.statusCode !== undefined && res.statusCode >= 400) {
          reject(new Error(`HTTP ${res.statusCode}: ${raw}`));
          return;
        }
        try {
          resolve(JSON.parse(raw) as T);
        } catch (e) {
          reject(new Error(`Failed to parse JSON: ${raw}`));
        }
      });
    });

    req.on("error", reject);

    if (options.body) {
      req.write(options.body);
    }
    req.end();
  });
}

// ── VeynClient ────────────────────────────────────────────────────────────────

export class VeynClient {
  private readonly baseUrl: string;
  private readonly authHeader: string;

  constructor(baseUrl: string, token: string) {
    // Normalise: strip trailing slash
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.authHeader = `Bearer ${token}`;
  }

  private headers(): Record<string, string> {
    return { Authorization: this.authHeader };
  }

  private url(path: string): string {
    return `${this.baseUrl}${path}`;
  }

  // ── Health ──────────────────────────────────────────────────────────────────

  async getHealth(): Promise<HealthResponse> {
    return fetchJson<HealthResponse>(this.url("/v1/health"), {
      headers: this.headers(),
    });
  }

  // ── Context ─────────────────────────────────────────────────────────────────

  async getContext(): Promise<ContextSnapshot> {
    return fetchJson<ContextSnapshot>(this.url("/v1/context/current"), {
      headers: this.headers(),
    });
  }

  async getContextHistory(n?: number): Promise<ContextSnapshot[]> {
    const qs = buildQuery({ n });
    return fetchJson<ContextSnapshot[]>(
      this.url(`/v1/context/history${qs}`),
      { headers: this.headers() }
    );
  }

  /**
   * Subscribe to SSE context snapshot events.
   * Returns an unsubscribe function; call it to stop receiving events.
   * Auto-reconnects with a 1 s backoff on disconnection.
   */
  subscribe(
    onSnapshot: (snap: ContextSnapshot) => void,
    filter?: SubscribeFilter
  ): () => void {
    let cancelled = false;
    let currentReq: http.ClientRequest | null = null;

    const params: Record<string, string | number | undefined> = {
      min_confidence: filter?.minConfidence,
    };
    if (filter?.intents && filter.intents.length > 0) {
      params["intents"] = filter.intents.join(",");
    }
    if (filter?.sourceClass && filter.sourceClass.length > 0) {
      params["source_class"] = filter.sourceClass.join(",");
    }
    const qs = buildQuery(params);
    const requestUrl = this.url(`/v1/context/subscribe${qs}`);

    const connect = () => {
      if (cancelled) return;

      const parsed = new url.URL(requestUrl);
      const isHttps = parsed.protocol === "https:";
      const lib = isHttps ? https : http;

      const reqOptions: http.RequestOptions = {
        hostname: parsed.hostname,
        port: parsed.port || (isHttps ? 443 : 80),
        path: parsed.pathname + parsed.search,
        method: "GET",
        headers: {
          Authorization: this.authHeader,
          Accept: "text/event-stream",
          "Cache-Control": "no-cache",
        },
      };

      currentReq = lib.request(reqOptions, (res) => {
        let buffer = "";
        res.setEncoding("utf8");
        res.on("data", (chunk: string) => {
          buffer += chunk;
          const lines = buffer.split("\n");
          buffer = lines.pop() ?? "";
          let dataLine = "";
          for (const line of lines) {
            if (line.startsWith("data:")) {
              dataLine = line.slice(5).trim();
            } else if (line === "" && dataLine !== "") {
              try {
                const snap = JSON.parse(dataLine) as ContextSnapshot;
                onSnapshot(snap);
              } catch {
                // malformed event — skip
              }
              dataLine = "";
            }
          }
        });

        res.on("end", () => {
          if (!cancelled) {
            setTimeout(connect, 1000);
          }
        });

        res.on("error", () => {
          if (!cancelled) {
            setTimeout(connect, 1000);
          }
        });
      });

      currentReq.on("error", () => {
        if (!cancelled) {
          setTimeout(connect, 1000);
        }
      });

      currentReq.end();
    };

    connect();

    return () => {
      cancelled = true;
      currentReq?.destroy();
    };
  }

  // ── Events ──────────────────────────────────────────────────────────────────

  async getEvents(limit?: number): Promise<VeynEvent[]> {
    const qs = buildQuery({ limit });
    return fetchJson<VeynEvent[]>(this.url(`/v1/events/recent${qs}`), {
      headers: this.headers(),
    });
  }

  // ── Devices ─────────────────────────────────────────────────────────────────

  async getDevices(): Promise<VeynDevice[]> {
    return fetchJson<VeynDevice[]>(this.url("/v1/devices"), {
      headers: this.headers(),
    });
  }

  // ── Sessions ─────────────────────────────────────────────────────────────────

  async startSession(label: string, annotation?: string): Promise<Session> {
    const body: Record<string, string> = { label };
    if (annotation !== undefined) body["annotation"] = annotation;
    return fetchJson<Session>(this.url("/v1/session/start"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(body),
    });
  }

  async stopSession(): Promise<Session> {
    return fetchJson<Session>(this.url("/v1/session/stop"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({}),
    });
  }

  async getSession(id: string): Promise<Session> {
    return fetchJson<Session>(this.url(`/v1/session/${encodeURIComponent(id)}`), {
      headers: this.headers(),
    });
  }

  async replaySession(id: string): Promise<VeynEvent[]> {
    return fetchJson<VeynEvent[]>(
      this.url(`/v1/session/${encodeURIComponent(id)}/replay`),
      { headers: this.headers() }
    );
  }

  // ── Baseline ─────────────────────────────────────────────────────────────────

  async getBaseline(deviceId: string, metric: string): Promise<BaselineStats> {
    return fetchJson<BaselineStats>(
      this.url(
        `/v1/baseline/${encodeURIComponent(deviceId)}/${encodeURIComponent(metric)}`
      ),
      { headers: this.headers() }
    );
  }

  // ── Memory layer ─────────────────────────────────────────────────────────────

  /** Write a semantic memory record; the daemon attaches the current biometric state. */
  async writeMemory(topic: string, summary: string): Promise<MemoryRecord> {
    return fetchJson<MemoryRecord>(this.url("/v1/memory"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({ topic, summary }),
    });
  }

  /** Query memory records with optional filters. */
  async getMemory(query?: MemoryQuery): Promise<MemoryRecord[]> {
    const params: Record<string, string | number | undefined> = {
      topic: query?.topic,
      since: query?.since,
      until: query?.until,
      kind: query?.kind,
      limit: query?.limit,
    };
    const qs = buildQuery(params);
    const result = await fetchJson<{ records: MemoryRecord[]; count: number }>(
      this.url(`/v1/memory${qs}`),
      { headers: this.headers() }
    );
    return result.records;
  }

  /** Anchor an outcome rating to a memory record after the session. */
  async anchorOutcome(
    id: string,
    outcomeRating: OutcomeRating,
    notes?: string
  ): Promise<{ id: string; anchored: boolean }> {
    const body: Record<string, string> = { outcome_rating: outcomeRating };
    if (notes !== undefined) body["notes"] = notes;
    return fetchJson<{ id: string; anchored: boolean }>(
      this.url(`/v1/memory/${encodeURIComponent(id)}/outcome`),
      { method: "PATCH", headers: this.headers(), body: JSON.stringify(body) }
    );
  }

  // ── Pattern detection ─────────────────────────────────────────────────────────

  /** Return topic-level physiological patterns computed by veyn-insight. */
  async getPatterns(minSamples?: number): Promise<PatternRecord[]> {
    const qs = buildQuery({ min_samples: minSamples });
    const result = await fetchJson<{ patterns: PatternRecord[]; count: number }>(
      this.url(`/v1/patterns${qs}`),
      { headers: this.headers() }
    );
    return result.patterns;
  }

  // ── Temporal patterns ─────────────────────────────────────────────────────────

  /**
   * Return the current temporal trend signals for all metrics that have
   * accumulated enough samples in the sliding 20-minute window.
   */
  async getTemporalPatterns(): Promise<TemporalSignal[]> {
    const result = await fetchJson<{ patterns: TemporalSignal[]; count: number }>(
      this.url("/v1/temporal/patterns"),
      { headers: this.headers() }
    );
    return result.patterns;
  }

  // ── WebSocket stream ──────────────────────────────────────────────────────────

  /**
   * Subscribe to raw VeynEvent objects over WebSocket.
   * Returns an unsubscribe function; call it to close the connection.
   * Auto-reconnects with a 1 s backoff on disconnection or error.
   */
  wsSubscribe(
    onEvent: (ev: VeynEvent) => void,
    filter?: WsFilter
  ): () => void {
    let cancelled = false;
    let currentSocket: net.Socket | null = null;

    const params: Record<string, string | undefined> = {};
    if (filter?.deviceClass && filter.deviceClass.length > 0) {
      params["device_class"] = filter.deviceClass.join(",");
    }
    if (filter?.metrics && filter.metrics.length > 0) {
      params["metrics"] = filter.metrics.join(",");
    }
    const qs = buildQuery(params as Record<string, string | number | undefined>);

    // Derive WS URL from the HTTP base URL
    const wsUrl = this.baseUrl
      .replace(/^https:\/\//, "wss://")
      .replace(/^http:\/\//, "ws://")
      + `/v1/stream${qs}`;

    const connect = () => {
      if (cancelled) return;

      const parsed = new url.URL(wsUrl);
      const isWss = parsed.protocol === "wss:";
      const lib = isWss ? https : http;
      const port = parsed.port
        ? Number(parsed.port)
        : isWss
        ? 443
        : 80;

      // WebSocket opening handshake key
      const wsKey = Buffer.from(Math.random().toString(36)).toString("base64");

      const reqOptions: http.RequestOptions = {
        hostname: parsed.hostname,
        port,
        path: parsed.pathname + parsed.search,
        method: "GET",
        headers: {
          Authorization: this.authHeader,
          Upgrade: "websocket",
          Connection: "Upgrade",
          "Sec-WebSocket-Key": wsKey,
          "Sec-WebSocket-Version": "13",
        },
      };

      const req = lib.request(reqOptions);

      req.on("upgrade", (_res, socket, head) => {
        currentSocket = socket;
        let buf = head ? Buffer.from(head) : Buffer.alloc(0);

        socket.on("data", (chunk: Buffer) => {
          buf = Buffer.concat([buf, chunk]);
          // Parse WebSocket frames (text frames only, no masking from server)
          while (buf.length >= 2) {
            const firstByte = buf[0];
            const secondByte = buf[1];
            const opcode = firstByte & 0x0f;
            const payloadLen = secondByte & 0x7f;

            let headerLen = 2;
            let dataLen: number;

            if (payloadLen === 126) {
              if (buf.length < 4) break;
              dataLen = buf.readUInt16BE(2);
              headerLen = 4;
            } else if (payloadLen === 127) {
              if (buf.length < 10) break;
              // Use only lower 32 bits (sufficient for normal messages)
              dataLen = buf.readUInt32BE(6);
              headerLen = 10;
            } else {
              dataLen = payloadLen;
            }

            if (buf.length < headerLen + dataLen) break;

            const payload = buf.slice(headerLen, headerLen + dataLen);
            buf = buf.slice(headerLen + dataLen);

            if (opcode === 0x1) {
              // text frame
              try {
                const ev = JSON.parse(payload.toString("utf8")) as VeynEvent;
                onEvent(ev);
              } catch {
                // malformed — skip
              }
            } else if (opcode === 0x8) {
              // close frame
              socket.destroy();
            }
          }
        });

        socket.on("end", () => {
          if (!cancelled) setTimeout(connect, 1000);
        });

        socket.on("error", () => {
          if (!cancelled) setTimeout(connect, 1000);
        });
      });

      req.on("error", () => {
        if (!cancelled) setTimeout(connect, 1000);
      });

      req.end();
    };

    connect();

    return () => {
      cancelled = true;
      currentSocket?.destroy();
    };
  }

  /**
   * Subscribe to `ContextSnapshot` objects over WebSocket (Semantic tier).
   *
   * Returns an object with:
   * - `unsubscribe()` — closes the connection
   * - `setFilter(f)` — sends a runtime filter update to the server; the server
   *    will immediately start applying the new filter without reconnecting.
   *
   * Auto-reconnects with 1 s backoff on disconnection or error.
   */
  wsSubscribeContext(
    onSnapshot: (snap: ContextSnapshot) => void,
    initialFilter?: WsContextFilter
  ): { unsubscribe: () => void; setFilter: (f: WsContextFilter) => void } {
    let cancelled = false;
    let currentSocket: net.Socket | null = null;
    let pendingFilter: WsContextFilter | null = initialFilter ?? null;

    const wsUrl =
      this.baseUrl
        .replace(/^https:\/\//, "wss://")
        .replace(/^http:\/\//, "ws://") + "/v1/stream";

    const connect = () => {
      if (cancelled) return;

      const parsed = new url.URL(wsUrl);
      const isWss = parsed.protocol === "wss:";
      const lib = isWss ? https : http;
      const port = parsed.port ? Number(parsed.port) : isWss ? 443 : 80;
      const wsKey = Buffer.from(Math.random().toString(36)).toString("base64");

      const reqOptions: http.RequestOptions = {
        hostname: parsed.hostname,
        port,
        path: parsed.pathname + parsed.search,
        method: "GET",
        headers: {
          Authorization: this.authHeader,
          Upgrade: "websocket",
          Connection: "Upgrade",
          "Sec-WebSocket-Key": wsKey,
          "Sec-WebSocket-Version": "13",
        },
      };

      const req = lib.request(reqOptions);

      req.on("upgrade", (_res, socket, head) => {
        currentSocket = socket;
        let buf = head ? Buffer.from(head) : Buffer.alloc(0);

        // Send initial filter if one was supplied before connect.
        if (pendingFilter) {
          sendContextFilter(socket, pendingFilter);
          pendingFilter = null;
        }

        socket.on("data", (chunk: Buffer) => {
          buf = Buffer.concat([buf, chunk]);
          while (buf.length >= 2) {
            const firstByte = buf[0];
            const secondByte = buf[1];
            const opcode = firstByte & 0x0f;
            const payloadLen = secondByte & 0x7f;

            let headerLen = 2;
            let dataLen: number;

            if (payloadLen === 126) {
              if (buf.length < 4) break;
              dataLen = buf.readUInt16BE(2);
              headerLen = 4;
            } else if (payloadLen === 127) {
              if (buf.length < 10) break;
              dataLen = buf.readUInt32BE(6);
              headerLen = 10;
            } else {
              dataLen = payloadLen;
            }

            if (buf.length < headerLen + dataLen) break;

            const payload = buf.slice(headerLen, headerLen + dataLen);
            buf = buf.slice(headerLen + dataLen);

            if (opcode === 0x1) {
              try {
                const snap = JSON.parse(payload.toString("utf8")) as ContextSnapshot;
                onSnapshot(snap);
              } catch {
                // malformed — skip
              }
            } else if (opcode === 0x8) {
              socket.destroy();
            }
          }
        });

        socket.on("end", () => { if (!cancelled) setTimeout(connect, 1000); });
        socket.on("error", () => { if (!cancelled) setTimeout(connect, 1000); });
      });

      req.on("error", () => { if (!cancelled) setTimeout(connect, 1000); });
      req.end();
    };

    const sendContextFilter = (socket: net.Socket, f: WsContextFilter) => {
      const msg = JSON.stringify({
        type: "subscribe",
        context_filter: {
          intents: f.intents,
          min_confidence: f.minConfidence,
          source_class: f.sourceClass,
          exclude_neutral: f.excludeNeutral ?? false,
        },
      });
      const payload = Buffer.from(msg, "utf8");
      const frame = Buffer.alloc(2 + payload.length);
      frame[0] = 0x81; // FIN + text opcode
      frame[1] = payload.length;
      payload.copy(frame, 2);
      socket.write(frame);
    };

    connect();

    return {
      unsubscribe: () => {
        cancelled = true;
        currentSocket?.destroy();
      },
      setFilter: (f: WsContextFilter) => {
        if (currentSocket && !currentSocket.destroyed) {
          sendContextFilter(currentSocket, f);
        } else {
          pendingFilter = f;
        }
      },
    };
  }
}

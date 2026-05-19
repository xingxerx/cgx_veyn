/**
 * 12.1 – 12.4  Somatic Shell & Execution Environment
 *
 * SomaticShell — REPL wrapper that gates commands and adapts behaviour
 *               based on the operator's real-time biometric state.
 *
 * BiometricScheduler — Defers heavy tasks until the body state
 *               enters Approach or Recovery.
 *
 * JitterAdapter — Returns recommended keyboard debounce settings
 *               calibrated to the current CognitiveLoad level.
 */

import { VeynClient } from "./client";
import type { ContextSnapshot, IntentCode } from "./types";
import { exec as nodeExec } from "child_process";
import { promisify } from "util";

const exec = promisify(nodeExec);

// ── Risk classification ────────────────────────────────────────────────────────

/** Commands that must be blocked when the operator is biometrically impaired. */
const HIGH_RISK_PATTERNS: RegExp[] = [
  /\bgit\s+push\b/,
  /\bcargo\s+publish\b/,
  /\bnpm\s+publish\b/,
  /\brm\s+-[rf]+\b/,
  /\bdd\b/,
  /\bsudo\b/,
  /\bkubectl\s+delete\b/,
  /\bterraform\s+destroy\b/,
  /\bdropdb\b/,
  /\btruncate\b/,
];

/** Intent codes that trigger command gating. */
const GATE_INTENTS: readonly string[] = ["fatigue", "stress_response"];

// ── Override token ─────────────────────────────────────────────────────────────

/** Simple in-memory override: call `createOverrideToken()` to get a one-shot bypass. */
export function createOverrideToken(): string {
  return Math.random().toString(36).slice(2).toUpperCase();
}

// ── SomaticShell ──────────────────────────────────────────────────────────────

export interface ShellResult {
  stdout: string;
  stderr: string;
  blocked: boolean;
  blockReason?: string;
  intentCode?: string;
}

/**
 * 12.1 / 12.2  Somatic CLI interpreter.
 *
 * Wraps shell execution and hooks into `/v1/context/current` before every
 * command to decide whether execution should proceed.
 */
export class SomaticShell {
  private client: VeynClient;
  private overrideTokens = new Set<string>();
  private lastContext: ContextSnapshot | null = null;

  constructor(client: VeynClient) {
    this.client = client;
  }

  /** Register a manual override token (one-shot bypass). */
  addOverride(token: string): void {
    this.overrideTokens.add(token);
  }

  /** Fetch current biometric context; caches last successful result. */
  async getContext(): Promise<ContextSnapshot | null> {
    try {
      this.lastContext = await this.client.getContext();
    } catch {
      // daemon unreachable — use cached value
    }
    return this.lastContext;
  }

  /** Returns true if the command pattern is high-risk. */
  isHighRisk(command: string): boolean {
    return HIGH_RISK_PATTERNS.some((p) => p.test(command));
  }

  /**
   * Execute a shell command with biometric gating.
   *
   * @param command  The shell command to run.
   * @param override Optional one-shot override token to bypass gating.
   */
  async run(command: string, override?: string): Promise<ShellResult> {
    const ctx = await this.getContext();
    const intentCode = (ctx?.intent_code ?? "neutral") as string;
    const gated = GATE_INTENTS.includes(intentCode);
    const highRisk = this.isHighRisk(command);

    // Biometric gate: block if high-risk and operator is impaired.
    if (gated && highRisk) {
      if (override && this.overrideTokens.has(override)) {
        this.overrideTokens.delete(override); // consume one-shot token
      } else {
        return {
          stdout: "",
          stderr: "",
          blocked: true,
          blockReason: `Command blocked — biometric state: ${intentCode}. Provide an override token or wait for recovery.`,
          intentCode,
        };
      }
    }

    try {
      const { stdout, stderr } = await exec(command);
      return { stdout, stderr, blocked: false, intentCode };
    } catch (err: unknown) {
      const e = err as { stdout?: string; stderr?: string; message?: string };
      return {
        stdout: e.stdout ?? "",
        stderr: e.stderr ?? e.message ?? String(err),
        blocked: false,
        intentCode,
      };
    }
  }
}

// ── BiometricScheduler ────────────────────────────────────────────────────────

export type ScheduledTask = () => Promise<void>;

export interface ScheduledJob {
  id: string;
  name: string;
  task: ScheduledTask;
  queued_at: number;
  allow_intents: string[];
}

/**
 * 12.3  Biological Process Scheduler ("Biometric cron").
 *
 * Tasks queued here only execute when the operator's biometric context
 * has stabilised into an `approach` or `recovery` vector.
 */
export class BiometricScheduler {
  private client: VeynClient;
  private queue: ScheduledJob[] = [];
  private pollMs: number;
  private _running = false;
  private _timer?: ReturnType<typeof setTimeout>;

  constructor(client: VeynClient, pollIntervalMs = 10_000) {
    this.client = client;
    this.pollMs = pollIntervalMs;
  }

  /** Schedule a heavy task; it will run when biometrics permit. */
  schedule(
    name: string,
    task: ScheduledTask,
    allowIntents: string[] = ["approach", "recovery", "neutral"]
  ): string {
    const id = Math.random().toString(36).slice(2);
    this.queue.push({ id, name, task, queued_at: Date.now(), allow_intents: allowIntents });
    return id;
  }

  /** Cancel a queued task by id. */
  cancel(id: string): boolean {
    const before = this.queue.length;
    this.queue = this.queue.filter((j) => j.id !== id);
    return this.queue.length < before;
  }

  /** Start the polling loop. */
  start(): void {
    if (this._running) return;
    this._running = true;
    this._poll();
  }

  /** Stop the polling loop. */
  stop(): void {
    this._running = false;
    if (this._timer) clearTimeout(this._timer);
  }

  private async _poll(): Promise<void> {
    if (!this._running) return;

    try {
      const ctx = await this.client.getContext();
      const intent = (ctx?.intent_code ?? "neutral") as string;

      const runnable = this.queue.filter((j) => j.allow_intents.includes(intent));
      for (const job of runnable) {
        this.queue = this.queue.filter((j) => j.id !== job.id);
        try {
          await job.task();
        } catch {
          // re-queue on failure so it retries next poll
          this.queue.push(job);
        }
      }
    } catch {
      // context fetch failed — retry next poll
    }

    this._timer = setTimeout(() => this._poll(), this.pollMs);
  }
}

// ── JitterAdapter ─────────────────────────────────────────────────────────────

export interface DebounceSettings {
  /** Recommended key debounce delay in milliseconds. */
  debounce_ms: number;
  /** Autocomplete ranking bias: prefer short/simple completions when stressed. */
  prefer_simple: boolean;
  intent_code: string;
}

/**
 * 12.4  Dynamic Input Micro-Jitter Adaptation.
 *
 * Returns keyboard debounce recommendations calibrated to the current
 * biometric state.  Integrate with evdev/hidraw drivers or UI keybind layers.
 */
export class JitterAdapter {
  private client: VeynClient;

  constructor(client: VeynClient) {
    this.client = client;
  }

  async getSettings(): Promise<DebounceSettings> {
    let intentCode = "neutral";
    let confidence = 0.5;

    try {
      const ctx = await this.client.getContext();
      intentCode = (ctx?.intent_code ?? "neutral") as string;
      confidence = ctx?.confidence ?? 0.5;
    } catch {
      // daemon unreachable — use safe defaults
    }

    // Higher cognitive load → more debounce to suppress mis-types.
    let debounce_ms = 20; // default (ms)
    let prefer_simple = false;

    switch (intentCode) {
      case "cognitive_load":
        debounce_ms = 40 + Math.round((1 - confidence) * 60);
        prefer_simple = true;
        break;
      case "stress_response":
        debounce_ms = 60 + Math.round((1 - confidence) * 80);
        prefer_simple = true;
        break;
      case "fatigue":
        debounce_ms = 80;
        prefer_simple = true;
        break;
      default:
        debounce_ms = 20;
        prefer_simple = false;
    }

    return { debounce_ms, prefer_simple, intent_code: intentCode };
  }
}


#!/usr/bin/env node

/**
 * 12.1 — Somatic CLI Interpreter & REPL
 *
 * Implements a premium terminal REPL that reads somatic state from the
 * local VEYN daemon before every command, color-codes the prompt dynamically,
 * and intercepts high-risk commands if biometrics indicate stress or fatigue.
 */

import * as readline from "readline";
import { VeynClient } from "./client";
import { SomaticShell, createOverrideToken } from "./somatic";

// Color codes.
const RESET = "\x1b[0m";
const BOLD = "\x1b[1m";
const RED = "\x1b[31m";
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const BLUE = "\x1b[34m";
const CYAN = "\x1b[36m";

async function main() {
  console.log(`${BOLD}${CYAN}=== VEYN Somatic CLI v1.0 ===${RESET}`);
  console.log("Zero-Trust physiological shell wrapping your command execution.");
  console.log("High-risk commands are gated by live autonomic telemetry.\n");

  const daemonURL = process.env.VEYN_URL || "http://127.0.0.1:9000";
  const token = process.env.VEYN_TOKEN || "";

  console.log(`Connecting to VEYN daemon at: ${BLUE}${daemonURL}${RESET}...`);
  const client = new VeynClient(daemonURL, token);
  const shell = new SomaticShell(client);

  // Probe the daemon first.
  try {
    const health = await client.getHealth();
    console.log(`${GREEN}Connected successfully.${RESET} Server version: ${BOLD}${health.version || "0.1.0"}${RESET}\n`);
  } catch (e) {
    console.log(`${YELLOW}Warning: VEYN daemon not running at ${daemonURL}. Running in simulation/offline mode.${RESET}\n`);
  }

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  const promptUser = async () => {
    // 1. Fetch current biometric context.
    const ctx = await shell.getContext();
    const intent = ctx?.intent_code || "neutral";
    const confidence = ctx?.confidence || 1.0;

    // 2. Dynamic prompt styling.
    let color = GREEN;
    let badge = "NEUTRAL";

    if (intent === "stress_response") {
      color = RED;
      badge = "STRESS";
    } else if (intent === "fatigue") {
      color = YELLOW;
      badge = "FATIGUE";
    } else if (intent === "cognitive_load") {
      color = BLUE;
      badge = "COGNITIVE_LOAD";
    } else if (intent === "approach") {
      color = CYAN;
      badge = "APPROACH";
    } else if (intent === "recovery") {
      color = GREEN;
      badge = "RECOVERY";
    }

    const promptStr = `${BOLD}${color}[VEYN:${badge} (${(confidence * 100).toFixed(0)}%)]${RESET} $ `;
    rl.question(promptStr, async (input) => {
      const trimmed = input.trim();

      if (trimmed === "exit" || trimmed === "quit") {
        rl.close();
        return;
      }

      if (trimmed === "") {
        promptUser();
        return;
      }

      // Help menu.
      if (trimmed === "help") {
        console.log(`\n${BOLD}Somatic CLI Special Commands:${RESET}`);
        console.log("  override    - Request a one-shot biological bypass token");
        console.log("  status      - Display full live somatic biometric state");
        console.log("  exit / quit - Close Somatic CLI\n");
        promptUser();
        return;
      }

      // Live status.
      if (trimmed === "status") {
        if (ctx) {
          console.log(`\n${BOLD}=== Live Somatic State ===${RESET}`);
          console.log(`Intent Code:      ${color}${ctx.intent_code}${RESET}`);
          console.log(`Confidence:       ${(ctx.confidence * 100).toFixed(1)}%`);
          console.log(`Session ID:       ${ctx.session_id}`);
          if (ctx.baseline_delta) {
            console.log(`${BOLD}Baseline Deltas (z-scores):${RESET}`);
            for (const [k, v] of Object.entries(ctx.baseline_delta)) {
              const sign = v >= 0 ? "+" : "";
              console.log(`  ${k.padEnd(16)}: ${v >= 1.5 || v <= -1.5 ? RED : GREEN}${sign}${v.toFixed(3)}σ${RESET}`);
            }
          }
          console.log();
        } else {
          console.log("\nNo biometric telemetry received from VEYN daemon.\n");
        }
        promptUser();
        return;
      }

      // Override request.
      if (trimmed === "override") {
        const token = createOverrideToken();
        shell.addOverride(token);
        console.log(`\n${BOLD}${YELLOW}=== One-Shot Override Token Generated ===${RESET}`);
        console.log(`Token: ${BOLD}${token}${RESET}`);
        console.log("Bypass command blockage by appending it: command --override=<TOKEN>\n");
        promptUser();
        return;
      }

      // 3. Command execution with physiological gating.
      let overrideToken: string | undefined;
      let targetCommand = trimmed;

      const overrideMatch = trimmed.match(/--override=([A-Z0-9]+)/);
      if (overrideMatch) {
        overrideToken = overrideMatch[1];
        targetCommand = trimmed.replace(/--override=[A-Z0-9]+/, "").trim();
      }

      const res = await shell.run(targetCommand, overrideToken);

      if (res.blocked) {
        console.log(`\n${BOLD}${RED}[BLOCKED BY VEYN BIOMETRIC GATE]${RESET}`);
        console.log(`${res.blockReason}\n`);
      } else {
        if (res.stdout) process.stdout.write(res.stdout);
        if (res.stderr) process.stderr.write(res.stderr);
      }

      promptUser();
    });
  };

  promptUser();
}

main().catch((err) => {
  console.error("CLI crashed:", err);
});

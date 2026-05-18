/**
 * context_reader.ts — Fetch the current VEYN context snapshot and 10 history items.
 *
 * Compile and run:
 *   cd sdk/ts && npm run build
 *   node dist/../../../examples/context_reader.js  \
 *       --url http://localhost:8888 --token dev-token
 *
 * Or execute directly with ts-node:
 *   npx ts-node sdk/examples/context_reader.ts
 */

import { VeynClient } from "../ts/src/client";
import type { ContextSnapshot } from "../ts/src/types";

// ── CLI args ──────────────────────────────────────────────────────────────────

function getArg(flag: string, defaultValue: string): string {
  const idx = process.argv.indexOf(flag);
  return idx !== -1 && process.argv[idx + 1] ? process.argv[idx + 1] : defaultValue;
}

const BASE_URL = getArg("--url", "http://localhost:8888");
const TOKEN = getArg("--token", "dev-token");

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatSnapshot(snap: ContextSnapshot, label: string): void {
  console.log(`\n── ${label} ──────────────────────────────────────────`);
  console.log(`  timestamp_ms      : ${new Date(snap.timestamp_ms).toISOString()}`);
  console.log(`  session_id        : ${snap.session_id}`);
  console.log(`  intent            : ${snap.intent}`);
  console.log(`  intent_code       : ${snap.intent_code}`);
  console.log(`  confidence        : ${snap.confidence.toFixed(3)}`);
  console.log(`  intent_confidence : ${snap.intent_confidence.toFixed(3)}`);
  console.log(`  active_devices    : [${snap.active_devices.join(", ")}]`);
  console.log(`  state_deltas      : ${snap.state_deltas.length} entries`);
  for (const delta of snap.state_deltas) {
    console.log(
      `    ${delta.device_id}/${delta.metric} = ${delta.value} ${delta.unit}` +
        ` (${delta.source_class})`
    );
  }
  if (snap.baseline_delta) {
    console.log("  baseline_delta    :");
    for (const [metric, z] of Object.entries(snap.baseline_delta)) {
      const sign = z >= 0 ? "+" : "";
      console.log(`    ${metric}: ${sign}${z.toFixed(2)}σ`);
    }
  }
  if (snap.recording_session_id) {
    console.log(`  recording_session : ${snap.recording_session_id}`);
  }
}

// ── Main ──────────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  const client = new VeynClient(BASE_URL, TOKEN);

  console.log(`Connecting to VEYN at ${BASE_URL} …`);

  // Health check
  const health = await client.getHealth();
  console.log(`Daemon health: ${JSON.stringify(health)}`);

  // Current context
  const current = await client.getContext();
  formatSnapshot(current, "Current context");

  // History
  const history = await client.getContextHistory(10);
  console.log(`\nFetched ${history.length} history snapshot(s):`);
  history.forEach((snap, i) => {
    formatSnapshot(snap, `History[${i}]`);
  });

  console.log("\nDone.");
}

main().catch((err) => {
  console.error("Error:", err instanceof Error ? err.message : err);
  process.exit(1);
});

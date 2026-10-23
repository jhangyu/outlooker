// Per-test time judgment shared by scripts/cargo-test-runner.mjs and scripts/vitest-capped.mjs (CLAUDE.md "Test
// policy"). A test over its base cap was rerun alone; `judge` decides on that solo time.
import { availableParallelism, loadavg } from "node:os";

export const TOLERANCE_S = 6;
const BUSY = 0.25;

// ponytail: "busy" = 1-min load / cores >= 0.25 (quiet ~4, busy 15-32 on 28 cores, 2026-10-23). TEST_LOAD_OVERRIDE
// replaces the measured load average, for the red/green proofs only. Ceiling: load average lags ~1 min behind a burst.
export function judge(name, seconds, capS) {
  const load = Number(process.env.TEST_LOAD_OVERRIDE ?? loadavg()[0]);
  const cores = availableParallelism();
  const busy = load / cores >= BUSY;
  const at = `load ${load.toFixed(1)}/${cores} cores, busy threshold ${BUSY}`;
  if (seconds <= capS) return { fail: false, line: `slow under contention: ${name} ${seconds}s alone <= ${capS} s cap (${at})` };
  if (busy && seconds <= TOLERANCE_S) {
    return { fail: false, line: `WARNING: ${name} ${seconds}s alone > ${capS} s cap, allowed while busy up to ${TOLERANCE_S} s (${at})` };
  }
  return { fail: true, line: `FAIL: ${name} ${seconds}s alone > ${capS} s cap (${busy ? `over ${TOLERANCE_S} s tolerance; ` : "machine not busy; "}${at})` };
}

import { describe, expect, it } from "vitest";

import type { SkillQueueEntry } from "../../../api/characters";
import { deriveTrainingState } from "../training-state";

const HOUR = 60 * 60 * 1000;

function entry(queuePosition: number, startDate: string | null, finishDate: string | null): SkillQueueEntry {
  return {
    skillId: 1000 + queuePosition,
    skillName: null,
    finishedLevel: 1,
    queuePosition,
    startDate,
    finishDate,
    trainingStartSp: null,
    levelStartSp: null,
    levelEndSp: null,
    currentTrainedLevel: null,
  };
}

describe("deriveTrainingState", () => {
  it("returns empty for an empty queue", () => {
    expect(deriveTrainingState([], Date.now())).toEqual({ kind: "empty" });
  });

  it("returns active for a single in-progress entry", () => {
    const now = Date.now();
    const e = entry(0, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString());
    const state = deriveTrainingState([e], now);
    expect(state.kind).toBe("active");
    if (state.kind === "active") {
      expect(state.entry).toEqual(e);
    }
  });

  it("returns cachedQueueExpired for a single completed entry with nothing after", () => {
    const now = Date.now();
    const finish = new Date(now - 5 * 60 * 1000).toISOString();
    const e = entry(0, new Date(now - HOUR).toISOString(), finish);
    expect(deriveTrainingState([e], now)).toEqual({ kind: "cachedQueueExpired", knownUntil: finish });
  });

  it("skips one completed entry to find the active next one", () => {
    const now = Date.now();
    const completed = entry(0, new Date(now - 2 * HOUR).toISOString(), new Date(now - HOUR).toISOString());
    const active = entry(1, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString());
    const state = deriveTrainingState([completed, active], now);
    expect(state.kind).toBe("active");
    if (state.kind === "active") expect(state.entry.skillId).toBe(active.skillId);
  });

  it("skips multiple completed entries to find the active next one", () => {
    const now = Date.now();
    const a = entry(0, new Date(now - 3 * HOUR).toISOString(), new Date(now - 2 * HOUR).toISOString());
    const b = entry(1, new Date(now - 2 * HOUR).toISOString(), new Date(now - HOUR).toISOString());
    const active = entry(2, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString());
    const state = deriveTrainingState([a, b, active], now);
    expect(state.kind).toBe("active");
    if (state.kind === "active") expect(state.entry.skillId).toBe(active.skillId);
  });

  it("returns paused reporting itself as next for a single uniformly-undated entry", () => {
    const now = Date.now();
    const e = entry(0, null, null);
    expect(deriveTrainingState([e], now)).toEqual({ kind: "paused", next: e });
  });

  it("returns paused reporting the first entry as next when every entry is undated", () => {
    const now = Date.now();
    const first = entry(0, null, null);
    const second = entry(1, null, null);
    expect(deriveTrainingState([first, second], now)).toEqual({ kind: "paused", next: first });
  });

  it("does not treat a mixed dated/undated queue as paused", () => {
    // Real ESI never mixes dated/undated entries in one response -- a
    // pause omits dates on every entry at once. A mix like this is
    // malformed/stale data and must not be silently upgraded into a
    // confident paused state (refinement #1).
    const now = Date.now();
    const completed = entry(0, new Date(now - 2 * HOUR).toISOString(), new Date(now - HOUR).toISOString());
    const undated = entry(1, null, null);
    const state = deriveTrainingState([completed, undated], now);
    expect(state.kind).toBe("cachedQueueExpired");
  });

  it("drops an entry missing only finishDate as malformed", () => {
    const now = Date.now();
    const malformed = entry(0, new Date(now - HOUR).toISOString(), null);
    const active = entry(1, new Date(now - 30 * 60 * 1000).toISOString(), new Date(now + HOUR).toISOString());
    const state = deriveTrainingState([malformed, active], now);
    expect(state.kind).toBe("active");
    if (state.kind === "active") expect(state.entry.skillId).toBe(active.skillId);
  });

  it("drops an entry missing only startDate as malformed", () => {
    const now = Date.now();
    const malformed = entry(0, null, new Date(now + HOUR).toISOString());
    const active = entry(1, new Date(now - 30 * 60 * 1000).toISOString(), new Date(now + 2 * HOUR).toISOString());
    const state = deriveTrainingState([malformed, active], now);
    expect(state.kind).toBe("active");
    if (state.kind === "active") expect(state.entry.skillId).toBe(active.skillId);
  });

  it("drops an entry with an invalid time range and never makes it active", () => {
    const now = Date.now();
    const malformed = entry(0, new Date(now + HOUR).toISOString(), new Date(now - HOUR).toISOString());
    expect(deriveTrainingState([malformed], now)).toEqual({ kind: "empty" });
  });

  it("folds a future gap with no active match into paused", () => {
    const now = Date.now();
    const future = entry(0, new Date(now + 10 * 60 * 1000).toISOString(), new Date(now + HOUR).toISOString());
    expect(deriveTrainingState([future], now)).toEqual({ kind: "paused", next: future });
  });

  it("treats a not-yet-started single entry as paused with itself as next, fraction unused", () => {
    const now = Date.now();
    const start = new Date(now + 500).toISOString();
    const finish = new Date(now + 1500).toISOString();
    const state = deriveTrainingState([entry(0, start, finish)], now);
    expect(state).toEqual({ kind: "paused", next: entry(0, start, finish) });
  });

  it("never returns a negative remainingMs and clamps fraction within [0,1] near the finish boundary", () => {
    const now = Date.now();
    const start = new Date(now - HOUR).toISOString();
    const finish = new Date(now + 100).toISOString();
    const state = deriveTrainingState([entry(0, start, finish)], now);
    expect(state.kind).toBe("active");
    if (state.kind === "active") {
      expect(state.remainingMs).toBeGreaterThanOrEqual(0);
      expect(state.fraction).toBeGreaterThanOrEqual(0);
      expect(state.fraction).toBeLessThanOrEqual(1);
    }
  });

  it("derives currentSp from the same fraction used for the progress bar", () => {
    const now = Date.now();
    const start = now - HOUR;
    const finish = now + HOUR;
    const e: SkillQueueEntry = {
      ...entry(0, new Date(start).toISOString(), new Date(finish).toISOString()),
      trainingStartSp: 1_000_000,
      levelStartSp: null,
      levelEndSp: 1_100_000,
      currentTrainedLevel: null,
    };
    const state = deriveTrainingState([e], now);
    expect(state.kind).toBe("active");
    if (state.kind === "active") {
      const expectedSp = 1_000_000 + state.fraction * (1_100_000 - 1_000_000);
      expect(currentSp(state)).toBeCloseTo(expectedSp, 5);
    }
  });
});

function currentSp(state: { fraction: number; entry: SkillQueueEntry }): number | null {
  if (state.entry.trainingStartSp === null || state.entry.levelEndSp === null) return null;
  return state.entry.trainingStartSp + state.fraction * (state.entry.levelEndSp - state.entry.trainingStartSp);
}

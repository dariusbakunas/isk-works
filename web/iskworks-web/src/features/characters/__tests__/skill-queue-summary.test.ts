import { describe, expect, test } from "vitest";

import type { SkillQueueEntry, SourceRefreshState } from "../../../api/characters";
import {
  deriveSkillQueueSummary,
  SKILL_QUEUE_CAPACITY,
  type SkillQueueSummaryInput,
} from "../skill-queue-summary";
import { formatRemainingMs } from "../characters-formatters";

const NOW = Date.parse("2026-09-09T12:00:00Z");
const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

function entry(overrides: Partial<SkillQueueEntry> = {}): SkillQueueEntry {
  return {
    skillId: 3327,
    skillName: "Caldari Industrial",
    finishedLevel: 5,
    queuePosition: 0,
    startDate: null,
    finishDate: null,
    trainingStartSp: null,
    levelStartSp: null,
    levelEndSp: null,
    currentTrainedLevel: null,
    ...overrides,
  };
}

function input(overrides: Partial<SkillQueueSummaryInput> = {}): SkillQueueSummaryInput {
  return {
    trainingQueue: [],
    trainingObservedAt: new Date(NOW - 5 * MIN).toISOString(),
    totalSp: 94_300_000,
    unallocatedSp: 0,
    trainingQueueScopeMissing: false,
    skillsSourceState: "current" as SourceRefreshState,
    skillsSourceMissingScope: false,
    ...overrides,
  };
}

// An active head entry (half-trained) followed by `queued` further entries.
function activeQueue(): SkillQueueEntry[] {
  return [
    entry({
      skillId: 100,
      skillName: "Target Navigation Prediction",
      finishedLevel: 5,
      queuePosition: 0,
      startDate: new Date(NOW - 30 * MIN).toISOString(),
      finishDate: new Date(NOW + 30 * MIN).toISOString(),
      trainingStartSp: 1_000_000,
      levelStartSp: 1_000_000,
      levelEndSp: 1_100_000,
      currentTrainedLevel: null,
    }),
    entry({
      skillId: 101,
      skillName: "Missile Projection",
      finishedLevel: 4,
      queuePosition: 1,
      startDate: new Date(NOW + 30 * MIN).toISOString(),
      finishDate: new Date(NOW + 30 * MIN + 7 * HOUR).toISOString(),
      trainingStartSp: 200_000,
      levelStartSp: 200_000,
      levelEndSp: 500_000,
      currentTrainedLevel: null,
    }),
    entry({
      skillId: 102,
      skillName: "Missile Projection",
      finishedLevel: 5,
      queuePosition: 2,
      startDate: new Date(NOW + 30 * MIN + 7 * HOUR).toISOString(),
      finishDate: new Date(NOW + 3 * DAY).toISOString(),
      trainingStartSp: 500_000,
      levelStartSp: 500_000,
      levelEndSp: 1_400_000,
      currentTrainedLevel: null,
    }),
  ];
}

describe("deriveSkillQueueSummary — capacity & count", () => {
  test("capacity is EVE's current 150-entry limit, count is the real entry count", () => {
    const summary = deriveSkillQueueSummary(input({ trainingQueue: activeQueue() }), NOW);
    expect(summary.queueCapacity).toBe(150);
    expect(SKILL_QUEUE_CAPACITY).toBe(150);
    expect(summary.queueCount).toBe(3);
  });
});

describe("deriveSkillQueueSummary — status", () => {
  test("missing skill-queue scope is reauthRequired, never an empty queue", () => {
    const summary = deriveSkillQueueSummary(
      input({ trainingQueue: activeQueue(), trainingQueueScopeMissing: true }),
      NOW,
    );
    expect(summary.status).toBe("reauthRequired");
  });

  test("missing base skills scope is also reauthRequired", () => {
    expect(deriveSkillQueueSummary(input({ skillsSourceMissingScope: true }), NOW).status).toBe("reauthRequired");
  });

  test("failed skills source is syncFailed", () => {
    expect(deriveSkillQueueSummary(input({ skillsSourceState: "failed" }), NOW).status).toBe("syncFailed");
  });

  test("never observed is notSynced", () => {
    expect(deriveSkillQueueSummary(input({ trainingObservedAt: null }), NOW).status).toBe("notSynced");
  });

  test("synced but no entries is queueEmpty", () => {
    expect(deriveSkillQueueSummary(input({ trainingQueue: [] }), NOW).status).toBe("queueEmpty");
  });

  test("active head entry is trainingActive", () => {
    expect(deriveSkillQueueSummary(input({ trainingQueue: activeQueue() }), NOW).status).toBe("trainingActive");
  });

  test("entries present but none active (uniform no dates) is queuePaused, not inferred active", () => {
    const paused = [entry({ queuePosition: 0 }), entry({ queuePosition: 1, skillId: 2 })];
    expect(deriveSkillQueueSummary(input({ trainingQueue: paused }), NOW).status).toBe("queuePaused");
  });
});

describe("deriveSkillQueueSummary — queue order", () => {
  test("rows are sorted strictly by queuePosition regardless of input order", () => {
    const shuffled = [activeQueue()[2], activeQueue()[0], activeQueue()[1]];
    const summary = deriveSkillQueueSummary(input({ trainingQueue: shuffled }), NOW);
    expect(summary.rows.map((r) => r.entry.queuePosition)).toEqual([0, 1, 2]);
  });

  test("successive levels of one skill stay as independent rows, never collapsed", () => {
    const queue = [1, 2, 3, 4, 5].map((level, i) =>
      entry({
        skillId: 500,
        skillName: "Missile Projection",
        finishedLevel: level,
        queuePosition: i,
        startDate: new Date(NOW + i * HOUR).toISOString(),
        finishDate: new Date(NOW + (i + 1) * HOUR).toISOString(),
        trainingStartSp: level * 1000,
        levelStartSp: level * 1000,
        levelEndSp: level * 1000 + 900,
        currentTrainedLevel: null,
      }),
    );
    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    expect(summary.rows).toHaveLength(5);
    expect(summary.rows.map((r) => r.targetLevel)).toEqual([1, 2, 3, 4, 5]);
  });

  test("targetLevel mirrors finishedLevel for every level I–V", () => {
    for (const level of [1, 2, 3, 4, 5]) {
      const summary = deriveSkillQueueSummary(
        input({ trainingQueue: [entry({ finishedLevel: level, queuePosition: 0 })] }),
        NOW,
      );
      expect(summary.rows[0].targetLevel).toBe(level);
    }
  });
});

describe("deriveSkillQueueSummary — per-row duration semantics", () => {
  test("active row counts down from now; queued rows show their own endDate-startDate", () => {
    const summary = deriveSkillQueueSummary(input({ trainingQueue: activeQueue() }), NOW);
    const [current, q1, q2] = summary.rows;

    expect(current.status).toBe("training");
    expect(current.durationKind).toBe("remaining");
    expect(current.durationMs).toBe(30 * MIN);

    expect(q1.status).toBe("queued");
    expect(q1.durationKind).toBe("nominal");
    expect(q1.durationMs).toBe(7 * HOUR);

    expect(q2.durationMs).toBe(
      Date.parse(q2.entry.finishDate!) - Date.parse(q2.entry.startDate!),
    );
  });

  test("an already-finished entry is dropped from the visible current queue", () => {
    const queue = [
      entry({
        skillId: 1,
        finishedLevel: 3,
        queuePosition: 0,
        startDate: new Date(NOW - 2 * HOUR).toISOString(),
        finishDate: new Date(NOW - 1 * HOUR).toISOString(),
      }),
      ...activeQueue().map((e) => entry({ ...e, queuePosition: e.queuePosition + 1 })),
    ];
    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    // completed history is not part of the current queue
    expect(summary.rows.some((r) => r.entry.skillId === 1)).toBe(false);
    expect(summary.rows.every((r) => r.status !== "completed")).toBe(true);
    // deriveTrainingState still skipped forward past it to the active entry
    expect(summary.rows[0].status).toBe("training");
    expect(summary.rows[0].entry.skillId).toBe(100);
    expect(summary.rows.every((r) => r.durationMs >= 0)).toBe(true);
    // count reflects unfinished entries only (active + 2 future queued)
    expect(summary.queueCount).toBe(3);
  });
});

describe("deriveSkillQueueSummary — SP remaining", () => {
  test("current = untrained portion of its level; queued = full level delta; completed = 0", () => {
    const queue = [
      // completed
      entry({
        skillId: 1,
        finishedLevel: 2,
        queuePosition: 0,
        startDate: new Date(NOW - 2 * HOUR).toISOString(),
        finishDate: new Date(NOW - 1 * HOUR).toISOString(),
        trainingStartSp: 0,
        levelStartSp: 0,
        levelEndSp: 999_999,
        currentTrainedLevel: null,
      }),
      // active, half-way: 100k level, 1M..1.1M, started at 1M -> 50k left
      entry({
        skillId: 2,
        finishedLevel: 5,
        queuePosition: 1,
        startDate: new Date(NOW - 30 * MIN).toISOString(),
        finishDate: new Date(NOW + 30 * MIN).toISOString(),
        trainingStartSp: 1_000_000,
        levelStartSp: 1_000_000,
        levelEndSp: 1_100_000,
        currentTrainedLevel: null,
      }),
      // queued, full level delta 300k (levelStart..levelEnd)
      entry({
        skillId: 3,
        finishedLevel: 4,
        queuePosition: 2,
        startDate: new Date(NOW + 30 * MIN).toISOString(),
        finishDate: new Date(NOW + 4 * HOUR).toISOString(),
        trainingStartSp: 200_000,
        levelStartSp: 200_000,
        levelEndSp: 500_000,
        currentTrainedLevel: null,
      }),
      // queued, only trainingStartSp + levelEndSp -> 400k
      entry({
        skillId: 4,
        finishedLevel: 3,
        queuePosition: 3,
        startDate: new Date(NOW + 4 * HOUR).toISOString(),
        finishDate: new Date(NOW + 8 * HOUR).toISOString(),
        trainingStartSp: 600_000,
        levelStartSp: null,
        levelEndSp: 1_000_000,
        currentTrainedLevel: null,
      }),
    ];
    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    // 50_000 (active) + 300_000 (queued) + 400_000 (queued) + 0 (completed)
    expect(summary.spRemaining).toBe(750_000);
  });

  test("empty queue has zero SP remaining", () => {
    expect(deriveSkillQueueSummary(input({ trainingQueue: [] }), NOW).spRemaining).toBe(0);
  });

  test("compact SP formatting of a realistic remaining value", () => {
    const queue = [
      entry({
        skillId: 2,
        finishedLevel: 5,
        queuePosition: 0,
        startDate: new Date(NOW - 30 * MIN).toISOString(),
        finishDate: new Date(NOW + 30 * MIN).toISOString(),
        trainingStartSp: 0,
        levelStartSp: 0,
        levelEndSp: 26_454_179 * 2,
        currentTrainedLevel: null,
      }),
    ];
    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    expect(summary.spRemaining).toBe(26_454_179);
  });
});

describe("deriveSkillQueueSummary — total queue remaining", () => {
  test("active queue: wall-clock time until the final entry finishes", () => {
    const summary = deriveSkillQueueSummary(input({ trainingQueue: activeQueue() }), NOW);
    expect(summary.totalRemainingKind).toBe("wallClock");
    expect(summary.totalRemainingMs).toBe(3 * DAY);
  });

  test("paused queue (no dates anywhere): nominal total of 0", () => {
    const paused = [entry({ queuePosition: 0 }), entry({ queuePosition: 1, skillId: 2 })];
    const summary = deriveSkillQueueSummary(input({ trainingQueue: paused }), NOW);
    expect(summary.totalRemainingKind).toBe("nominal");
    expect(summary.totalRemainingMs).toBe(0);
  });

  test("empty queue: zero remaining", () => {
    expect(deriveSkillQueueSummary(input({ trainingQueue: [] }), NOW).totalRemainingMs).toBe(0);
  });

  test("very long queue (>365 days) formats as days, no year rollover", () => {
    const queue = [
      entry({
        skillId: 1,
        finishedLevel: 5,
        queuePosition: 0,
        startDate: new Date(NOW - HOUR).toISOString(),
        finishDate: new Date(NOW + 559 * DAY + 22 * HOUR).toISOString(),
      }),
    ];
    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    expect(summary.totalRemainingMs).toBe(559 * DAY + 22 * HOUR);
    expect(formatRemainingMs(summary.totalRemainingMs)).toBe("559d 22h");
  });
});

describe("deriveSkillQueueSummary — realistic long queue", () => {
  test("an 80-entry queue renders every entry, in order, with one identifiable current row", () => {
    const queue: SkillQueueEntry[] = [];
    // head entry is active
    queue.push(
      entry({
        skillId: 1000,
        skillName: "Head Skill",
        finishedLevel: 5,
        queuePosition: 0,
        startDate: new Date(NOW - 10 * MIN).toISOString(),
        finishDate: new Date(NOW + 50 * MIN).toISOString(),
        trainingStartSp: 0,
        levelStartSp: 0,
        levelEndSp: 100_000,
        currentTrainedLevel: null,
      }),
    );
    let cursor = NOW + 50 * MIN;
    for (let i = 1; i < 80; i += 1) {
      const start = cursor;
      cursor += (i % 5 + 1) * HOUR;
      queue.push(
        entry({
          skillId: 1000 + i,
          skillName: `Queued Skill ${i}`,
          finishedLevel: (i % 5) + 1,
          queuePosition: i,
          startDate: new Date(start).toISOString(),
          finishDate: new Date(cursor).toISOString(),
          trainingStartSp: 1000 * i,
          levelStartSp: 1000 * i,
          levelEndSp: 1000 * i + 5000,
          currentTrainedLevel: null,
        }),
      );
    }

    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    expect(summary.queueCount).toBe(80);
    expect(summary.rows).toHaveLength(80);
    expect(summary.rows.map((r) => r.entry.queuePosition)).toEqual(
      Array.from({ length: 80 }, (_, i) => i),
    );
    const currentRows = summary.rows.filter((r) => r.status === "training");
    expect(currentRows).toHaveLength(1);
    expect(currentRows[0].entry.queuePosition).toBe(0);
  });
});

describe("deriveSkillQueueSummary — effective trained level reconciliation", () => {
  // /skills/ snapshot lags /skillqueue/: it still says Hull Upgrades II
  // while the queue proves III completed and IV training.
  function hullUpgradesQueue(): SkillQueueEntry[] {
    return [
      entry({
        skillId: 3392,
        skillName: "Hull Upgrades",
        finishedLevel: 3,
        queuePosition: 0,
        currentTrainedLevel: 2,
        startDate: new Date(NOW - 3 * HOUR).toISOString(),
        finishDate: new Date(NOW - 2 * HOUR).toISOString(),
        trainingStartSp: 16_000,
        levelStartSp: 16_000,
        levelEndSp: 90_510,
      }),
      entry({
        skillId: 3392,
        skillName: "Hull Upgrades",
        finishedLevel: 4,
        queuePosition: 1,
        currentTrainedLevel: 2,
        startDate: new Date(NOW - 2 * HOUR).toISOString(),
        finishDate: new Date(NOW + 20 * HOUR).toISOString(),
        trainingStartSp: 90_510,
        levelStartSp: 90_510,
        levelEndSp: 512_000,
      }),
      entry({
        skillId: 3392,
        skillName: "Hull Upgrades",
        finishedLevel: 5,
        queuePosition: 2,
        currentTrainedLevel: 2,
        startDate: new Date(NOW + 20 * HOUR).toISOString(),
        finishDate: new Date(NOW + 6 * DAY).toISOString(),
        trainingStartSp: 512_000,
        levelStartSp: 512_000,
        levelEndSp: 3_000_000,
      }),
    ];
  }

  test("stale snapshot II + completed III + active IV -> active row shows I-III trained, IV training", () => {
    const summary = deriveSkillQueueSummary(input({ trainingQueue: hullUpgradesQueue() }), NOW);

    const active = summary.rows.find((r) => r.status === "training");
    expect(active?.entry.finishedLevel).toBe(4);
    expect(active?.levelCells).toEqual(["trained", "trained", "trained", "training", "empty"]);

    // completed Hull Upgrades III is not a current-queue row, and not counted
    expect(summary.rows.some((r) => r.entry.finishedLevel === 3)).toBe(false);
    expect(summary.queueCount).toBe(2); // IV active + V queued
  });

  test("a later same-skill row (Hull Upgrades V) also reflects the reconciled level and the animating IV", () => {
    const summary = deriveSkillQueueSummary(input({ trainingQueue: hullUpgradesQueue() }), NOW);
    const vRow = summary.rows.find((r) => r.entry.finishedLevel === 5);
    // effective trained level is III (completed III / active IV-1) -> I-III
    // trained; IV is the level training right now so it animates in this row
    // too; V is this row's own queued target.
    expect(vRow?.levelCells).toEqual(["trained", "trained", "trained", "training", "queued"]);
  });

  test("future successive levels are NEVER reconciled from queue order", () => {
    const queue = [1, 2, 3].map((level, i) =>
      entry({
        skillId: 500,
        skillName: "Some Skill",
        finishedLevel: level,
        queuePosition: i,
        currentTrainedLevel: 0,
        startDate: new Date(NOW + (i + 1) * HOUR).toISOString(),
        finishDate: new Date(NOW + (i + 2) * HOUR).toISOString(),
        trainingStartSp: level * 1000,
        levelStartSp: level * 1000,
        levelEndSp: level * 1000 + 500,
      }),
    );
    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    expect(summary.rows.map((r) => r.levelCells)).toEqual([
      ["queued", "empty", "empty", "empty", "empty"],
      ["queued", "queued", "empty", "empty", "empty"],
      ["queued", "queued", "queued", "empty", "empty"],
    ]);
  });

  test("reconciliation is read-model only: inputs are not mutated", () => {
    const queue = hullUpgradesQueue();
    const snapshot = JSON.parse(JSON.stringify(queue)) as SkillQueueEntry[];
    deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    expect(queue).toEqual(snapshot);
    expect(queue.every((e) => e.currentTrainedLevel === 2)).toBe(true);
  });
});

describe("deriveSkillQueueSummary — completed history is excluded from the current queue", () => {
  function historyThenActive(): SkillQueueEntry[] {
    return [
      entry({ skillId: 10, skillName: "Skill A", finishedLevel: 5, queuePosition: 0,
        startDate: new Date(NOW - 10 * DAY).toISOString(), finishDate: new Date(NOW - 9 * DAY).toISOString() }),
      entry({ skillId: 11, skillName: "Skill B", finishedLevel: 1, queuePosition: 1,
        startDate: new Date(NOW - 9 * DAY).toISOString(), finishDate: new Date(NOW - 8 * DAY).toISOString() }),
      entry({ skillId: 11, skillName: "Skill B", finishedLevel: 2, queuePosition: 2,
        startDate: new Date(NOW - 8 * DAY).toISOString(), finishDate: new Date(NOW - 7 * DAY).toISOString() }),
      entry({ skillId: 12, skillName: "Skill C", finishedLevel: 4, queuePosition: 3,
        currentTrainedLevel: 3,
        startDate: new Date(NOW - 1 * HOUR).toISOString(), finishDate: new Date(NOW + 5 * HOUR).toISOString(),
        trainingStartSp: 100_000, levelStartSp: 100_000, levelEndSp: 400_000 }),
      entry({ skillId: 13, skillName: "Skill D", finishedLevel: 5, queuePosition: 4,
        currentTrainedLevel: 4,
        startDate: new Date(NOW + 5 * HOUR).toISOString(), finishDate: new Date(NOW + 30 * HOUR).toISOString(),
        trainingStartSp: 200_000, levelStartSp: 200_000, levelEndSp: 1_000_000 }),
    ];
  }

  test("only the active + future queued rows are visible; count is 2; currentRow is C IV", () => {
    const summary = deriveSkillQueueSummary(input({ trainingQueue: historyThenActive() }), NOW);
    expect(summary.rows.map((r) => [r.entry.skillName, r.entry.finishedLevel, r.status])).toEqual([
      ["Skill C", 4, "training"],
      ["Skill D", 5, "queued"],
    ]);
    expect(summary.queueCount).toBe(2);
    expect(summary.status).toBe("trainingActive");
  });

  test("queueCount / totalRemainingMs / spRemaining count unfinished work only", () => {
    const summary = deriveSkillQueueSummary(input({ trainingQueue: historyThenActive() }), NOW);
    expect(summary.queueCount).toBe(2);
    // wall-clock to the last future entry's finish (NOW + 30h), not the sum
    // of the historical spans.
    expect(summary.totalRemainingMs).toBe(30 * HOUR);
    expect(summary.totalRemainingKind).toBe("wallClock");
    // C IV: 300k level, started at its levelStart, ~1/6 through -> ~250k left;
    // D V: full 800k delta. History contributes nothing.
    const cRemaining = Math.round((1 - 1 / 6) * 300_000);
    expect(summary.spRemaining).toBe(cRemaining + 800_000);
  });

  test("all-completed / cache-expired queue: status queueStale, empty rows, zeroed totals", () => {
    const queue = [
      entry({ skillId: 20, finishedLevel: 4, queuePosition: 0,
        startDate: new Date(NOW - 5 * DAY).toISOString(), finishDate: new Date(NOW - 4 * DAY).toISOString() }),
      entry({ skillId: 20, finishedLevel: 5, queuePosition: 1,
        startDate: new Date(NOW - 4 * DAY).toISOString(), finishDate: new Date(NOW - 3 * DAY).toISOString() }),
    ];
    const summary = deriveSkillQueueSummary(input({ trainingQueue: queue }), NOW);
    expect(summary.status).toBe("queueStale");
    expect(summary.rows).toEqual([]);
    expect(summary.queueCount).toBe(0);
    expect(summary.totalRemainingMs).toBe(0);
    expect(summary.spRemaining).toBe(0);
  });
});

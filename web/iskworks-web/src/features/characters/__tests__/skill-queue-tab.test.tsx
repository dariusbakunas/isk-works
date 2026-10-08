import { render, screen, within } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import type { CharacterDetail, CharacterSourceDetail, SkillQueueEntry } from "../../../api/characters";
import { CharacterSkillQueueTab } from "../skill-queue-tab";

const NOW = Date.parse("2026-09-09T12:00:00Z");
const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

function skill(overrides: Partial<SkillQueueEntry> = {}): SkillQueueEntry {
  return {
    skillId: 100,
    skillName: "Missile Projection",
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

function source(overrides: Partial<CharacterSourceDetail> = {}): CharacterSourceDetail {
  return {
    sourceKind: "skills",
    refreshState: "current",
    observedAt: new Date(NOW - 5 * MIN).toISOString(),
    nextRefreshAt: null,
    lastError: null,
    ...overrides,
  };
}

function detail(overrides: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    connectionId: "conn-1",
    eveCharacterId: 1,
    characterName: "Aeva Stark",
    corporationId: null,
    corporationName: null,
    securityStatus: null,
    solarSystemId: null,
    solarSystemName: null,
    walletBalance: null,
    totalSp: 94_300_000,
    unallocatedSp: 0,
    trainingQueue: [],
    trainingObservedAt: new Date(NOW - 5 * MIN).toISOString(),
    trainingQueueScopeMissing: false,
    manufacturingActiveJobs: null,
    manufacturingMaxJobs: null,
    reactionActiveJobs: null,
    reactionMaxJobs: null,
    researchActiveJobs: null,
    researchMaxJobs: null,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: new Date(NOW - 5 * MIN).toISOString(),
    industryJobs: [],
    sources: [source()],
    ...overrides,
  };
}

function activeQueue(): SkillQueueEntry[] {
  return [
    skill({
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
    skill({
      skillId: 101,
      skillName: "Missile Projection",
      finishedLevel: 3,
      queuePosition: 1,
      startDate: new Date(NOW + 30 * MIN).toISOString(),
      finishDate: new Date(NOW + 30 * MIN + 7 * HOUR).toISOString(),
      trainingStartSp: 100_000,
      levelStartSp: 100_000,
      levelEndSp: 400_000,
      currentTrainedLevel: null,
    }),
  ];
}

describe("CharacterSkillQueueTab", () => {
  test("active queue: header count, TRAINING row, queued row, and footer totals", () => {
    render(<CharacterSkillQueueTab detail={detail({ trainingQueue: activeQueue() })} nowMs={NOW} />);

    expect(screen.getByText("2 / 150 queued")).toBeInTheDocument();
    expect(screen.getByText("Training active")).toBeInTheDocument();
    expect(screen.getByText(/94\.3m/)).toBeInTheDocument();

    expect(screen.getByText("Training")).toBeInTheDocument();
    expect(screen.getByText(/Target Navigation Prediction V/)).toBeInTheDocument();
    expect(screen.getByText("30m 0s")).toBeInTheDocument(); // current: endDate - now

    expect(screen.getByText(/Missile Projection III/)).toBeInTheDocument();
    expect(screen.getByText("7h 0m")).toBeInTheDocument(); // queued: endDate - startDate
    expect(screen.getByText("Queued")).toBeInTheDocument();

    expect(screen.getByText("Queue remaining")).toBeInTheDocument();
    expect(screen.getByText("SP remaining")).toBeInTheDocument();
  });

  test("missing skill-queue scope shows the re-authorization state, not an empty queue", () => {
    render(
      <CharacterSkillQueueTab
        detail={detail({ trainingQueue: activeQueue(), trainingQueueScopeMissing: true })}
        nowMs={NOW}
      />,
    );
    expect(screen.getByText("Skill queue needs re-authorization")).toBeInTheDocument();
    expect(screen.getByText("esi-skills.read_skillqueue.v1")).toBeInTheDocument();
    expect(screen.queryByText("No skills queued.")).not.toBeInTheDocument();
    // total SP is still shown
    expect(screen.getByText(/94\.3m/)).toBeInTheDocument();
  });

  test("paused queue is labelled paused and the first row is not TRAINING", () => {
    const paused = [skill({ queuePosition: 0 }), skill({ queuePosition: 1, skillId: 2 })];
    render(<CharacterSkillQueueTab detail={detail({ trainingQueue: paused })} nowMs={NOW} />);
    expect(screen.getByText("Queue paused")).toBeInTheDocument();
    expect(screen.queryByText("Training")).not.toBeInTheDocument();
    expect(screen.getByText("Queue duration (paused)")).toBeInTheDocument();
  });

  test("empty queue: 0 / 150, no skills queued, total SP still shown", () => {
    render(<CharacterSkillQueueTab detail={detail({ trainingQueue: [] })} nowMs={NOW} />);
    expect(screen.getByText("0 / 150 queued")).toBeInTheDocument();
    expect(screen.getByText("No skills queued.")).toBeInTheDocument();
    expect(screen.getByText(/94\.3m/)).toBeInTheDocument();
  });

  test("never synced is distinct from empty", () => {
    render(
      <CharacterSkillQueueTab
        detail={detail({
          trainingObservedAt: null,
          sources: [source({ refreshState: "missing", observedAt: null })],
        })}
        nowMs={NOW}
      />,
    );
    expect(screen.getByText("Not synced yet.")).toBeInTheDocument();
    expect(screen.queryByText("No skills queued.")).not.toBeInTheDocument();
  });

  test("sync failure is distinct from paused/empty", () => {
    render(
      <CharacterSkillQueueTab
        detail={detail({ sources: [source({ refreshState: "failed", lastError: "ESI 500" })] })}
        nowMs={NOW}
      />,
    );
    expect(screen.getByText(/Skills failed to sync: ESI 500/)).toBeInTheDocument();
  });

  test("level indicators reflect current trained state + this entry's target, per row", () => {
    function rowCellStates(nameMatcher: RegExp): (string | null)[] {
      const nameEl = screen.getByText(nameMatcher);
      const row = nameEl.closest("li") ?? nameEl.closest("div");
      return Array.from(row!.querySelectorAll("[data-level-state]")).map((el) =>
        el.getAttribute("data-level-state"),
      );
    }

    const queue: SkillQueueEntry[] = [
      // active: Supply Chain Management III, character has II, training into III
      skill({
        skillId: 24_270,
        skillName: "Supply Chain Management",
        finishedLevel: 3,
        queuePosition: 0,
        currentTrainedLevel: 2,
        startDate: new Date(NOW - HOUR).toISOString(),
        finishDate: new Date(NOW + HOUR).toISOString(),
        trainingStartSp: 8_000,
        levelStartSp: 8_000,
        levelEndSp: 45_255,
      }),
      // queued: Supply Chain Management IV — still only II trained now
      skill({
        skillId: 24_270,
        skillName: "Supply Chain Management",
        finishedLevel: 4,
        queuePosition: 1,
        currentTrainedLevel: 2,
        startDate: new Date(NOW + HOUR).toISOString(),
        finishDate: new Date(NOW + 6 * HOUR).toISOString(),
        trainingStartSp: 45_255,
        levelStartSp: 45_255,
        levelEndSp: 256_000,
      }),
      // queued: Caldari Drone Specialization I — nothing trained
      skill({
        skillId: 23_615,
        skillName: "Caldari Drone Specialization",
        finishedLevel: 1,
        queuePosition: 2,
        currentTrainedLevel: 0,
        startDate: new Date(NOW + 6 * HOUR).toISOString(),
        finishDate: new Date(NOW + 7 * HOUR).toISOString(),
        trainingStartSp: 0,
        levelStartSp: 0,
        levelEndSp: 2_500,
      }),
      // queued: Caldari Drone Specialization V — still nothing trained
      skill({
        skillId: 23_615,
        skillName: "Caldari Drone Specialization",
        finishedLevel: 5,
        queuePosition: 3,
        currentTrainedLevel: 0,
        startDate: new Date(NOW + 7 * HOUR).toISOString(),
        finishDate: new Date(NOW + 40 * HOUR).toISOString(),
        trainingStartSp: 40_000,
        levelStartSp: 40_000,
        levelEndSp: 1_280_000,
      }),
    ];
    render(<CharacterSkillQueueTab detail={detail({ trainingQueue: queue })} nowMs={NOW} />);

    // active row: I–II trained, III being trained now, IV–V beyond
    expect(rowCellStates(/Supply Chain Management III/)).toEqual([
      "trained",
      "trained",
      "training",
      "empty",
      "empty",
    ]);
    // queued row for IV, SAME skill that is training now: III is NOT shown
    // as trained (it isn't finished), but it IS shown as 'training' because
    // that level is being trained right this moment; IV is a plain queued
    // target.
    expect(rowCellStates(/Supply Chain Management IV/)).toEqual([
      "trained",
      "trained",
      "training",
      "queued",
      "empty",
    ]);
    expect(rowCellStates(/Caldari Drone Specialization I/)).toEqual([
      "queued",
      "empty",
      "empty",
      "empty",
      "empty",
    ]);
    // V targets all five, none trained yet, no simulation of the level-I entry
    expect(rowCellStates(/Caldari Drone Specialization V/)).toEqual([
      "queued",
      "queued",
      "queued",
      "queued",
      "queued",
    ]);
  });

  test("renders an 80-entry queue with every row present and in queue order", () => {
    const queue: SkillQueueEntry[] = [
      skill({
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
    ];
    let cursor = NOW + 50 * MIN;
    for (let i = 1; i < 80; i += 1) {
      const start = cursor;
      cursor += HOUR;
      queue.push(
        skill({
          skillId: 2000 + i,
          skillName: `Queued Skill ${i}`,
          finishedLevel: ((i % 5) + 1),
          queuePosition: i,
          startDate: new Date(start).toISOString(),
          finishDate: new Date(cursor).toISOString(),
        }),
      );
    }
    render(<CharacterSkillQueueTab detail={detail({ trainingQueue: queue })} nowMs={NOW} />);

    expect(screen.getByText("80 / 150 queued")).toBeInTheDocument();
    const list = screen.getByRole("list");
    // 79 non-training rows in the list + the current row rendered above it
    expect(within(list).getAllByRole("listitem")).toHaveLength(79);
    expect(screen.getByText(/Head Skill V/)).toBeInTheDocument();
    expect(screen.getByText(/Queued Skill 79 /)).toBeInTheDocument();
  });
});

describe("CharacterSkillQueueTab — completed history reconciliation", () => {
  function rowCellStates(nameMatcher: RegExp): (string | null)[] {
    const nameEl = screen.getByText(nameMatcher);
    const row = nameEl.closest("li") ?? nameEl.closest("div");
    return Array.from(row!.querySelectorAll("[data-level-state]")).map((el) => el.getAttribute("data-level-state"));
  }

  test("stale trained II + completed III + active IV + future V: no COMPLETED row, count excludes III, level III trained", () => {
    const queue: SkillQueueEntry[] = [
      skill({
        skillId: 3392,
        skillName: "Hull Upgrades",
        finishedLevel: 3,
        queuePosition: 0,
        currentTrainedLevel: 2,
        startDate: new Date(NOW - 3 * HOUR).toISOString(),
        finishDate: new Date(NOW - 2 * HOUR).toISOString(),
      }),
      skill({
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
      skill({
        skillId: 3392,
        skillName: "Hull Upgrades",
        finishedLevel: 5,
        queuePosition: 2,
        currentTrainedLevel: 2,
        startDate: new Date(NOW + 20 * HOUR).toISOString(),
        finishDate: new Date(NOW + 6 * DAY).toISOString(),
      }),
    ];
    render(<CharacterSkillQueueTab detail={detail({ trainingQueue: queue })} nowMs={NOW} />);

    expect(screen.queryByText("Completed")).not.toBeInTheDocument();
    // 2 unfinished entries: IV active + V queued
    expect(screen.getByText("2 / 150 queued")).toBeInTheDocument();

    // active Hull Upgrades IV: I-III trained, IV training, V empty
    expect(rowCellStates(/Hull Upgrades IV/)).toEqual(["trained", "trained", "trained", "training", "empty"]);
    // future Hull Upgrades V still queued normally at its own target level
    expect(rowCellStates(/Hull Upgrades V/)).toEqual(["trained", "trained", "trained", "training", "queued"]);
  });

  test("all-completed / cache-expired queue shows a calm refreshing state, not an empty paused shell", () => {
    const queue: SkillQueueEntry[] = [
      skill({
        skillId: 20,
        skillName: "Some Skill",
        finishedLevel: 5,
        queuePosition: 0,
        startDate: new Date(NOW - 5 * DAY).toISOString(),
        finishDate: new Date(NOW - 4 * DAY).toISOString(),
      }),
    ];
    render(<CharacterSkillQueueTab detail={detail({ trainingQueue: queue })} nowMs={NOW} />);

    expect(screen.getByText("No active training — refreshing…")).toBeInTheDocument();
    expect(screen.getByText("0 / 150 queued")).toBeInTheDocument();
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    expect(screen.queryByText("Completed")).not.toBeInTheDocument();
    // still calm, not an error colour
    expect(screen.getByText("No active training — refreshing…").className).toContain("text-muted");
  });
});

import { act, render, screen } from "@testing-library/react";
import { Profiler, type ProfilerOnRenderCallback } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { SkillQueueEntry } from "../../../api/characters";
import { CharacterCard } from "../character-card";
import { LiveTrainingProgress } from "../live-training-progress";
import type { CharacterRosterEntry } from "../../../api/characters";

const HOUR = 60 * 60 * 1000;

function entry(
  queuePosition: number,
  startDate: string | null,
  finishDate: string | null,
  overrides: Partial<SkillQueueEntry> = {},
): SkillQueueEntry {
  return {
    skillId: 1000 + queuePosition,
    skillName: `Skill ${queuePosition}`,
    finishedLevel: 5,
    queuePosition,
    startDate,
    finishDate,
    trainingStartSp: null,
    levelStartSp: null,
    levelEndSp: null,
    currentTrainedLevel: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("LiveTrainingProgress", () => {
  test("ticks the remaining-time label and progress bar down toward zero", () => {
    const now = Date.now();
    const active = entry(0, new Date(now - HOUR).toISOString(), new Date(now + 90_000).toISOString());
    render(<LiveTrainingProgress queue={[active]} observedAt={null} onNeedsRefresh={vi.fn()} variant="card" />);

    expect(screen.getByText(/1m 30s/)).toBeInTheDocument();
    const bar = screen.getByTestId("training-progress-bar");
    const widthBefore = bar.style.width;

    act(() => vi.advanceTimersByTime(78_000));

    expect(screen.getByText(/12s/)).toBeInTheDocument();
    expect(bar.style.width).not.toBe(widthBefore);
  });

  test("advances instantly to the next cached entry with no intermediate refreshing flash", () => {
    const now = Date.now();
    const a = entry(0, new Date(now - HOUR).toISOString(), new Date(now + 2_000).toISOString());
    const b = entry(1, new Date(now + 2_000).toISOString(), new Date(now + HOUR).toISOString());
    render(<LiveTrainingProgress queue={[a, b]} observedAt={null} onNeedsRefresh={vi.fn()} variant="card" />);

    act(() => vi.advanceTimersByTime(3_000));

    expect(screen.getByText(/Skill 1/)).toBeInTheDocument();
    expect(screen.queryByText(/Refreshing training state/)).not.toBeInTheDocument();
  });

  test("renders Paused for a uniformly-undated queue", () => {
    const paused = entry(0, null, null);
    render(<LiveTrainingProgress queue={[paused]} observedAt={null} onNeedsRefresh={vi.fn()} variant="card" />);

    expect(screen.getByText(/Paused/)).toBeInTheDocument();
  });

  test("renders No skill queue for an empty queue", () => {
    render(<LiveTrainingProgress queue={[]} observedAt={null} onNeedsRefresh={vi.fn()} variant="card" />);

    expect(screen.getByText("No skill queue")).toBeInTheDocument();
  });

  test("renders Refreshing training state only for cachedQueueExpired, not for paused/empty", () => {
    const now = Date.now();
    const stale = entry(0, new Date(now - 2 * HOUR).toISOString(), new Date(now - HOUR).toISOString());
    render(<LiveTrainingProgress queue={[stale]} observedAt={null} onNeedsRefresh={vi.fn()} variant="card" />);

    expect(screen.getByText("Refreshing training state…")).toBeInTheDocument();
  });

  test("a non-training card does not re-render while a sibling training card ticks", () => {
    const now = Date.now();
    const training = characterEntryFixture({
      connectionId: "conn-training",
      trainingQueue: [entry(0, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString())],
    });
    const idle = characterEntryFixture({ connectionId: "conn-idle", trainingQueue: [] });

    const idleRenderCount = { current: 0 };
    const onIdleRender: ProfilerOnRenderCallback = () => {
      idleRenderCount.current += 1;
    };

    render(
      <>
        <CharacterCard entry={training} />
        <Profiler id="idle-card" onRender={onIdleRender}>
          <CharacterCard entry={idle} />
        </Profiler>
      </>,
    );

    const countAfterMount = idleRenderCount.current;
    act(() => vi.advanceTimersByTime(5_000));

    expect(idleRenderCount.current).toBe(countAfterMount);
  });
});

function characterEntryFixture(overrides: Partial<CharacterRosterEntry> = {}): CharacterRosterEntry {
  return {
    connectionId: "conn-1",
    eveCharacterId: 2_119_000_001,
    characterName: "Aeva Stark",
    corporationId: 98_000_001,
    corporationName: "Perimeter Industrial Holdings",
    securityStatus: "5.00000",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    walletBalance: "4820000000.0000",
    totalSp: 94_300_000,
    unallocatedSp: null,
    trainingQueue: [],
    trainingObservedAt: null,
    trainingQueueScopeMissing: false,
    manufacturingActiveJobs: 8,
    manufacturingMaxJobs: 10,
    reactionActiveJobs: 0,
    reactionMaxJobs: 10,
    researchActiveJobs: 2,
    researchMaxJobs: 10,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: new Date(Date.now() - 4 * 60 * 1000).toISOString(),
    ...overrides,
  };
}

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { SkillQueueEntry } from "../../../api/characters";
import { useTrainingCountdown } from "../use-training-countdown";

const HOUR = 60 * 60 * 1000;

function entry(
  queuePosition: number,
  startDate: string | null,
  finishDate: string | null,
  overrides: Partial<SkillQueueEntry> = {},
): SkillQueueEntry {
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
    ...overrides,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("useTrainingCountdown", () => {
  test("remaining time decreases as time advances, with no fetch involved", () => {
    const now = Date.now();
    const active = entry(0, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString());
    const onNeedsRefresh = vi.fn();
    const { result } = renderHook(() => useTrainingCountdown([active], onNeedsRefresh));

    expect(result.current.kind).toBe("active");
    const initialRemaining = result.current.kind === "active" ? result.current.remainingMs : -1;

    act(() => vi.advanceTimersByTime(30_000));

    expect(result.current.kind).toBe("active");
    const laterRemaining = result.current.kind === "active" ? result.current.remainingMs : -1;
    expect(laterRemaining).toBeLessThan(initialRemaining);
  });

  test("progress fraction increases as time advances", () => {
    const now = Date.now();
    const active = entry(0, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString());
    const { result } = renderHook(() => useTrainingCountdown([active], vi.fn()));

    const initialFraction = result.current.kind === "active" ? result.current.fraction : -1;
    act(() => vi.advanceTimersByTime(30_000));
    const laterFraction = result.current.kind === "active" ? result.current.fraction : -1;

    expect(laterFraction).toBeGreaterThan(initialFraction);
  });

  test("fraction clamps to [0,1] and remainingMs never goes negative near the finish boundary", () => {
    const now = Date.now();
    const active = entry(0, new Date(now - HOUR).toISOString(), new Date(now + 2_000).toISOString());
    const { result } = renderHook(() => useTrainingCountdown([active], vi.fn()));

    act(() => vi.advanceTimersByTime(5_000));

    if (result.current.kind === "active") {
      expect(result.current.fraction).toBeLessThanOrEqual(1);
      expect(result.current.fraction).toBeGreaterThanOrEqual(0);
      expect(result.current.remainingMs).toBeGreaterThanOrEqual(0);
    }
  });

  test("stops showing an expired entry as active once finishDate passes with no next entry cached", () => {
    const now = Date.now();
    const active = entry(0, new Date(now - HOUR).toISOString(), new Date(now + 2_000).toISOString());
    const onNeedsRefresh = vi.fn();
    const { result } = renderHook(() => useTrainingCountdown([active], onNeedsRefresh));

    act(() => vi.advanceTimersByTime(3_000));

    expect(result.current.kind).toBe("cachedQueueExpired");
    expect(onNeedsRefresh).toHaveBeenCalledTimes(1);
  });

  test("advances instantly to a cached next entry with no intermediate state, and reconciles once", () => {
    const now = Date.now();
    const a = entry(0, new Date(now - HOUR).toISOString(), new Date(now + 2_000).toISOString());
    const b = entry(1, new Date(now + 2_000).toISOString(), new Date(now + HOUR).toISOString());
    const onNeedsRefresh = vi.fn();
    const { result } = renderHook(() => useTrainingCountdown([a, b], onNeedsRefresh));

    act(() => vi.advanceTimersByTime(3_000));

    expect(result.current.kind).toBe("active");
    if (result.current.kind === "active") {
      expect(result.current.entry.skillId).toBe(b.skillId);
    }
    expect(onNeedsRefresh).toHaveBeenCalledTimes(1);
  });

  test("skips multiple already-completed entries in one evaluation as a single crossing", () => {
    const now = Date.now();
    const a = entry(0, new Date(now - HOUR).toISOString(), new Date(now + 1_000).toISOString());
    const b = entry(1, new Date(now + 1_000).toISOString(), new Date(now + 1_500).toISOString());
    const c = entry(2, new Date(now + 1_500).toISOString(), new Date(now + HOUR).toISOString());
    const onNeedsRefresh = vi.fn();
    const { result } = renderHook(() => useTrainingCountdown([a, b, c], onNeedsRefresh));

    act(() => vi.advanceTimersByTime(2_000));

    expect(result.current.kind).toBe("active");
    if (result.current.kind === "active") {
      expect(result.current.entry.skillId).toBe(c.skillId);
    }
    expect(onNeedsRefresh).toHaveBeenCalledTimes(1);
  });

  test("empty queue starts no interval and does not call onNeedsRefresh on mount", () => {
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const onNeedsRefresh = vi.fn();
    const { result } = renderHook(() => useTrainingCountdown([], onNeedsRefresh));

    expect(result.current).toEqual({ kind: "empty" });
    expect(setIntervalSpy).not.toHaveBeenCalled();
    expect(onNeedsRefresh).not.toHaveBeenCalled();
  });

  test("paused queue starts no interval and does not call onNeedsRefresh on mount", () => {
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const onNeedsRefresh = vi.fn();
    const paused = entry(0, null, null);
    const { result } = renderHook(() => useTrainingCountdown([paused], onNeedsRefresh));

    expect(result.current).toEqual({ kind: "paused", next: paused });
    expect(setIntervalSpy).not.toHaveBeenCalled();
    expect(onNeedsRefresh).not.toHaveBeenCalled();
  });

  test("mounting directly into an already-expired cached queue reconciles once", () => {
    const now = Date.now();
    const stale = entry(0, new Date(now - 2 * HOUR).toISOString(), new Date(now - HOUR).toISOString());
    const onNeedsRefresh = vi.fn();
    const { result } = renderHook(() => useTrainingCountdown([stale], onNeedsRefresh));

    expect(result.current.kind).toBe("cachedQueueExpired");
    expect(onNeedsRefresh).toHaveBeenCalledTimes(1);
  });

  test("does not call onNeedsRefresh again while sitting in the same active entry across ticks", () => {
    const now = Date.now();
    const active = entry(0, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString());
    const onNeedsRefresh = vi.fn();
    renderHook(() => useTrainingCountdown([active], onNeedsRefresh));

    act(() => vi.advanceTimersByTime(1_000));
    act(() => vi.advanceTimersByTime(1_000));
    act(() => vi.advanceTimersByTime(1_000));

    expect(onNeedsRefresh).not.toHaveBeenCalled();
  });

  test("resamples now when entries change after mounting stale, instead of misreading an already-active entry as not-yet-started", () => {
    // Regression: the hook mounted with an empty queue (before the initial
    // fetch resolved), so `now` was captured then and never advanced
    // (nothing ticks while `empty`). An hour passes in the real world,
    // then fresh entries land whose startDate is anchored to *that* later
    // moment -- the hook must not judge them against the stale mount-time
    // `now`, which would make an already-active entry look like it hasn't
    // started yet.
    const onNeedsRefresh = vi.fn();
    const { result, rerender } = renderHook(({ entries }) => useTrainingCountdown(entries, onNeedsRefresh), {
      initialProps: { entries: [] as SkillQueueEntry[] },
    });
    expect(result.current.kind).toBe("empty");

    act(() => vi.advanceTimersByTime(HOUR));

    const now = Date.now();
    const active = entry(0, new Date(now - 60_000).toISOString(), new Date(now + HOUR).toISOString());
    rerender({ entries: [active] });

    expect(result.current.kind).toBe("active");
  });

  test("clears the interval on unmount", () => {
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const clearIntervalSpy = vi.spyOn(window, "clearInterval");
    const now = Date.now();
    const active = entry(0, new Date(now - HOUR).toISOString(), new Date(now + HOUR).toISOString());
    const { unmount } = renderHook(() => useTrainingCountdown([active], vi.fn()));

    expect(setIntervalSpy).toHaveBeenCalledTimes(1);
    unmount();

    expect(clearIntervalSpy).toHaveBeenCalled();
  });
});

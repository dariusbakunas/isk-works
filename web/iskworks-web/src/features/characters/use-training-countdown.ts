import { useEffect, useRef, useState } from "react";

import type { SkillQueueEntry } from "../../api/characters";
import { deriveTrainingState, type TrainingState } from "./training-state";

// A stable identity for a derived state, used only to edge-trigger
// onNeedsRefresh -- fires once per *change*, never once per tick while
// parked in the same state.
function stateKey(state: TrainingState): string {
  switch (state.kind) {
    case "active":
      return `active:${state.entry.skillId}:${state.entry.finishedLevel}`;
    case "paused":
      return `paused:${state.next?.skillId ?? "none"}`;
    case "cachedQueueExpired":
      return "cachedQueueExpired";
    case "empty":
      return "empty";
  }
}

// Ticks a local `now` once a second, but only while the derived state is
// "active" -- no timer at all for paused/empty/expired queues, and the
// timer starts/stops itself as that changes. Crossing a completion
// boundary (the active entry's finishDate passes) updates the returned
// state immediately from whatever `entries` already has cached -- no
// loading state -- while `onNeedsRefresh` fires exactly once per crossing
// so the caller can reconcile against authoritative data in the
// background. `trainingObservedAt`/staleness deliberately stay out of this
// hook -- they don't change which entry is current, so they belong in the
// presentation layer instead.
export function useTrainingCountdown(entries: SkillQueueEntry[], onNeedsRefresh: () => void): TrainingState {
  const [now, setNow] = useState(() => Date.now());
  const state = deriveTrainingState(entries, now);

  // `now` otherwise only advances via the ticking interval below, which
  // only runs while `state.kind === "active"`. Without this, a component
  // that first mounted (or last ticked) well before `entries` changed --
  // e.g. the initial fetch resolving after mount, or a targeted refetch
  // landing while parked in a non-active state -- would derive against a
  // stale `now` from mount time, which can easily predate a queue entry's
  // own `startDate`, misreading an already-active entry as "not started
  // yet". Resampling on every `entries` change keeps derivation anchored
  // to the current instant whenever new data arrives.
  useEffect(() => {
    setNow(Date.now());
  }, [entries]);

  useEffect(() => {
    if (state.kind !== "active") return undefined;
    const interval = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(interval);
    // `entries` is listed so fresh queue data restarts the tick.
  }, [state.kind, entries]);

  const key = stateKey(state);
  const previousKeyRef = useRef<string | null>(null);
  const onNeedsRefreshRef = useRef(onNeedsRefresh);
  onNeedsRefreshRef.current = onNeedsRefresh;

  useEffect(() => {
    const isFirstEvaluation = previousKeyRef.current === null;
    if (!isFirstEvaluation && key !== previousKeyRef.current) {
      onNeedsRefreshRef.current();
    } else if (isFirstEvaluation && state.kind === "cachedQueueExpired") {
      onNeedsRefreshRef.current();
    }
    previousKeyRef.current = key;
    // Fires on `key` transitions only; `state.kind` matters just for the first evaluation and the callback is read through a ref.
  }, [key]);

  return state;
}

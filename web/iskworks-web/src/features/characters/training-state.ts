import type { SkillQueueEntry } from "../../api/characters";

// TS mirror of iskworks_core::training::{SkillQueueEntry, TrainingState,
// derive_training_state} (crates/iskworks-core/src/training.rs). Kept as
// an independent implementation (Rust and TS can't share code) but tested
// against the same edge-case table.
//
// `now` is deliberately not read from a clock inside this module -- it's
// always passed in, so this stays a pure function callers can tick from a
// timer, and staleness (trainingObservedAt) never enters into it: it's a
// presentation/reconciliation concern, not part of "what's current."
export type TrainingState =
  | { kind: "active"; entry: SkillQueueEntry; fraction: number; remainingMs: number }
  | { kind: "paused"; next: SkillQueueEntry | null }
  | { kind: "cachedQueueExpired"; knownUntil: string }
  | { kind: "empty" };

export function deriveTrainingState(entries: SkillQueueEntry[], now: number): TrainingState {
  if (entries.length === 0) {
    return { kind: "empty" };
  }

  if (entries.every((entry) => entry.startDate === null && entry.finishDate === null)) {
    return { kind: "paused", next: entries[0] ?? null };
  }

  const usable = entries.filter(
    (entry): entry is SkillQueueEntry & { startDate: string; finishDate: string } =>
      entry.startDate !== null && entry.finishDate !== null && Date.parse(entry.finishDate) > Date.parse(entry.startDate),
  );

  if (usable.length === 0) {
    return { kind: "empty" };
  }

  for (const entry of usable) {
    const finish = Date.parse(entry.finishDate);
    if (finish <= now) continue;
    const start = Date.parse(entry.startDate);
    if (start <= now) {
      const fraction = Math.min(1, Math.max(0, (now - start) / (finish - start)));
      return { kind: "active", entry, fraction, remainingMs: Math.max(0, finish - now) };
    }
    return { kind: "paused", next: entry };
  }

  const knownUntil = usable.reduce(
    (latest, entry) => (Date.parse(entry.finishDate) > Date.parse(latest) ? entry.finishDate : latest),
    usable[0].finishDate,
  );
  return { kind: "cachedQueueExpired", knownUntil };
}

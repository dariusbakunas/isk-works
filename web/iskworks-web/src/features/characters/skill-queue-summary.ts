import type { SkillQueueEntry, SourceRefreshState } from "../../api/characters";
import { deriveTrainingState } from "./training-state";

// Maximum entries EVE allows in a skill queue. ESI does not report this, so
// it is mirrored from the authoritative Rust helper
// (`iskworks_core::training::SKILL_QUEUE_CAPACITY`) -- CCP raised it from 50
// to 150 for Alpha and Omega in the Aug 2021 "Updates To Skill Training"
// patch: https://www.eveonline.com/news/view/updates-to-skill-training
export const SKILL_QUEUE_CAPACITY = 150;

export type SkillQueueStatus =
  | "trainingActive"
  | "queuePaused"
  // Raw queue has entries but every one has already finished (our cached
  // projection ran out). No currently-actionable entry to show; the real
  // queue may have been extended since the last sync.
  | "queueStale"
  | "queueEmpty"
  | "notSynced"
  | "syncFailed"
  | "reauthRequired";

export type SkillQueueRowStatus = "training" | "queued" | "completed" | "paused";

// State of one of the five skill-level cells shown per queue row:
//  - "trained":  the character already holds this level (ESI trained_skill_level)
//  - "queued":   this level is part of THIS entry's target, not yet trained
//  - "training": the frontier level of the row that is training right now
//  - "partial":  the frontier level of a *queued* row that already has SP
//                accrued into it (static diagonal, no animation)
//  - "empty":    above this entry's target level
export type LevelCellState = "trained" | "queued" | "training" | "partial" | "empty";

// Pure per-row derivation of the five level cells. Uses only authoritative
// synced state -- the character's current trained level and this entry's own
// ESI SP boundaries -- never a simulation of earlier queue entries
// completing. Successive levels of one skill therefore each show the
// character's *current* trained levels plus that entry's target, not a
// running projection.
export function skillLevelCells(params: {
  targetLevel: number;
  /** The character's effective trained level for this skill: the persisted
   *  ESI `trained_skill_level` snapshot, reconciled by the caller
   *  (`deriveSkillQueueSummary`) against authoritative completed/active
   *  queue evidence. `null` (skills array absent) is treated as 0. */
  currentTrainedLevel: number | null;
  /** The level of *this row's skill* that is training right now, or null
   *  when this skill is not the one currently in training. It is the same
   *  value for every queue row of that skill -- so a later "Skill IV" row
   *  still shows level III mid-training as animated while III is being
   *  trained, not as a plain queued cell. */
  trainingLevel: number | null;
  /** The entry already had SP inside its target level when queued
   *  (trainingStartSp > levelStartSp). */
  hasPriorPartialSp: boolean;
}): LevelCellState[] {
  const trained = Math.max(0, params.currentTrainedLevel ?? 0);
  const { targetLevel, trainingLevel, hasPriorPartialSp } = params;
  return [1, 2, 3, 4, 5].map((level) => {
    if (level <= trained) return "trained";
    if (level > targetLevel) return "empty";
    // The level being trained right now animates in every row of that
    // skill, even one whose own target is a higher level.
    if (level === trainingLevel) return "training";
    if (level < targetLevel) return "queued";
    // level === targetLevel, above the current trained level: the frontier.
    const isFrontier = level === trained + 1;
    return isFrontier && hasPriorPartialSp ? "partial" : "queued";
  });
}

export interface SkillQueueRowView {
  entry: SkillQueueEntry;
  /** The level this entry finishes at (ESI `finished_level`). */
  targetLevel: number;
  /** The five skill-level indicator cells, level I..V. */
  levelCells: LevelCellState[];
  status: SkillQueueRowStatus;
  /** Within-level progress `[0,1]`, only for the currently training row. */
  fraction: number | null;
  /**
   * training row -> time left until it finishes (`finishDate - now`).
   * queued row   -> the skill's own training cost (`finishDate - startDate`).
   * completed/paused -> 0.
   */
  durationMs: number;
  durationKind: "remaining" | "nominal" | "done";
}

export interface SkillQueueSummaryView {
  status: SkillQueueStatus;
  /** Unfinished entries only: currently training + future queued + paused.
   *  Historical completed entries (retained in the raw queue for
   *  deriveTrainingState / reconciliation) are not counted. */
  queueCount: number;
  /** EVE's hard queue cap (`SKILL_QUEUE_CAPACITY`). */
  queueCapacity: number;
  totalSp: number | null;
  unallocatedSp: number | null;
  /** SP still to be trained across the current + queued entries. Exact. */
  spRemaining: number;
  /**
   * active queue -> wall-clock time until the last entry finishes.
   * paused/expired queue -> summed nominal per-entry durations (no
   * completion instant is implied, since EVE would not honour one).
   */
  totalRemainingMs: number;
  totalRemainingKind: "wallClock" | "nominal";
  syncedAt: string | null;
  /** Visible rows for the current queue: `status !== "completed"`, in
   *  queue_position order. Completed entries are dropped here only for
   *  presentation — the full raw queue is still used internally for
   *  deriveTrainingState (skip-forward) and effective-trained-level
   *  reconciliation. */
  rows: SkillQueueRowView[];
}

export interface SkillQueueSummaryInput {
  trainingQueue: SkillQueueEntry[];
  trainingObservedAt: string | null;
  totalSp: number | null;
  unallocatedSp: number | null;
  trainingQueueScopeMissing: boolean;
  /** `skills` source `refreshState`, or null if that source is absent. */
  skillsSourceState: SourceRefreshState | null;
  /** `skills` source failed specifically for a missing ESI scope. */
  skillsSourceMissingScope: boolean;
}

function parseMs(iso: string | null): number | null {
  if (iso === null) return null;
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? null : ms;
}

// Full SP cost of a not-yet-started queued level, from whatever bounds ESI
// provided. Prefers the level's own [start, end] span; falls back to
// [trainingStart, end] (equal unless the skill was partially trained into
// this level before being queued); 0 when nothing usable is present.
function queuedLevelSp(entry: SkillQueueEntry): number {
  const { levelStartSp, levelEndSp, trainingStartSp } = entry;
  if (levelStartSp !== null && levelEndSp !== null) return Math.max(0, levelEndSp - levelStartSp);
  if (trainingStartSp !== null && levelEndSp !== null) return Math.max(0, levelEndSp - trainingStartSp);
  return 0;
}

function resolveStatus(
  input: SkillQueueSummaryInput,
  orderedTraining: ReturnType<typeof deriveTrainingState>,
): SkillQueueStatus {
  if (input.trainingQueueScopeMissing || input.skillsSourceMissingScope) return "reauthRequired";
  if (input.skillsSourceState === "failed") return "syncFailed";
  if (input.trainingObservedAt === null) return "notSynced";
  if (input.trainingQueue.length === 0) return "queueEmpty";
  if (orderedTraining.kind === "active") return "trainingActive";
  if (orderedTraining.kind === "cachedQueueExpired") return "queueStale";
  return "queuePaused";
}

// Pure: `now` (epoch ms) is always supplied so callers can tick it from a
// timer. Rows stay in `queue_position` order -- the queue is an ordered
// plan, never re-sorted by name/duration/completion. Mirrors and reuses
// `deriveTrainingState` for the "which entry is current / how far in" call.
export function deriveSkillQueueSummary(
  input: SkillQueueSummaryInput,
  now: number,
): SkillQueueSummaryView {
  const ordered = [...input.trainingQueue].sort((a, b) => a.queuePosition - b.queuePosition);
  const training = deriveTrainingState(ordered, now);
  const status = resolveStatus(input, training);
  const activePosition = training.kind === "active" ? training.entry.queuePosition : null;
  // The skill and level being trained right now -- every queue row for that
  // skill animates that level's cell, not just the active row.
  const activeSkillId = training.kind === "active" ? training.entry.skillId : null;
  const activeSkillTrainingLevel = training.kind === "active" ? training.entry.finishedLevel : null;

  // --- Effective trained level reconciliation (read-model only) -----------
  // The persisted /skills/ snapshot (`currentTrainedLevel`) can lag a cache
  // vintage behind /skillqueue/. Correct it upward -- never downward, never
  // mutating the input -- using only authoritative evidence:
  //   * a completed queue entry for the skill proves that level was reached
  //   * an active entry targeting N proves levels < N are trained (EVE
  //     trains sequentially)
  // A *future queued* entry is never used -- that would simulate training
  // that has not happened yet (see the successive-levels design).
  const completedMaxLevelBySkill = new Map<number, number>();
  for (const entry of ordered) {
    const isActive = activePosition !== null && entry.queuePosition === activePosition;
    const finishMs = parseMs(entry.finishDate);
    if (isActive || finishMs === null || finishMs > now) continue;
    const prev = completedMaxLevelBySkill.get(entry.skillId) ?? 0;
    if (entry.finishedLevel > prev) completedMaxLevelBySkill.set(entry.skillId, entry.finishedLevel);
  }
  const effectiveTrainedLevel = (entry: SkillQueueEntry): number => {
    const fromSnapshot = Math.max(0, entry.currentTrainedLevel ?? 0);
    const fromCompleted = completedMaxLevelBySkill.get(entry.skillId) ?? 0;
    const fromActive =
      activeSkillId !== null && entry.skillId === activeSkillId && activeSkillTrainingLevel !== null
        ? activeSkillTrainingLevel - 1
        : 0;
    return Math.max(fromSnapshot, fromCompleted, fromActive);
  };

  const allRows: SkillQueueRowView[] = ordered.map((entry) => {
    const startMs = parseMs(entry.startDate);
    const finishMs = parseMs(entry.finishDate);
    const isActive = activePosition !== null && entry.queuePosition === activePosition;

    let rowStatus: SkillQueueRowStatus;
    if (isActive) rowStatus = "training";
    else if (finishMs !== null && finishMs <= now) rowStatus = "completed";
    else if (startMs === null && finishMs === null) rowStatus = "paused";
    else rowStatus = "queued";

    let durationMs = 0;
    let durationKind: SkillQueueRowView["durationKind"] = "nominal";
    if (rowStatus === "training" && finishMs !== null) {
      durationMs = Math.max(0, finishMs - now);
      durationKind = "remaining";
    } else if (rowStatus === "queued" && startMs !== null && finishMs !== null) {
      durationMs = Math.max(0, finishMs - startMs);
      durationKind = "nominal";
    } else if (rowStatus === "completed") {
      durationKind = "done";
    }

    const hasPriorPartialSp =
      entry.trainingStartSp !== null &&
      entry.levelStartSp !== null &&
      entry.trainingStartSp > entry.levelStartSp;

    return {
      entry,
      targetLevel: entry.finishedLevel,
      levelCells: skillLevelCells({
        targetLevel: entry.finishedLevel,
        currentTrainedLevel: effectiveTrainedLevel(entry),
        trainingLevel: activeSkillId !== null && entry.skillId === activeSkillId ? activeSkillTrainingLevel : null,
        hasPriorPartialSp,
      }),
      status: rowStatus,
      fraction: isActive && training.kind === "active" ? training.fraction : null,
      durationMs,
      durationKind,
    };
  });

  // The current queue is unfinished work only. Completed entries stay in
  // `allRows`/`ordered` for derivation but never reach the UI.
  const rows = allRows.filter((row) => row.status !== "completed");

  // SP remaining, total time and count all read from the same unfinished
  // row set so their notion of "current queue" cannot drift apart.
  const spRemaining = rows.reduce((sum, row) => {
    if (row.status === "paused") return sum;
    if (row.status === "training") {
      const { trainingStartSp, levelEndSp } = row.entry;
      if (trainingStartSp === null || levelEndSp === null) return sum;
      const delta = Math.max(0, levelEndSp - trainingStartSp);
      const fraction = row.fraction ?? 0;
      return sum + Math.max(0, Math.round((1 - fraction) * delta));
    }
    return sum + queuedLevelSp(row.entry);
  }, 0);

  let totalRemainingMs = 0;
  let totalRemainingKind: SkillQueueSummaryView["totalRemainingKind"] = "nominal";
  if (status === "trainingActive") {
    const latestFinish = rows.reduce((latest, row) => {
      const finishMs = parseMs(row.entry.finishDate);
      return finishMs !== null && finishMs > latest ? finishMs : latest;
    }, now);
    totalRemainingMs = Math.max(0, latestFinish - now);
    totalRemainingKind = "wallClock";
  } else {
    totalRemainingMs = rows.reduce((sum, row) => {
      const startMs = parseMs(row.entry.startDate);
      const finishMs = parseMs(row.entry.finishDate);
      return startMs !== null && finishMs !== null && finishMs > startMs ? sum + (finishMs - startMs) : sum;
    }, 0);
    totalRemainingKind = "nominal";
  }

  return {
    status,
    queueCount: rows.length,
    queueCapacity: SKILL_QUEUE_CAPACITY,
    totalSp: input.totalSp,
    unallocatedSp: input.unallocatedSp,
    spRemaining,
    totalRemainingMs,
    totalRemainingKind,
    syncedAt: input.trainingObservedAt,
    rows,
  };
}

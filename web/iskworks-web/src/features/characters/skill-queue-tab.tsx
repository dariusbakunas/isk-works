import type { CharacterDetail } from "../../api/characters";
import {
  formatRelativeAge,
  formatRemainingMs,
  formatSpCompact,
  missingScopeFrom,
  skillLevelNumeral,
} from "./characters-formatters";
import { SkillLevelBlocks } from "./skill-level-blocks";
import {
  deriveSkillQueueSummary,
  type SkillQueueRowView,
  type SkillQueueStatus,
  type SkillQueueSummaryView,
} from "./skill-queue-summary";

const TRAINING_GREEN = "#4a8f5a";
const SKILL_QUEUE_SCOPE = "esi-skills.read_skillqueue.v1";

const STATUS_BADGE: Record<SkillQueueStatus, { label: string; className: string } | null> = {
  trainingActive: { label: "Training active", className: "border-[#4a8f5a]/50 text-[#5cae6c]" },
  queuePaused: { label: "Queue paused", className: "border-border text-muted" },
  queueStale: { label: "Refreshing", className: "border-border text-muted" },
  queueEmpty: { label: "Queue empty", className: "border-border text-muted" },
  notSynced: { label: "Not synced", className: "border-border text-muted" },
  syncFailed: { label: "Sync failed", className: "border-danger/50 text-danger" },
  reauthRequired: { label: "Reauthorization required", className: "border-warning/50 text-warning" },
};

function exactSp(value: number): string {
  return `${value.toLocaleString()} SP`;
}

function TotalSpLine({ summary }: { summary: SkillQueueSummaryView }) {
  if (summary.totalSp === null) {
    return <span className="text-muted">SP total not synced</span>;
  }
  return (
    <span className="text-muted">
      <span className="font-mono text-foreground" title={exactSp(summary.totalSp)}>
        {formatSpCompact(summary.totalSp)}
      </span>{" "}
      SP total
      {summary.unallocatedSp !== null && summary.unallocatedSp > 0 ? (
        <>
          {" · "}
          <span className="font-mono text-foreground" title={exactSp(summary.unallocatedSp)}>
            {formatSpCompact(summary.unallocatedSp)}
          </span>{" "}
          unallocated
        </>
      ) : null}
    </span>
  );
}

function CurrentTrainingRow({ row }: { row: SkillQueueRowView }) {
  const numeral = skillLevelNumeral(row.targetLevel);
  return (
    <div className="border border-[#4a8f5a]/40 bg-[#4a8f5a]/[0.06] px-2.5 py-2" style={{ borderLeftWidth: 3 }}>
      <div className="flex items-center gap-2">
        <SkillLevelBlocks cells={row.levelCells} target={row.targetLevel} />
        <span className="shrink-0 text-[8px] font-bold uppercase tracking-widest text-[#5cae6c]">Training</span>
        <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-foreground" title={row.entry.skillName ?? undefined}>
          {row.entry.skillName ?? `Type ${row.entry.skillId}`} {numeral}
        </span>
        <span className="shrink-0 font-mono text-[10px] text-foreground">{formatRemainingMs(row.durationMs)}</span>
      </div>
      <div className="mt-1.5 h-[3px] bg-border">
        <div
          className="h-full"
          style={{ width: `${(row.fraction ?? 0) * 100}%`, backgroundColor: TRAINING_GREEN }}
        />
      </div>
    </div>
  );
}

function QueuedRow({ row }: { row: SkillQueueRowView }) {
  const numeral = skillLevelNumeral(row.targetLevel);
  const completed = row.status === "completed";
  const paused = row.status === "paused";
  return (
    <li className="flex items-center gap-2 py-1">
      <SkillLevelBlocks cells={row.levelCells} target={row.targetLevel} />
      <span
        className={`min-w-0 flex-1 truncate text-[10px] ${completed ? "text-muted line-through" : "text-foreground"}`}
        title={row.entry.skillName ?? undefined}
      >
        {row.entry.skillName ?? `Type ${row.entry.skillId}`} {numeral}
      </span>
      <span className="shrink-0 text-[8px] uppercase tracking-wide text-muted">
        {completed ? "Completed" : paused ? "Paused" : "Queued"}
      </span>
      <span className="w-16 shrink-0 text-right font-mono text-[10px] tabular-nums text-muted">
        {completed || paused ? "—" : formatRemainingMs(row.durationMs)}
      </span>
    </li>
  );
}

// The inspector's Skills tab: an authoritative, read-only view of the
// character's real EVE skill queue. Row order is strictly queuePosition
// (an ordered plan). Current-skill remaining time, progress and the queue
// total tick off the parent inspector's `nowMs` clock -- no per-row
// timers, no ESI refetch on tick, no queue editing.
export function CharacterSkillQueueTab({ detail, nowMs }: { detail: CharacterDetail; nowMs: number }) {
  const skillsSource = detail.sources.find((source) => source.sourceKind === "skills") ?? null;
  const summary = deriveSkillQueueSummary(
    {
      trainingQueue: detail.trainingQueue,
      trainingObservedAt: detail.trainingObservedAt,
      totalSp: detail.totalSp,
      unallocatedSp: detail.unallocatedSp,
      trainingQueueScopeMissing: detail.trainingQueueScopeMissing,
      skillsSourceState: skillsSource?.refreshState ?? null,
      skillsSourceMissingScope: missingScopeFrom(skillsSource?.lastError ?? null) !== null,
    },
    nowMs,
  );

  const badge = STATUS_BADGE[summary.status];
  const currentRow = summary.rows.find((row) => row.status === "training") ?? null;
  const otherRows = summary.rows.filter((row) => row.status !== "training");
  const totalLabel = summary.totalRemainingKind === "wallClock" ? "Queue remaining" : "Queue duration (paused)";

  return (
    <div className="space-y-3">
      <div>
        <div className="flex items-baseline justify-between gap-2">
          <div className="flex items-baseline gap-2">
            <span className="text-[11px] font-semibold text-foreground">Skill Queue</span>
            <span className="font-mono text-[10px] text-muted">
              {summary.queueCount} / {summary.queueCapacity} queued
            </span>
          </div>
          {badge ? (
            <span className={`shrink-0 border px-1.5 py-0.5 text-[8px] font-semibold uppercase tracking-wide ${badge.className}`}>
              {badge.label}
            </span>
          ) : null}
        </div>
        <div className="mt-1 text-[10px]">
          <TotalSpLine summary={summary} />
        </div>
      </div>

      {summary.status === "reauthRequired" ? (
        <div className="border border-warning/40 bg-warning/10 p-2 text-[10px] text-warning">
          <p className="font-semibold">Skill queue needs re-authorization</p>
          <p className="mt-0.5 text-warning/80">
            This character hasn’t granted <span className="font-mono">{SKILL_QUEUE_SCOPE}</span>. Use “Reconnect” on the
            Sync tab to add it.
          </p>
        </div>
      ) : summary.status === "syncFailed" ? (
        <p className="border border-danger/40 bg-danger/10 p-2 text-[10px] text-danger">
          Skills failed to sync{skillsSource?.lastError ? `: ${skillsSource.lastError}` : "."}
        </p>
      ) : summary.status === "notSynced" ? (
        <p className="text-[10px] text-muted">Not synced yet.</p>
      ) : summary.status === "queueEmpty" ? (
        <p className="text-[10px] text-muted">No skills queued.</p>
      ) : summary.status === "queueStale" ? (
        <p className="text-[10px] text-muted">No active training — refreshing…</p>
      ) : (
        <>
          {currentRow ? <CurrentTrainingRow row={currentRow} /> : null}
          {otherRows.length > 0 ? (
            <ul className="divide-y divide-border/60 border-y border-border/60">
              {otherRows.map((row) => (
                <QueuedRow key={`${row.entry.queuePosition}:${row.entry.skillId}:${row.targetLevel}`} row={row} />
              ))}
            </ul>
          ) : null}

          <div className="grid grid-cols-2 gap-2 border border-border bg-background px-3 py-2">
            <div>
              <div className="text-[8px] font-semibold uppercase tracking-wider text-muted">{totalLabel}</div>
              <div className="font-mono text-[12px] text-foreground">
                {summary.totalRemainingMs > 0 ? formatRemainingMs(summary.totalRemainingMs) : "—"}
              </div>
            </div>
            <div className="text-right">
              <div className="text-[8px] font-semibold uppercase tracking-wider text-muted">SP remaining</div>
              <div className="font-mono text-[12px] text-foreground" title={exactSp(summary.spRemaining)}>
                {formatSpCompact(summary.spRemaining)}
              </div>
            </div>
          </div>
        </>
      )}

      {summary.syncedAt ? (
        <p className="text-[9px] text-muted">Synced {formatRelativeAge(summary.syncedAt)}</p>
      ) : null}
    </div>
  );
}

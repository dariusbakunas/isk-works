import type { SkillQueueEntry } from "../../api/characters";
import { formatRelativeAge, formatRemainingMs, formatTrainingSkillLabel } from "./characters-formatters";
import { useTrainingCountdown } from "./use-training-countdown";

const STALE_OBSERVATION_MS = 2 * 60 * 60 * 1000;

const NOTE_CLASS: Record<"card" | "inspector", string> = {
  card: "mt-1 text-[9px] text-muted",
  inspector: "text-[10px] text-muted",
};

// Leaf component: owns the ticking state internally via useTrainingCountdown
// so a 1-second tick re-renders only this subtree, never the parent card or
// its siblings. `observedAt` is consumed here (not by the hook) purely to
// caveat the "refreshing" copy -- staleness doesn't change which entry is
// current, so it stays out of the pure derivation.
export function LiveTrainingProgress({
  queue,
  observedAt,
  onNeedsRefresh,
  variant,
}: {
  queue: SkillQueueEntry[];
  observedAt: string | null;
  onNeedsRefresh: () => void;
  variant: "card" | "inspector";
}) {
  const state = useTrainingCountdown(queue, onNeedsRefresh);

  if (state.kind === "empty") {
    return <p className={NOTE_CLASS[variant]}>No skill queue</p>;
  }

  if (state.kind === "cachedQueueExpired") {
    const stale = observedAt !== null && Date.now() - new Date(observedAt).getTime() > STALE_OBSERVATION_MS;
    return (
      <p className={NOTE_CLASS[variant]}>
        Refreshing training state…{stale ? ` (last synced ${formatRelativeAge(observedAt)})` : ""}
      </p>
    );
  }

  if (state.kind === "paused") {
    const label = formatTrainingSkillLabel(state.next?.skillName ?? null, state.next?.finishedLevel ?? null);
    return (
      <div className={variant === "card" ? "mt-1.5" : undefined}>
        <div className="flex items-baseline justify-between gap-2">
          <span className={variant === "card" ? "max-w-[140px] truncate text-[9px] text-muted" : "truncate text-[10px] text-foreground"}>
            {label ?? "Training"}
          </span>
          <span className={variant === "card" ? "shrink-0 font-mono text-[9px] text-muted" : "shrink-0 font-mono text-[10px] text-muted"}>
            Paused
          </span>
        </div>
      </div>
    );
  }

  const label = formatTrainingSkillLabel(state.entry.skillName, state.entry.finishedLevel);
  const remaining = formatRemainingMs(state.remainingMs);
  const progressPercent = state.fraction * 100;

  if (variant === "card") {
    return (
      <div className="mt-1.5">
        <div className="h-[3px] bg-border">
          <div
            className="h-full bg-[#4a8f5a]"
            data-testid="training-progress-bar"
            style={{ width: `${progressPercent}%` }}
          />
        </div>
        <div className="mt-0.5 flex items-baseline justify-between gap-2">
          <span className="max-w-[140px] truncate text-[9px] text-muted">{label ?? "Training"}</span>
          <span className="shrink-0 font-mono text-[9px] text-muted">{remaining}</span>
        </div>
      </div>
    );
  }

  return (
    <>
      <div className="mb-1 flex items-center justify-between text-[10px]">
        <span className="truncate text-foreground">{label ?? "Training"}</span>
        <span className="shrink-0 font-mono text-muted">{remaining}</span>
      </div>
      <div className="h-1 bg-border">
        <div className="h-full bg-[#4a8f5a]" data-testid="training-progress-bar" style={{ width: `${progressPercent}%` }} />
      </div>
    </>
  );
}

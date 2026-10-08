import type { ReactNode } from "react";

export function BuildPlannerCommitBar({
  cancelDisabled = false,
  cancelLabel = "Cancel",
  commitDisabledReason,
  commitLabel,
  committing,
  onCancel,
  onCommit,
  secondaryAction,
}: {
  cancelDisabled?: boolean;
  cancelLabel?: string;
  commitDisabledReason: string;
  commitLabel: string;
  committing: boolean;
  onCancel?: () => void;
  onCommit: () => void;
  secondaryAction?: ReactNode;
}) {
  const reasonId = "build-planner-commit-disabled-reason";
  const disabled = committing || Boolean(commitDisabledReason);
  return (
    <div className="sticky bottom-0 z-30 flex min-h-10 flex-wrap items-center justify-end gap-1.5 border-t border-border bg-panel px-2 py-1">
      {commitDisabledReason ? (
        <span className="mr-auto text-xs text-muted" id={reasonId}>{commitDisabledReason}</span>
      ) : null}
      {onCancel ? (
        <button className="iw-button-secondary" disabled={committing || cancelDisabled} onClick={onCancel} type="button">
          {cancelLabel}
        </button>
      ) : null}
      {secondaryAction}
      <button
        aria-describedby={commitDisabledReason ? reasonId : undefined}
        className="iw-button-primary disabled:cursor-not-allowed disabled:opacity-50"
        disabled={disabled}
        onClick={onCommit}
        type="button"
      >
        {committing ? "Working..." : commitLabel}
      </button>
    </div>
  );
}

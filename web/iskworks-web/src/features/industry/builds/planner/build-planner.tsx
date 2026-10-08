import type { ReactNode } from "react";

import type { BuildPlannerMode } from "./contracts";
import { BuildPlannerCommitBar } from "./build-planner-commit-bar";
import { PlannerWorkbench } from "./planner-workbench";

export function BuildPlanner({
  assumptions,
  cancelDisabled,
  cancelLabel,
  commitDisabledReason,
  commitLabel,
  committing,
  identity,
  mode,
  notes,
  onCancel,
  onCommit,
  preview,
  secondaryAction,
  toolbar,
  workbench,
}: {
  assumptions: ReactNode;
  cancelDisabled?: boolean;
  cancelLabel?: string;
  commitDisabledReason?: string;
  commitLabel?: string;
  committing?: boolean;
  identity?: ReactNode;
  mode: BuildPlannerMode;
  notes?: ReactNode;
  onCancel?: () => void;
  onCommit?: () => void;
  preview?: ReactNode;
  secondaryAction?: ReactNode;
  toolbar?: ReactNode;
  workbench?: {
    editorError: string;
    previewUpdating: boolean;
    statusMessage?: string | null;
  };
}) {
  const identityLabel = mode === "create" ? "Build identity" : "Current Build";
  const actionBar = onCommit ? (
    <BuildPlannerCommitBar
      cancelDisabled={cancelDisabled}
      cancelLabel={cancelLabel}
      commitDisabledReason={commitDisabledReason ?? ""}
      commitLabel={commitLabel ?? "Commit"}
      committing={committing ?? false}
      onCancel={onCancel}
      onCommit={onCommit}
      secondaryAction={secondaryAction}
    />
  ) : null;
  return (
    <section aria-label="Build planner">
      {identity ? (
        <section aria-label={identityLabel}>
          {identity}
        </section>
      ) : null}
      {workbench ? (
        <div className="mt-4">
          <PlannerWorkbench
            actionBar={actionBar}
            error={workbench.editorError}
            overlay={assumptions}
            previewUpdating={workbench.previewUpdating}
            statusMessage={workbench.statusMessage}
            secondary={(
              <>
                {notes ? (
                  <details className="border-t border-border px-2 py-2">
                    <summary className="cursor-pointer text-xs font-semibold">Planning notes</summary>
                    <section className="mt-3" aria-label="Planning notes">{notes}</section>
                  </details>
                ) : null}
              </>
            )}
            toolbar={toolbar}
            worksheet={preview}
          />
        </div>
      ) : (
        <>
      <div className="min-w-0">
        <section className="mt-4" aria-label="Planning assumptions">
          {assumptions}
        </section>
        {notes ? (
          <section className="mt-4" aria-label="Planning notes">
            {notes}
          </section>
        ) : null}
      </div>
      {preview ? <div className="mt-5 min-w-0">{preview}</div> : null}
      {actionBar}
        </>
      )}
    </section>
  );
}

import type { ReactNode } from "react";

/**
 * The persistent Build-level context shown above
 * Plan / Logistics / Graph -- root runs and whole-Build economics
 * (`BuildEditorHeader`: material, installation, total, revenue, profit,
 * margin, the decision line -- shortage, blocking validation -- and the
 * pricing / evidence warnings badge), the root settings strip with "Edit
 * build settings", and notes.
 *
 * It renders what the one Build preview already computes (the editor's
 * `estimate`, refreshed on every overlay change and every sourcing write
 * via `bumpPreview`); it computes nothing itself.
 */
export function BuildWorkspaceSummary({
  identity,
  toolbar,
  blueprintDialog,
  notes,
  statusMessage,
  updating,
}: {
  identity: ReactNode;
  toolbar: ReactNode;
  blueprintDialog: ReactNode;
  notes: ReactNode;
  statusMessage: string | null;
  updating: boolean;
}) {
  return (
    <section aria-label="Build summary" className="mb-3 space-y-2">
      {identity}
      {toolbar}
      {blueprintDialog}
      {statusMessage ? (
        <p className="text-xs text-muted" role="status">
          {statusMessage}
        </p>
      ) : updating ? (
        <p className="sr-only" role="status">
          Updating Build economics
        </p>
      ) : null}
      <details className="text-xs">
        <summary className="cursor-pointer font-semibold text-muted">Planning notes</summary>
        <div className="mt-2">{notes}</div>
      </details>
    </section>
  );
}

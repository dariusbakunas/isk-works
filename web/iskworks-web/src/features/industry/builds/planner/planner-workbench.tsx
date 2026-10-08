import type { ReactNode } from "react";
import { Loader2 } from "lucide-react";

export function PlannerWorkbench({
  actionBar,
  error,
  overlay,
  previewUpdating,
  secondary,
  statusMessage,
  toolbar,
  worksheet,
}: {
  actionBar: ReactNode;
  error: string;
  overlay?: ReactNode;
  previewUpdating: boolean;
  secondary?: ReactNode;
  statusMessage?: string | null;
  toolbar: ReactNode;
  worksheet: ReactNode;
}) {
  return (
    <section aria-label="Build planner workbench" className="min-w-0">
      {toolbar}
      {overlay}
      {error ? <p className="border-b border-border px-2 py-2 text-sm text-destructive" role="alert">{error}</p> : null}
      {statusMessage ? (
        <p className="flex items-center gap-2 border-b border-border bg-muted/10 px-2 py-2 text-xs text-muted" role="status">
          <Loader2 aria-hidden="true" className="h-3.5 w-3.5 animate-spin" />
          {statusMessage}
        </p>
      ) : null}
      <div
        aria-busy={previewUpdating}
        className={`min-w-0 transition-opacity ${previewUpdating ? "opacity-60" : ""}`}
      >
        {worksheet}
      </div>
      {secondary}
      {actionBar}
    </section>
  );
}

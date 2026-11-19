import { AlertTriangle, CheckCircle2 } from "lucide-react";

import type { WorksheetBlueprintSummary } from "../../../../api/industry";

/** Worksheet header summary: blueprints owned, missing for planned builds,
 * and producers planning on manual ME/TE despite an owned blueprint. */
export function BlueprintCoverageChip({ summary }: { summary: WorksheetBlueprintSummary }) {
  const missing = summary.missing > 0;
  return (
    <span className="inline-flex h-6 items-center gap-1.5 whitespace-nowrap border border-border bg-panel-strong px-2 text-xs text-muted" data-testid="blueprint-coverage-chip">
      {missing
        ? <AlertTriangle aria-hidden="true" className="h-3.5 w-3.5 text-warning" />
        : <CheckCircle2 aria-hidden="true" className="h-3.5 w-3.5 text-muted" />}
      <span>Blueprints · <span className="font-mono text-foreground">{summary.owned} owned</span></span>
      {missing ? <span className="text-warning">· {summary.missing} missing</span> : null}
      {summary.producerUsesManual > 0 ? <span>· {summary.producerUsesManual} manual</span> : null}
    </span>
  );
}

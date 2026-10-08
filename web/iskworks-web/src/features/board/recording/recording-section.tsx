import { useRef, useState } from "react";

import type { TicketSummary } from "../../../api/industry";
import { Badge } from "../../../components/primitives";
import { AcquisitionRecordingForm } from "./acquisition-recording-form";
import { ProductionRecordingForm } from "./production-recording-form";
import { RecordingHistory } from "./recording-history";
import { recordingStateMeta, recordingTerms } from "./recording-meta";

function Row({
  label,
  value,
  emphasise = false,
}: {
  label: string;
  value: string;
  emphasise?: boolean;
}) {
  return (
    <div className="flex justify-between gap-2">
      <dt className="text-muted">{label}</dt>
      <dd
        className={`min-w-0 flex-1 text-right font-mono tabular-nums ${
          emphasise ? "text-warning" : "text-foreground"
        }`}
      >
        {value}
      </dd>
    </div>
  );
}

// The canonical outer recording panel, shared by Acquisition and
// Manufacturing/Reaction tickets. It owns: the derived recording state
// chip, the requested / recorded / remaining / surplus rollup, and
// expansion of the type-specific editor. It renders NOTHING about workflow
// status -- recording state and workflow status are independent dimensions
// and a difference between them (Complete + Not recorded, Ready + Recorded)
// is never surfaced as a warning.
export function RecordingSection({
  ticket,
  onRecorded,
}: {
  ticket: TicketSummary;
  /** Called after a confirmed recording so the caller can refetch the
   * ticket summary (and any visible Inventory). Never a status change. */
  onRecorded: () => void;
}) {
  const [open, setOpen] = useState(false);
  const actionRef = useRef<HTMLButtonElement>(null);

  const recording = ticket.recording;
  // The backend only returns a summary for Acquisition / Manufacturing /
  // Reaction tickets; anything else has no recording concept.
  if (!recording) return null;

  const terms = recordingTerms(ticket.kind);
  const stateMeta = recordingStateMeta[recording.state];
  const isProduction = ticket.kind === "manufacturing" || ticket.kind === "reaction";
  const isBatchedAcquisition =
    ticket.kind === "acquisition" && ticket.acquisitionRunId !== null;

  function collapse() {
    setOpen(false);
    // Return focus to the trigger the user came from.
    requestAnimationFrame(() => actionRef.current?.focus());
  }

  function handleRecorded() {
    collapse();
    onRecorded();
  }

  return (
    <section aria-label="Recording" className="space-y-2">
      <h3 className="text-[10px] font-semibold uppercase tracking-wide text-muted">Recording</h3>

      <div data-testid="recording-state">
        <Badge square tone={stateMeta.tone}>
          <span aria-hidden="true" className="mr-1">
            {stateMeta.glyph}
          </span>
          {stateMeta.label}
        </Badge>
      </div>

      <dl className="space-y-1 text-xs">
        <Row label={terms.requested} value={recording.requestedQuantity.toLocaleString()} />
        <Row label={terms.recorded} value={recording.recordedQuantity.toLocaleString()} />
        <Row label={terms.remaining} value={recording.remainingQuantity.toLocaleString()} />
        {recording.surplusQuantity > 0 ? (
          <Row
            emphasise
            label={terms.surplus}
            value={recording.surplusQuantity.toLocaleString()}
          />
        ) : null}
      </dl>

      <RecordingHistory
        onChanged={onRecorded}
        recordings={ticket.recordings ?? []}
        ticketId={ticket.id}
      />

      {isBatchedAcquisition ? (
        <p className="iw-muted text-xs">
          Recorded through its Acquisition Run — record and complete it from the Run.
        </p>
      ) : open ? (
        isProduction ? (
          <ProductionRecordingForm
            onCancel={collapse}
            onRecorded={handleRecorded}
            ticket={ticket}
          />
        ) : (
          <AcquisitionRecordingForm
            onCancel={collapse}
            onRecorded={handleRecorded}
            ticket={ticket}
          />
        )
      ) : (
        <button
          className="iw-button-secondary"
          onClick={() => setOpen(true)}
          ref={actionRef}
          type="button"
        >
          {terms.action}
        </button>
      )}
    </section>
  );
}

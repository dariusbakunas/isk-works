import { useEffect } from "react";

import type { TicketSummary } from "../../../api/industry";
import { AcquisitionRecordingForm } from "./acquisition-recording-form";
import { ProductionRecordingForm } from "./production-recording-form";
import { recordingTerms } from "./recording-meta";

// Recording a ticket's outcome, as a dialog over the Board: enter the
// actual values, preview what they do to inventory, record. Closing never
// records anything.
export function RecordOutcomeDialog({
  ticket,
  onClose,
  onRecorded,
}: {
  ticket: TicketSummary;
  onClose: () => void;
  onRecorded: () => void;
}) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const isProduction = ticket.kind === "manufacturing" || ticket.kind === "reaction";
  const titleId = `record-outcome-${ticket.id}`;
  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby={titleId}
        aria-modal="true"
        className="iw-dialog max-h-[90vh] w-full max-w-md overflow-y-auto p-4"
        role="dialog"
      >
        <div className="mb-3 min-w-0">
          <h2 className="text-base font-semibold" id={titleId}>
            {recordingTerms(ticket.kind).action}
          </h2>
          <p className="truncate text-xs text-muted">
            {ticket.displayId} · {ticket.capturedName}
          </p>
        </div>
        {isProduction ? (
          <ProductionRecordingForm onCancel={onClose} onRecorded={onRecorded} ticket={ticket} />
        ) : (
          <AcquisitionRecordingForm onCancel={onClose} onRecorded={onRecorded} ticket={ticket} />
        )}
      </section>
    </div>
  );
}

import { useState } from "react";

import {
  revertTicketInventoryRecording,
  type TicketInventoryRecording,
} from "../../../api/industry";
import { Badge, ConfirmDialog, InlineAlert } from "../../../components/primitives";
import { apiMessage } from "../../industry/shared/api-error";

function signed(quantity: number): string {
  return `${quantity >= 0 ? "+" : ""}${quantity.toLocaleString()}`;
}

function recordingLabel(recording: TicketInventoryRecording): string {
  const output = recording.effects.find((effect) => effect.kind === "productionOutput");
  const primary = output ?? recording.effects.find((effect) => effect.kind === "purchase");
  if (primary) return `${Math.abs(primary.quantityDelta).toLocaleString()} × ${primary.capturedName}`;
  if (recording.kind === "production") {
    return `${recording.runsCompleted?.toLocaleString() ?? "0"} production run${recording.runsCompleted === 1 ? "" : "s"}`;
  }
  return `${recording.recordedQuantity?.toLocaleString() ?? "0"} recorded`;
}

export function RecordingHistory({
  ticketId,
  recordings,
  onChanged,
}: {
  ticketId: string;
  recordings: TicketInventoryRecording[];
  onChanged: () => void;
}) {
  const [selected, setSelected] = useState<TicketInventoryRecording | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  if (recordings.length === 0) return null;

  async function revert() {
    if (!selected || busy) return;
    const recordingId = selected.id;
    setSelected(null);
    setError("");
    setBusy(true);
    try {
      await revertTicketInventoryRecording(ticketId, recordingId);
      onChanged();
    } catch (requestError) {
      setError(apiMessage(requestError));
      onChanged();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-2">
      <h4 className="text-[10px] font-semibold uppercase tracking-wide text-muted">History</h4>
      {error ? <InlineAlert title="Recording was not reverted">{error}</InlineAlert> : null}
      <ol className="space-y-2">
        {recordings.map((recording) => (
          <li className="min-w-0 rounded border border-border p-2 text-xs" key={recording.id}>
            <div className="flex min-w-0 flex-wrap items-start justify-between gap-2">
              <div className="min-w-0 break-words">
                <div className="font-medium text-foreground">{recordingLabel(recording)}</div>
                <div className="text-muted">
                  {new Date(recording.recordedAt).toLocaleString()}
                  {recording.locationNote ? ` · ${recording.locationNote}` : ""}
                </div>
                {recording.note ? <div className="mt-1 break-words text-muted">{recording.note}</div> : null}
              </div>
              {recording.status === "reversed" ? (
                <Badge square tone="muted">Reverted</Badge>
              ) : (
                <button
                  className="iw-button-secondary shrink-0"
                  disabled={busy}
                  onClick={() => setSelected(recording)}
                  type="button"
                >
                  Revert recording
                </button>
              )}
            </div>
          </li>
        ))}
      </ol>

      <ConfirmDialog
        confirmLabel="Revert recording"
        onCancel={() => setSelected(null)}
        onConfirm={() => void revert()}
        open={selected !== null}
        title="Revert recording?"
      >
        <span className="block">
          This will reverse the inventory change created by this recording. The original record
          will remain in history.
        </span>
        {selected ? (
          selected.effects.length === 0 ? (
            <span className="mt-2 block">No inventory events were posted by this recording.</span>
          ) : (
            <span className="mt-2 block">
              {selected.effects.length > 1
                ? `This will reverse ${selected.effects.length} inventory changes.`
                : "This will reverse:"}
              {selected.effects.map((effect) => (
                <span className="mt-1 block break-words" key={effect.eventId}>
                  {signed(effect.quantityDelta)} × {effect.capturedName}
                  {selected.locationNote ? ` at ${selected.locationNote}` : ""}
                </span>
              ))}
            </span>
          )
        ) : null}
      </ConfirmDialog>
    </div>
  );
}

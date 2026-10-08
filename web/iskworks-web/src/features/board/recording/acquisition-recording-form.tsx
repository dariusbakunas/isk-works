import { useRef, useState } from "react";

import { recordTicketAcquisition, type TicketSummary } from "../../../api/industry";
import { MoneyInput } from "../../../components/money-input/money-input";
import type { MoneyInputResult } from "../../../components/money-input/parse-money-input";
import { InlineAlert } from "../../../components/primitives";
import { apiMessage } from "../../industry/shared/api-error";
import { newIdempotencyKey } from "./recording-meta";

// The type-specific editor behind RecordingSection for an Acquisition
// ticket. Posts exactly one explicit `record-acquisition` -- never
// `/start`, `/complete`, `/cancel`, and never a workflow-status write.
export function AcquisitionRecordingForm({
  ticket,
  onCancel,
  onRecorded,
}: {
  ticket: TicketSummary;
  onCancel: () => void;
  onRecorded: () => void;
}) {
  // RecordingSection only renders this when `ticket.recording` is present.
  const remaining = ticket.recording?.remainingQuantity ?? 0;

  // Default to the outstanding amount; once it's all recorded, start blank
  // (0 can't be submitted, and acquiring surplus is a legitimate explicit
  // choice the user makes deliberately, not a default).
  const [quantity, setQuantity] = useState(remaining > 0 ? String(remaining) : "");
  // Unit cost stays blank -- the backend has its own cost-resolution
  // hierarchy (actual price -> ticket estimate -> inventory average) and
  // that logic must not be mirrored here.
  const [unitCost, setUnitCost] = useState("");
  const [unitCostResult, setUnitCostResult] = useState<MoneyInputResult>({ status: "empty" });
  const [locationNote, setLocationNote] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // Retained across re-renders and retries; replaced only after a confirmed
  // success so the next recording is a new logical operation.
  const idempotencyKeyRef = useRef<string>(newIdempotencyKey());

  const quantityNumber = Number(quantity);
  const quantityValid =
    quantity.trim() !== "" &&
    Number.isFinite(quantityNumber) &&
    Number.isInteger(quantityNumber) &&
    quantityNumber > 0;
  const unitCostValid = unitCostResult.status === "empty" || unitCostResult.status === "valid";
  const canSubmit = quantityValid && unitCostValid && !busy;

  async function submit() {
    if (!canSubmit) {
      setError(
        !quantityValid
          ? "Enter a positive whole-number quantity."
          : "Enter a valid unit cost, or leave it blank.",
      );
      return;
    }
    setBusy(true);
    setError("");
    try {
      await recordTicketAcquisition(ticket.id, {
        idempotencyKey: idempotencyKeyRef.current,
        quantity: quantityNumber,
        unitCost:
          unitCostResult.status === "valid" ? unitCostResult.canonical : undefined,
        locationNote: locationNote.trim() || undefined,
        note: note.trim() || undefined,
      });
      // Fresh key for whatever the user records next.
      idempotencyKeyRef.current = newIdempotencyKey();
      onRecorded();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      aria-label="Record acquisition"
      className="space-y-2 rounded-[2px] border border-border bg-panel-strong/40 p-2"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <p className="text-[10px] font-semibold uppercase tracking-wide text-muted">Record acquisition</p>

      <label className="block text-xs font-medium text-muted">
        Quantity
        <input
          className="iw-input mt-0.5 font-mono"
          inputMode="numeric"
          min="1"
          onChange={(event) => setQuantity(event.target.value)}
          step="1"
          type="number"
          value={quantity}
        />
      </label>

      <label className="block text-xs font-medium text-muted">
        Unit cost <span className="font-normal">(ISK — optional)</span>
        <MoneyInput
          aria-label="Unit cost (ISK)"
          className="mt-0.5"
          onClear={() => setUnitCost("")}
          onCommit={(canonical) => setUnitCost(canonical)}
          onValueChange={setUnitCostResult}
          value={unitCost}
        />
      </label>

      <label className="block text-xs font-medium text-muted">
        Location / reference <span className="font-normal">(optional)</span>
        <input
          className="iw-input mt-0.5"
          onChange={(event) => setLocationNote(event.target.value)}
          value={locationNote}
        />
      </label>

      <label className="block text-xs font-medium text-muted">
        Note <span className="font-normal">(optional)</span>
        <textarea
          className="iw-input mt-0.5 min-h-14"
          onChange={(event) => setNote(event.target.value)}
          value={note}
        />
      </label>

      {error ? <InlineAlert title="Recording did not finish">{error}</InlineAlert> : null}

      <div className="flex justify-end gap-2">
        <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">
          Cancel
        </button>
        <button className="iw-button-primary" disabled={!canSubmit} type="submit">
          {busy ? "Recording…" : "Record"}
        </button>
      </div>
    </form>
  );
}

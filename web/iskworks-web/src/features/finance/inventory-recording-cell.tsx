import { useEffect, useRef, useState } from "react";

import type { InventoryPreview } from "../../api/inventory";
import {
  previewFinanceInventory,
  recordFinanceInventory,
  revertFinanceInventoryRecording,
  type FinanceInventoryRecording,
  type FinanceTransaction,
} from "../../api/finance";
import { MoneyAmount, formatIskForSentence } from "../../components/money";
import { ConfirmDialog, KV } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { InventoryPreviewPanel } from "../industry/inventory/inventory-posting-panel";

interface Props {
  row: FinanceTransaction;
  /** Called with the server-confirmed state after a record or revert. */
  onChange: (observationId: string, recording: FinanceInventoryRecording) => void;
}

const compactButton =
  "inline-flex h-4 items-center rounded-[3px] border px-1.5 text-[0.625rem] font-semibold leading-none transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-60";

/**
 * The Finance table's Inventory cell: "+ Inventory" on an eligible Market Buy
 * opens the server's posting preview (what the quantity, total cost and
 * average unit cost become), and only its confirm button records. Recorded
 * rows show a quiet "✓ Inventory" that opens the recording's details and the
 * explicit Revert. Every state comes from the server; nothing is optimistic.
 */
export function InventoryRecordingCell({ row, onChange }: Props) {
  const recording = row.inventoryRecording;
  const [busy, setBusy] = useState<"previewing" | "recording" | "reverting" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ preview: InventoryPreview; again: boolean } | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  if (!recording) return null;

  const addLabel = `Add ${row.quantity.toLocaleString()} ${row.typeName} to inventory`;

  async function openPreview(again: boolean) {
    setBusy("previewing");
    setError(null);
    try {
      setPreview({ preview: await previewFinanceInventory(row.observationId), again });
    } catch (cause) {
      setError(errorMessage(cause, "Could not preview the inventory recording."));
    } finally {
      setBusy(null);
    }
  }

  async function record(again: boolean) {
    setBusy("recording");
    setError(null);
    try {
      onChange(row.observationId, await recordFinanceInventory(row.observationId));
      setPreview(null);
    } catch (cause) {
      // The preview dialog stays open and shows why.
      setError(errorMessage(cause, again ? "Could not add to inventory again." : "Could not add to inventory."));
    } finally {
      setBusy(null);
    }
  }

  const previewDialog = preview ? (
    <RecordingPreview
      busy={busy === "recording"}
      error={error}
      onClose={() => { setPreview(null); setError(null); }}
      onConfirm={() => void record(preview.again)}
      preview={preview.preview}
      row={row}
    />
  ) : null;

  async function revert() {
    if (!recording?.recordingId) return;
    setConfirmOpen(false);
    setBusy("reverting");
    setError(null);
    try {
      onChange(row.observationId, await revertFinanceInventoryRecording(row.observationId, recording.recordingId));
      setDetailsOpen(false);
    } catch (cause) {
      // The recording stays active; the details dialog shows why.
      setError(errorMessage(cause, "Could not revert the inventory recording."));
    } finally {
      setBusy(null);
    }
  }

  const failure = error && !detailsOpen && !preview ? (
    <span className="ml-1 text-[0.625rem] text-destructive" role="alert" title={error}>Failed</span>
  ) : null;

  if (busy === "previewing") {
    return <button className={`${compactButton} border-border text-muted`} disabled type="button">Loading…</button>;
  }

  switch (recording.state) {
    case "unrecorded":
      return (
        <>
          <button
            aria-label={addLabel}
            className={`${compactButton} border-primary/40 text-primary hover:bg-primary/10`}
            onClick={() => void openPreview(false)}
            title={error ?? "Preview adding this purchase to inventory"}
            type="button"
          >
            + Inventory
          </button>
          {failure}
          {previewDialog}
        </>
      );
    case "recorded":
      return (
        <>
          <button
            aria-label={`Inventory recording for ${row.quantity.toLocaleString()} ${row.typeName}`}
            className={`${compactButton} border-transparent text-positive/80 hover:border-positive/30 hover:text-positive`}
            onClick={() => { setError(null); setDetailsOpen(true); }}
            title="Recorded in inventory. Click for details."
            type="button"
          >
            ✓ Inventory
          </button>
          <RecordingDetails
            busy={busy === "reverting"}
            error={error}
            onClose={() => { setDetailsOpen(false); setError(null); }}
            onRevert={() => setConfirmOpen(true)}
            open={detailsOpen}
            recording={recording}
            row={row}
          />
          <ConfirmDialog
            confirmLabel="Revert recording"
            onCancel={() => setConfirmOpen(false)}
            onConfirm={() => void revert()}
            open={confirmOpen}
            title="Revert this inventory recording?"
          >
            This will remove {(recording.quantity ?? row.quantity).toLocaleString()} {row.typeName} and reverse{" "}
            {formatIskForSentence(recording.totalBasis ?? row.totalPrice)} of recorded acquisition cost. The
            original Finance transaction will remain unchanged.
          </ConfirmDialog>
        </>
      );
    case "reverted":
      return (
        <span className="inline-flex items-center gap-1">
          <span className="text-[0.625rem] text-warning" title="This inventory recording was reverted.">Reverted</span>
          <button
            aria-label={`${addLabel} again`}
            className={`${compactButton} border-border text-muted hover:border-primary/70 hover:text-foreground`}
            onClick={() => void openPreview(true)}
            title={error ?? "Preview adding to inventory again"}
            type="button"
          >
            +
          </button>
          {failure}
          {previewDialog}
        </span>
      );
    case "unavailable":
      return <span className="text-muted" title="Can't be added: the item type is unknown or this isn't a personal purchase.">—</span>;
  }
}

function RecordingPreview({ busy, error, onClose, onConfirm, preview, row }: {
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onConfirm: () => void;
  preview: InventoryPreview;
  row: FinanceTransaction;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);
  const titleId = `inventory-recording-preview-${row.observationId}`;
  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog w-full max-w-sm p-4 text-left" role="dialog">
        <div className="mb-3 flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="text-base font-semibold" id={titleId}>Add to inventory</h2>
            <p className="truncate text-xs text-muted" data-private="">
              {row.typeName} · <CharacterName name={row.characterName} />
            </p>
          </div>
          <button className="iw-button-secondary" disabled={busy} onClick={onClose} ref={closeRef} type="button">Cancel</button>
        </div>
        <InventoryPreviewPanel
          actionLabel="Add to Inventory"
          busy={busy}
          busyLabel="Adding…"
          onSave={onConfirm}
          preview={preview}
        />
        {error ? <p className="mt-3 text-xs text-destructive" role="alert">{error}</p> : null}
      </section>
    </div>
  );
}

function RecordingDetails({ busy, error, onClose, onRevert, open, recording, row }: {
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onRevert: () => void;
  open: boolean;
  recording: FinanceInventoryRecording;
  row: FinanceTransaction;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    closeRef.current?.focus();
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, open]);
  if (!open) return null;
  const titleId = `inventory-recording-${row.observationId}`;
  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog w-full max-w-sm p-4" role="dialog">
        <h2 className="text-base font-semibold" id={titleId}>Recorded in inventory</h2>
        <div className="mt-2 divide-y divide-border" data-private="">
          <KV label="Item" value={row.typeName} />
          <KV label="Quantity recorded" value={(recording.quantity ?? row.quantity).toLocaleString()} />
          <KV label="Acquisition cost" value={<MoneyAmount value={recording.totalBasis ?? row.totalPrice} />} />
          <KV label="Unit cost" value={<MoneyAmount value={row.unitPrice} />} />
          <KV
            label="Recorded"
            value={recording.recordedAt
              ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(recording.recordedAt))
              : "—"}
          />
          <KV label="Character" value={<CharacterName name={row.characterName} />} />
          <KV label="Status" value="Recorded" />
        </div>
        {error ? <p className="mt-3 text-xs text-destructive" role="alert">{error}</p> : null}
        <div className="mt-4 flex justify-end gap-2">
          <button className="iw-button-secondary" onClick={onClose} ref={closeRef} type="button">Close</button>
          <button className="iw-button-danger" disabled={busy} onClick={onRevert} type="button">
            {busy ? "Reverting…" : "Revert inventory recording"}
          </button>
        </div>
      </section>
    </div>
  );
}

function errorMessage(cause: unknown, fallback: string) {
  return cause instanceof Error && cause.message ? cause.message : fallback;
}

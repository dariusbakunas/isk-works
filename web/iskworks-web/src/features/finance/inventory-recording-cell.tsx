import { useEffect, useRef, useState } from "react";
import { Minus, Plus } from "lucide-react";

import type { InventoryPreview } from "../../api/inventory";
import {
  previewFinanceInventory,
  recordFinanceInventory,
  revertFinanceInventoryRecording,
  type FinanceInventoryRecording,
  type FinanceTransaction,
} from "../../api/finance";
import { MoneyAmount, formatIskForSentence, formatIskSummary } from "../../components/money";
import { sumDecimalStrings } from "../../lib/decimal";
import { ConfirmDialog, KV } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { InventoryPreviewPanel } from "../../components/inventory-preview-panel";

interface Props {
  row: FinanceTransaction;
  /** Called with the server-confirmed state after a record or revert. */
  onChange: (observationId: string, recording: FinanceInventoryRecording) => void;
}

const compactButton =
  "inline-flex h-4 items-center rounded-[3px] border px-1.5 text-[0.625rem] font-semibold leading-none transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-60";

/**
 * The Finance table's Inventory cell: "+ Inventory" on an eligible Market Buy
 * (or "− Inventory" on a Market Sell) opens the server's posting preview (what
 * the quantity, total cost and average unit cost become), and only its confirm
 * button records. Recorded rows show a quiet "✓ Inventory" that opens the
 * recording's details and the explicit Revert. Every state comes from the
 * server; nothing is optimistic.
 */
export function InventoryRecordingCell({ row, onChange }: Props) {
  const recording = row.inventoryRecording;
  const [busy, setBusy] = useState<"previewing" | "recording" | "reverting" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ preview: InventoryPreview; again: boolean } | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  if (!recording) return null;

  const copy = recordingCopy(row);
  const shortage = saleShortage(row, recording);

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
      setError(errorMessage(cause, again ? `${copy.failure} again.` : `${copy.failure}.`));
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
      if (shortage) return <ShortStockButton label={copy.button} reason={shortage} row={row} />;
      return (
        <>
          <button
            aria-label={copy.action}
            className={`${compactButton} border-primary/40 text-primary hover:bg-primary/10`}
            onClick={() => void openPreview(false)}
            title={error ?? copy.previewHint}
            type="button"
          >
            {copy.button}
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
            title={`${copy.recorded}. Click for details.`}
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
            {copy.sale ? (
              <>
                This will return {(recording.quantity ?? row.quantity).toLocaleString()} {row.typeName} to inventory and
                restore {formatIskForSentence(recording.totalBasis ?? "0")} of cost basis.
              </>
            ) : (
              <>
                This will remove {(recording.quantity ?? row.quantity).toLocaleString()} {row.typeName} and reverse{" "}
                {formatIskForSentence(recording.totalBasis ?? row.totalPrice)} of recorded acquisition cost.
              </>
            )}{" "}
            The original Finance transaction will remain unchanged.
          </ConfirmDialog>
        </>
      );
    case "reverted":
      return (
        <span className="inline-flex items-center gap-1">
          <span className="text-[0.625rem] text-warning" title="This inventory recording was reverted.">Reverted</span>
          {shortage ? <ShortStockButton label="−" reason={shortage} row={row} again /> : (
          <button
            aria-label={`${copy.action} again`}
            className={`${compactButton} border-border text-muted hover:border-primary/70 hover:text-foreground`}
            onClick={() => void openPreview(true)}
            title={error ?? `${copy.previewHint} again`}
            type="button"
          >
            {copy.sale ? "−" : "+"}
          </button>
          )}
          {failure}
          {previewDialog}
        </span>
      );
    case "unavailable":
      return <span className="text-muted" title={copy.unavailable}>—</span>;
  }
}

/**
 * Why a sell can't be removed from inventory yet: the character holds fewer
 * units than were sold. `null` when it can (or for a buy, or when the server
 * didn't say how much is on hand -- then the preview reports it).
 */
function saleShortage(row: FinanceTransaction, recording: FinanceInventoryRecording): string | null {
  const onHand = recording.inventoryQuantity;
  if (row.transactionType !== "marketSell" || onHand == null || onHand >= row.quantity) return null;
  const held = onHand === 0 ? `No ${row.typeName} in inventory` : `Only ${onHand.toLocaleString()} ${row.typeName} in inventory`;
  return `${held}, but this sale is for ${row.quantity.toLocaleString()}. Add the missing stock to inventory first.`;
}

/**
 * A disabled "− Inventory" whose reason shows on hover. The title sits on a
 * wrapper because disabled buttons don't receive pointer events in every
 * browser.
 */
function ShortStockButton({ again = false, label, reason, row }: {
  again?: boolean;
  label: string;
  reason: string;
  row: FinanceTransaction;
}) {
  return (
    <span className="inline-flex" title={reason}>
      <button
        aria-label={`${recordingCopy(row).action}${again ? " again" : ""}`}
        aria-description={reason}
        className={`${compactButton} border-border text-muted`}
        disabled
        type="button"
      >
        {label}
      </button>
    </span>
  );
}

/** Wording for a buy (added to inventory) or a sell (removed from it). */
function recordingCopy(row: FinanceTransaction) {
  const units = `${row.quantity.toLocaleString()} ${row.typeName}`;
  return row.transactionType === "marketSell"
    ? {
      sale: true,
      action: `Remove ${units} sold from inventory`,
      button: "− Inventory",
      previewHint: "Preview removing these sold items from inventory",
      dialogTitle: "Remove sold items from inventory",
      confirm: "Remove from Inventory",
      confirming: "Removing…",
      failure: "Could not remove from inventory",
      recorded: "Removed from inventory",
      quantityLabel: "Quantity removed",
      basisLabel: "Cost basis removed",
      revertLabel: "Revert inventory removal",
      unavailable: "Can't be recorded: the item type is unknown or this isn't a personal sale.",
    }
    : {
      sale: false,
      action: `Add ${units} to inventory`,
      button: "+ Inventory",
      previewHint: "Preview adding this purchase to inventory",
      dialogTitle: "Add to inventory",
      confirm: "Add to Inventory",
      confirming: "Adding…",
      failure: "Could not add to inventory",
      recorded: "Recorded in inventory",
      quantityLabel: "Quantity recorded",
      basisLabel: "Acquisition cost",
      revertLabel: "Revert inventory recording",
      unavailable: "Can't be added: the item type is unknown or this isn't a personal purchase.",
    };
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
  const copy = recordingCopy(row);
  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog iw-dialog-frame w-full max-w-sm p-4 text-left" role="dialog">
        <div className="mb-3 flex shrink-0 items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="text-base font-semibold" id={titleId}>{copy.dialogTitle}</h2>
            <p className="truncate text-xs text-muted" data-private="">
              {row.typeName} · <CharacterName name={row.characterName} />
            </p>
          </div>
          <button className="iw-button-secondary" disabled={busy} onClick={onClose} ref={closeRef} type="button">Cancel</button>
        </div>
        <div className="iw-dialog-body">
          <InventoryPreviewPanel busy={busy} preview={preview} />
          {copy.sale ? <SaleMargin preview={preview} row={row} /> : null}
          {error ? <p className="mt-3 text-xs text-destructive" role="alert">{error}</p> : null}
        </div>
        <div className="iw-dialog-footer">
          <button className="iw-button-primary w-full" disabled={busy} onClick={onConfirm} type="button">
            {copy.sale
              ? <Minus className="mr-2 h-4 w-4" aria-hidden="true" />
              : <Plus className="mr-2 h-4 w-4" aria-hidden="true" />}
            {busy ? copy.confirming : copy.confirm}
          </button>
        </div>
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
  const copy = recordingCopy(row);
  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog iw-dialog-frame w-full max-w-sm p-4" role="dialog">
        <h2 className="text-base font-semibold" id={titleId}>{copy.recorded}</h2>
        <div className="iw-dialog-body">
          <div className="mt-2 divide-y divide-border" data-private="">
            <KV label="Item" value={row.typeName} />
            <KV label={copy.quantityLabel} value={(recording.quantity ?? row.quantity).toLocaleString()} />
            <KV label={copy.basisLabel} value={<MoneyAmount value={recording.totalBasis ?? row.totalPrice} />} />
            <KV label={copy.sale ? "Sale proceeds" : "Unit cost"} value={<MoneyAmount value={copy.sale ? row.totalPrice : row.unitPrice} />} />
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
        </div>
        <div className="iw-dialog-footer">
          <button className="iw-button-secondary" onClick={onClose} ref={closeRef} type="button">Close</button>
          <button className="iw-button-danger" disabled={busy} onClick={onRevert} type="button">
            {busy ? "Reverting…" : copy.revertLabel}
          </button>
        </div>
      </section>
    </div>
  );
}

/**
 * What a sale earned over the cost basis it removes: gross wallet proceeds
 * minus weighted-average cost. Taxes and broker fees are separate journal
 * entries, so this is before fees.
 */
function SaleMargin({ preview, row }: { preview: InventoryPreview; row: FinanceTransaction }) {
  // A sale's cost delta is negative, so proceeds + delta = proceeds - cost.
  const margin = sumDecimalStrings([row.totalPrice, preview.posting.totalCostDelta]).value ?? "0";
  const negative = margin.startsWith("-");
  return (
    <dl className="mt-3 grid grid-cols-2 gap-3 rounded-md border border-border p-3 text-sm" data-testid="sale-margin">
      <div><dt className="text-xs text-muted">Sale proceeds</dt><dd className="mt-0.5 font-mono">{formatIskSummary(row.totalPrice)}</dd></div>
      <div>
        <dt className="text-xs text-muted">Margin over cost</dt>
        <dd className={`mt-0.5 font-mono ${negative ? "text-destructive" : "text-positive"}`}>
          {formatIskSummary(margin, { signDisplay: "always" })}
        </dd>
      </div>
      <p className="col-span-2 text-xs text-muted">Before sales tax and broker fees.</p>
    </dl>
  );
}

function errorMessage(cause: unknown, fallback: string) {
  return cause instanceof Error && cause.message ? cause.message : fallback;
}

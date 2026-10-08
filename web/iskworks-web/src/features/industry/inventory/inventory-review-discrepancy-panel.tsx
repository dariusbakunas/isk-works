import { useEffect, useState } from "react";

import {
  postAdjustment,
  previewAdjustment,
  type InventoryAdjustmentInput,
  type InventoryItem,
  type InventoryPreview,
} from "../../../api/inventory";
import { formatIskSummary } from "../../../components/money";
import { MoneyInput, type MoneyInputResult } from "../../../components/money-input";
import { InlineAlert } from "../../../components/primitives";
import { apiMessage } from "./shared";
import { InventoryPreviewPanel } from "./inventory-posting-panel";

/**
 * The Overview tab's "Review discrepancy" flow -- an inline panel, not a
 * modal, since it's reviewing a specific number the user is already
 * looking at rather than opening a fresh form. Reuses the exact same
 * previewAdjustment/postAdjustment calls (and InventoryPreviewPanel for
 * the before/after step) as the standalone Adjust Inventory panel; this
 * component only adds presentation around the discrepancy-specific copy
 * and the brand-new-item cost-required case, never its own cost/quantity
 * business logic.
 */
export function InventoryReviewDiscrepancyPanel({
  item,
  onCancel,
  onSaved,
  cancelLabel = "Cancel",
}: {
  item: InventoryItem;
  onCancel: () => void;
  onSaved: () => void;
  cancelLabel?: string;
}) {
  const observed = item.esiObservedQuantity;
  const difference = observed == null ? 0 : (item.reconciliationDifference ?? 0);
  const hasExistingCostBasis = Boolean(item.balance.averageUnitCost);
  const needsCost = difference > 0 && !hasExistingCostBasis;

  const [unitCost, setUnitCost] = useState("");
  const [unitCostResult, setUnitCostResult] = useState<MoneyInputResult>({ status: "empty" });
  const [preview, setPreview] = useState<InventoryPreview | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(!needsCost);
  const [error, setError] = useState("");

  const parsedUnitCost = Number(unitCostResult.canonical ?? "0");
  // Generic Money validity, plus this panel's own stricter rule: a
  // brand-new item's cost basis must be strictly positive (zero is a
  // syntactically valid Money but not a usable acquisition cost here).
  const unitCostValid = unitCostResult.status === "valid" && parsedUnitCost > 0;
  const unitCostNonPositive = unitCostResult.status === "valid" && parsedUnitCost <= 0;

  function buildInput(): InventoryAdjustmentInput {
    return {
      typeId: item.balance.key.typeId,
      typeName: item.balance.typeName,
      quantityDelta: difference,
      unitCost: difference > 0 && unitCostValid ? unitCostResult.canonical ?? null : null,
      sourceReference: "ESI observation review",
      note: "",
      expectedRevision: item.balance.revision,
    };
  }

  // `autoConfirm` skips straight to the before/after Post step once the
  // preview lands -- used by the brand-new-item "Create adjustment"
  // button, which is designed as a single click, not a review-then-confirm
  // pair. The existing-cost-basis/negative-discrepancy path previews
  // silently on mount only to populate the review copy's numbers; that
  // one still needs an explicit "Confirm adjustment" click before Post.
  async function runPreview(autoConfirm: boolean) {
    setBusy(true);
    setError("");
    try {
      setPreview(await previewAdjustment(buildInput()));
      if (autoConfirm) setConfirmed(true);
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!needsCost) void runPreview(false);
    // Runs once on mount: the panel is remounted per item, and the preview fills in the review numbers.
  }, []);

  async function post() {
    setBusy(true);
    setError("");
    try {
      await postAdjustment(buildInput());
      onSaved();
    } catch (requestError) {
      const message = apiMessage(requestError);
      setError(message.includes("changed") ? "Inventory changed while this review was open. Reopen it to see the latest balance." : message);
      setPreview(null);
      setConfirmed(false);
    } finally {
      setBusy(false);
    }
  }

  if (observed == null) return null;

  const magnitude = Math.abs(difference);

  if (preview && confirmed) {
    return (
      <div className="mt-2">
        <ReconciliationSummary item={item} />
        <InventoryPreviewPanel
          actionLabel="Post adjustment"
          busy={busy}
          busyLabel="Posting..."
          onSave={post}
          preview={preview}
        />
        {error ? <div className="mt-2"><InlineAlert title="Adjustment not posted">{error}</InlineAlert></div> : null}
        <button className="iw-button-secondary mt-2 w-full" disabled={busy} onClick={onCancel} type="button">
          {cancelLabel}
        </button>
      </div>
    );
  }

  if (needsCost && !preview) {
    return (
      <div className="mt-2 rounded-md border border-primary/60 bg-primary/5 p-3">
        <p className="text-xs font-semibold uppercase text-primary">New item -- cost required</p>
        <p className="mt-1.5 text-xs text-muted">
          ESI reports <strong className="text-foreground">{magnitude.toLocaleString()} units</strong> with no
          accounting record. A unit cost must be supplied to create the Adjustment.
        </p>
        <ReconciliationSummary item={item} />
        {item.currentPrice ? (
          <p className="mt-1 text-xs text-muted">Market reference: {formatIskSummary(item.currentPrice)} / unit</p>
        ) : null}
        <label className="mt-2 block text-xs font-semibold text-foreground">
          Unit cost (ISK)
          <MoneyInput
            aria-label="Unit cost (ISK)"
            className="mt-1"
            onClear={() => setUnitCost("")}
            onCommit={setUnitCost}
            onValueChange={setUnitCostResult}
            placeholder="e.g. 82400"
            value={unitCost}
          />
        </label>
        {unitCostNonPositive ? (
          <p className="mt-1 text-xs text-destructive" role="alert">Enter a cost greater than zero.</p>
        ) : null}
        {unitCostValid ? (
          <dl className="mt-2 grid grid-cols-3 gap-2 text-xs">
            <div><dt className="text-muted">Qty</dt><dd className="font-mono">+{magnitude.toLocaleString()}</dd></div>
            <div><dt className="text-muted">Unit cost</dt><dd className="font-mono">{formatIskSummary(unitCostResult.canonical ?? "0")}</dd></div>
            <div><dt className="text-muted">Total value</dt><dd className="font-mono">{formatIskSummary(String(magnitude * parsedUnitCost))}</dd></div>
          </dl>
        ) : null}
        {error ? <div className="mt-2"><InlineAlert title="Adjustment not ready">{error}</InlineAlert></div> : null}
        <div className="mt-3 grid grid-cols-2 gap-2">
          <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">{cancelLabel}</button>
          <button className="iw-button-primary" disabled={busy || !unitCostValid} onClick={() => runPreview(true)} type="button">
            {busy ? "Calculating..." : "Create adjustment"}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="mt-2 rounded-md border border-warning/60 bg-warning/5 p-3">
      <p className="text-xs font-semibold uppercase text-warning">Review discrepancy</p>
      <p className="mt-1.5 text-xs text-muted">
        ESI suggests {difference > 0 ? "an additional" : "a deficit of"}{" "}
        <strong className="text-foreground">{magnitude.toLocaleString()} units</strong>. This will be recorded as
        an Inventory Adjustment.
      </p>
      <ReconciliationSummary item={item} />
      {busy && !preview ? <p className="mt-2 text-xs text-muted" role="status">Calculating...</p> : null}
      {preview ? (
        <dl className="mt-2 grid grid-cols-2 gap-2 text-xs">
          <div><dt className="text-muted">Quantity</dt><dd className="font-mono">{difference > 0 ? "+" : ""}{difference.toLocaleString()}</dd></div>
          <div><dt className="text-muted">Unit cost basis</dt><dd className="font-mono">{difference > 0 ? "existing avg" : "existing weighted avg"}</dd></div>
          <div><dt className="text-muted">Adjustment value</dt><dd className="font-mono">{formatIskSummary(preview.posting.totalCostDelta)}</dd></div>
          <div><dt className="text-muted">New owned qty</dt><dd className="font-mono">{preview.resulting.quantity.toLocaleString()}</dd></div>
          <div className="col-span-2">
            <dt className="text-muted">New avg cost</dt>
            <dd className="font-mono">{preview.resulting.averageUnitCost ? formatIskSummary(preview.resulting.averageUnitCost) : "Unavailable"}</dd>
          </div>
        </dl>
      ) : null}
      {error ? <div className="mt-2"><InlineAlert title="Adjustment not ready">{error}</InlineAlert></div> : null}
      <p className="mt-2 text-xs text-muted">
        ESI observations do not automatically update accounting inventory. Confirm only if this adjustment is
        intentional.
      </p>
      <div className="mt-3 grid grid-cols-2 gap-2">
        <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">{cancelLabel}</button>
        <button className="iw-button-primary" disabled={busy || !preview} onClick={() => setConfirmed(true)} type="button">
          Confirm adjustment
        </button>
      </div>
    </div>
  );
}

function ReconciliationSummary({ item }: { item: InventoryItem }) {
  return (
    <dl className="mt-2 grid grid-cols-2 gap-2 text-xs sm:grid-cols-5">
      <div><dt className="text-muted">Observed</dt><dd className="font-mono">{item.esiObservedQuantity?.toLocaleString() ?? "—"}</dd></div>
      <div><dt className="text-muted">Ignored</dt><dd className="font-mono">{(item.ignoredEsiQuantity ?? 0).toLocaleString()}</dd></div>
      <div><dt className="text-muted">Included</dt><dd className="font-mono">{(item.includedEsiQuantity ?? item.esiObservedQuantity ?? 0).toLocaleString()}</dd></div>
      <div><dt className="text-muted">Accounting</dt><dd className="font-mono">{item.balance.quantity.toLocaleString()}</dd></div>
      <div><dt className="text-muted">Adjustment</dt><dd className="font-mono">{item.reconciliationDifference == null ? "—" : `${item.reconciliationDifference > 0 ? "+" : ""}${item.reconciliationDifference.toLocaleString()}`}</dd></div>
    </dl>
  );
}

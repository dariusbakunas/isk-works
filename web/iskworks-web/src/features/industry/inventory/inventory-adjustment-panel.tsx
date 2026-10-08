import { Calculator } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import {
  postAdjustment,
  previewAdjustment,
  type InventoryAdjustmentInput,
  type InventoryItem,
  type InventoryPreview,
} from "../../../api/inventory";
import { searchTypes, type TypeSearchResult } from "../../../api/sde";
import { EveTypeImage } from "../../../components/eve-type-image";
import { MoneyInput, type MoneyInputResult } from "../../../components/money-input";
import { useDebouncedLookup } from "../../../hooks/use-debounced-lookup";
import { InlineAlert } from "../../../components/primitives";
import { apiMessage } from "./shared";
import { InventoryPreviewPanel } from "./inventory-posting-panel";

type Direction = "add" | "remove";

function buildAdjustmentInput(
  selected: TypeSearchResult | null,
  direction: Direction,
  quantity: string,
  unitCost: string,
  sourceReference: string,
  note: string,
  existingRevision: number,
): InventoryAdjustmentInput | null {
  const magnitude = Number(quantity);
  if (!selected || !Number.isSafeInteger(magnitude) || magnitude <= 0) return null;
  return {
    typeId: selected.typeId,
    typeName: selected.typeName,
    quantityDelta: direction === "add" ? magnitude : -magnitude,
    unitCost: direction === "add" && unitCost.trim() ? unitCost : null,
    sourceReference,
    note,
    expectedRevision: existingRevision,
  };
}

export function InventoryAdjustmentPanel({
  items,
  preselectedTypeId,
  onCancel,
  onSaved,
}: {
  items: InventoryItem[];
  preselectedTypeId: number;
  onCancel: () => void;
  onSaved: () => void;
}) {
  const preselected = items.find((item) => item.balance.key.typeId === preselectedTypeId);
  const [selected, setSelected] = useState<TypeSearchResult | null>(
    preselected ? { typeId: preselected.balance.key.typeId, typeName: preselected.balance.typeName, groupName: null, published: true } : null,
  );
  const [query, setQuery] = useState("");
  const [direction, setDirection] = useState<Direction>("add");
  const [quantity, setQuantity] = useState("");
  const [unitCost, setUnitCost] = useState("");
  const [unitCostResult, setUnitCostResult] = useState<MoneyInputResult>({ status: "empty" });
  const [sourceReference, setSourceReference] = useState("");
  const [note, setNote] = useState("");
  const [preview, setPreview] = useState<InventoryPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const existing = selected
    ? items.find((item) => item.balance.key.typeId === selected.typeId)
    : preselected;
  const itemLookup = useDebouncedLookup<TypeSearchResult>(
    query,
    searchTypes,
    (requestError) => setError(apiMessage(requestError)),
  );
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialogRef.current;
    if (!element || element.open) return;
    if (typeof element.showModal === "function") element.showModal();
    else element.setAttribute("open", "");
  }, []);

  useEffect(() => {
    if (selected || !preselected) return;
    setSelected({
      typeId: preselected.balance.key.typeId,
      typeName: preselected.balance.typeName,
      groupName: null,
      published: true,
    });
  }, [preselected, selected]);

  const hasExistingCostBasis = Boolean(existing?.balance.averageUnitCost);

  // The canonical unit cost as soon as it parses valid, decoupled from the
  // debounced `unitCost` state so Preview never races the commit. Empty
  // when the field is blank/invalid/incomplete -- `buildAdjustmentInput`
  // then sends `null` (fall back to the weighted average).
  const unitCostForInput =
    unitCostResult.status === "valid" && unitCostResult.canonical !== undefined
      ? unitCostResult.canonical
      : "";
  const input = useMemo(
    () =>
      buildAdjustmentInput(
        selected,
        direction,
        quantity,
        unitCostForInput,
        sourceReference,
        note,
        existing?.balance.revision ?? 0,
      ),
    [selected, direction, quantity, unitCostForInput, sourceReference, note, existing],
  );

  async function runPreview() {
    if (!input) {
      setError("Select an EVE item and enter a positive whole-number quantity.");
      return;
    }
    if (direction === "add" && unitCostResult.status === "invalid") {
      setError("Enter a valid unit cost.");
      return;
    }
    if (direction === "add" && !hasExistingCostBasis && unitCostResult.status !== "valid") {
      setError("This item has no existing cost basis, so a unit cost is required.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      setPreview(await previewAdjustment(input));
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    if (!input || !preview) return;
    setBusy(true);
    setError("");
    try {
      await postAdjustment(input);
      onSaved();
    } catch (requestError) {
      const message = apiMessage(requestError);
      setError(message.includes("changed") ? "Inventory changed while this form was open. Reload the latest balance before recording this adjustment." : message);
      setPreview(null);
    } finally {
      setBusy(false);
    }
  }

  return (
    <dialog
      aria-labelledby="inventory-adjustment-title"
      className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(760px,calc(100vw-2rem))] overflow-y-auto p-5 text-foreground backdrop:bg-black/70"
      onCancel={(event) => { event.preventDefault(); onCancel(); }}
      ref={dialogRef}
    >
      <div className="mb-4 flex items-start justify-between gap-3">
        <div>
          <p className="iw-eyebrow">Manual correction</p>
          <h2 className="text-lg font-semibold" id="inventory-adjustment-title">Adjust Inventory</h2>
        </div>
        <button className="iw-button-secondary" onClick={onCancel} type="button">Cancel</button>
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <div className="grid content-start gap-3">
          {!selected ? (
            <>
              <label className="text-sm font-semibold">
                EVE item
                <input className="iw-input mt-1" value={query} onChange={(event) => setQuery(event.target.value)} />
              </label>
              {itemLookup.searching ? <p className="iw-muted text-xs" role="status">Searching active SDE...</p> : null}
              {itemLookup.results.map((result) => (
                <button className="flex items-center gap-3 rounded-md border border-border p-3 text-left hover:border-primary" key={result.typeId} onClick={() => setSelected(result)} type="button">
                  <EveTypeImage size={40} typeId={result.typeId} typeName={result.typeName} />
                  <span><strong>{result.typeName}</strong><span className="iw-muted ml-2">{result.groupName}</span></span>
                </button>
              ))}
            </>
          ) : (
            <div className="rounded-md border border-border bg-background p-3">
              <span className="iw-eyebrow">EVE item</span>
              <div className="flex items-center justify-between gap-2">
                <strong>{selected.typeName}</strong>
                {!existing ? <button className="text-sm text-primary" onClick={() => { setSelected(null); setPreview(null); }} type="button">Change</button> : null}
              </div>
            </div>
          )}
          <fieldset>
            <legend className="mb-1 text-sm font-semibold">Direction</legend>
            <div className="grid grid-cols-2 gap-2">
              {([
                ["add", "Add quantity"],
                ["remove", "Remove quantity"],
              ] as const).map(([value, label]) => (
                <label className={`rounded-md border p-3 text-sm ${direction === value ? "border-primary bg-primary/10" : "border-border"}`} key={value}>
                  <input className="mr-2" checked={direction === value} name="direction" onChange={() => { setDirection(value); setUnitCost(""); setPreview(null); }} type="radio" />
                  {label}
                </label>
              ))}
            </div>
          </fieldset>
          <label className="text-sm font-semibold">Quantity
            <input className="iw-input mt-1 font-mono" inputMode="numeric" min="1" step="1" type="number" value={quantity} onChange={(event) => { setQuantity(event.target.value); setPreview(null); }} />
          </label>
          {direction === "add" ? (
            <label className="text-sm font-semibold">
              Unit cost (ISK){hasExistingCostBasis ? <span className="iw-muted ml-1 font-normal">optional -- defaults to the current weighted average</span> : null}
              <MoneyInput
                aria-label="Unit cost (ISK)"
                className="mt-1"
                onClear={() => { setUnitCost(""); setPreview(null); }}
                onCommit={(canonical) => { setUnitCost(canonical); setPreview(null); }}
                onValueChange={(result) => { setUnitCostResult(result); setPreview(null); }}
                placeholder={hasExistingCostBasis ? existing?.balance.averageUnitCost ?? "" : ""}
                value={unitCost}
              />
            </label>
          ) : (
            <InlineAlert title="Priced automatically" tone="info">
              Removed quantity draws down carrying cost at the item's current weighted average -- no cost entry needed.
            </InlineAlert>
          )}
          <label className="text-sm font-semibold">Reason <span className="iw-muted font-normal">(optional)</span>
            <input className="iw-input mt-1" placeholder="e.g. relic/data loot, corp transfer, physical count" value={sourceReference} onChange={(event) => setSourceReference(event.target.value)} />
          </label>
          <label className="text-sm font-semibold">Notes
            <textarea className="iw-input mt-1 min-h-20" value={note} onChange={(event) => setNote(event.target.value)} />
          </label>
          {error ? <InlineAlert title="Adjustment not ready">{error}</InlineAlert> : null}
          <button className="iw-button-secondary justify-self-start" disabled={busy || (direction === "add" && unitCostResult.status === "invalid")} onClick={runPreview} type="button">
            <Calculator className="mr-2 h-4 w-4" aria-hidden="true" />
            {busy ? "Calculating..." : "Preview"}
          </button>
        </div>
        <div>
          {preview ? (
            <InventoryPreviewPanel preview={preview} onSave={save} busy={busy} />
          ) : (
            <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted">
              Enter the adjustment details, then preview the exact inventory result before recording it.
            </div>
          )}
        </div>
      </div>
    </dialog>
  );
}

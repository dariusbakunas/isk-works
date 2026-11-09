import { Calculator, Plus } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import {
  postInventory,
  previewInventory,
  type InventoryItem,
  type InventoryPostingInput,
  type InventoryPreview,
} from "../../../api/inventory";
import { searchTypes, type TypeSearchResult } from "../../../api/sde";
import { MoneyInput, type MoneyInputResult } from "../../../components/money-input";
import { EveTypeImage } from "../../../components/eve-type-image";
import { InventoryPreviewPanel } from "../../../components/inventory-preview-panel";
import { useDebouncedLookup } from "../../../hooks/use-debounced-lookup";
import { InlineAlert } from "../../../components/primitives";
import { apiMessage } from "./shared";

// Every opening balance and purchase is recorded at the unit cost the user
// enters -- one cost model, posted as "known". Something genuinely free is
// just a unit cost of 0. (The API still accepts the older "estimated" /
// "zeroCost" treatments, which existing history may carry.)
function buildPostingInput(
  selected: TypeSearchResult | null,
  quantity: string,
  unitCost: string,
  sourceReference: string,
  note: string,
  effectiveDate: string,
  existingRevision: number,
): InventoryPostingInput | null {
  const parsedQuantity = Number(quantity);
  if (!selected || !Number.isSafeInteger(parsedQuantity) || parsedQuantity <= 0) return null;
  return {
    typeId: selected.typeId,
    typeName: selected.typeName,
    quantity: parsedQuantity,
    unitCost,
    costQuality: "known",
    sourceReference,
    note,
    effectiveAt: new Date(`${effectiveDate}T12:00:00Z`).toISOString(),
    expectedRevision: existingRevision,
    acknowledgeZeroCost: false,
  };
}

export function InventoryPostingPanel({
  kind,
  items,
  preselectedTypeId,
  onCancel,
  onSaved,
}: {
  kind: "opening" | "purchase";
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
  const [quantity, setQuantity] = useState("");
  const [unitCost, setUnitCost] = useState("");
  const [unitCostResult, setUnitCostResult] = useState<MoneyInputResult>({ status: "empty" });
  const [sourceReference, setSourceReference] = useState("");
  const [note, setNote] = useState("");
  const [effectiveDate, setEffectiveDate] = useState(new Date().toISOString().slice(0, 10));
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

  // The canonical unit cost the moment it is valid -- decoupled from the
  // debounced `unitCost` state so Preview never races the ~250ms commit.
  const effectiveUnitCost =
    unitCostResult.status === "valid" && unitCostResult.canonical !== undefined
      ? unitCostResult.canonical
      : unitCost;
  const input = useMemo(
    () =>
      buildPostingInput(
        selected,
        quantity,
        effectiveUnitCost,
        sourceReference,
        note,
        effectiveDate,
        existing?.balance.revision ?? 0,
      ),
    [selected, quantity, effectiveUnitCost, sourceReference, note, effectiveDate, existing],
  );

  async function runPreview() {
    if (!input) {
      setError("Select an EVE item and enter a positive whole-number quantity.");
      return;
    }
    if (unitCostResult.status === "invalid") {
      setError("Enter a valid unit cost.");
      return;
    }
    if (unitCostResult.status !== "valid") {
      setError("Unit cost is required.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      setPreview(await previewInventory(kind, input));
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
      await postInventory(kind, input);
      onSaved();
    } catch (requestError) {
      const message = apiMessage(requestError);
      setError(message.includes("changed") ? "Inventory changed while this form was open. Reload the latest balance before recording this event." : message);
      setPreview(null);
    } finally {
      setBusy(false);
    }
  }

  return (
    <dialog
      aria-labelledby="inventory-posting-title"
      className="iw-dialog iw-dialog-frame m-auto w-[min(760px,calc(100vw-2rem))] p-5 text-foreground backdrop:bg-black/70"
      onCancel={(event) => { event.preventDefault(); onCancel(); }}
      ref={dialogRef}
    >
      <div className="mb-4 shrink-0">
        <p className="iw-eyebrow">{kind === "opening" ? "Starting position" : "Inbound inventory"}</p>
        <h2 className="text-lg font-semibold" id="inventory-posting-title">{kind === "opening" ? "Add Opening Balance" : "Record Purchase"}</h2>
      </div>
      <div className="iw-dialog-body grid gap-4 lg:grid-cols-2">
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
          <label className="text-sm font-semibold">Quantity
            <input className="iw-input mt-1 font-mono" inputMode="numeric" min="1" step="1" type="number" value={quantity} onChange={(event) => { setQuantity(event.target.value); setPreview(null); }} />
          </label>
          <label className="text-sm font-semibold">Unit cost (ISK)
            <MoneyInput
              aria-label="Unit cost (ISK)"
              className="mt-1"
              onClear={() => { setUnitCost(""); setPreview(null); }}
              onCommit={(canonical) => { setUnitCost(canonical); setPreview(null); }}
              onValueChange={(result) => { setUnitCostResult(result); setPreview(null); }}
              value={unitCost}
            />
          </label>
          <label className="text-sm font-semibold">{kind === "opening" ? "As-of date" : "Purchase date"}
            <input className="iw-input mt-1" type="date" value={effectiveDate} onChange={(event) => setEffectiveDate(event.target.value)} />
          </label>
          <label className="text-sm font-semibold">Source or reference
            <input className="iw-input mt-1" value={sourceReference} onChange={(event) => setSourceReference(event.target.value)} />
          </label>
          <label className="text-sm font-semibold">Notes
            <textarea className="iw-input mt-1 min-h-20" value={note} onChange={(event) => setNote(event.target.value)} />
          </label>
          {error ? <InlineAlert title="Inventory event not ready">{error}</InlineAlert> : null}
        </div>
        <div>
          {preview ? (
            <InventoryPreviewPanel preview={preview} busy={busy} />
          ) : (
            <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted">
              Enter the event details, then preview the exact inventory result before recording it.
            </div>
          )}
        </div>
      </div>
      <div className="iw-dialog-footer">
        <button className="iw-button-secondary" onClick={onCancel} type="button">Cancel</button>
        <button className="iw-button-secondary" disabled={busy || unitCostResult.status === "invalid"} onClick={runPreview} type="button">
          <Calculator className="mr-2 h-4 w-4" aria-hidden="true" />
          {busy ? "Calculating..." : "Preview"}
        </button>
        {preview ? (
          <button className="iw-button-primary" disabled={busy} onClick={save} type="button">
            <Plus className="mr-2 h-4 w-4" aria-hidden="true" />
            {busy ? "Recording..." : "Record Event"}
          </button>
        ) : null}
      </div>
    </dialog>
  );
}

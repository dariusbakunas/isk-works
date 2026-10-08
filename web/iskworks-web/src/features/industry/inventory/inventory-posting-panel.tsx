import { ArrowDown, ArrowUp, Calculator, Plus } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import {
  postInventory,
  previewInventory,
  type InventoryItem,
  type InventoryPostingInput,
  type InventoryPreview,
} from "../../../api/inventory";
import { searchTypes, type TypeSearchResult } from "../../../api/sde";
import { formatIskSummary } from "../../../components/money";
import { MoneyInput, type MoneyInputResult } from "../../../components/money-input";
import { EveTypeImage } from "../../../components/eve-type-image";
import { useDebouncedLookup } from "../../../hooks/use-debounced-lookup";
import { InlineAlert } from "../../../components/primitives";
import { apiMessage } from "./shared";

function formatQuantity(value: number) {
  return value.toLocaleString("en-US");
}

function formatMoney(value: string) {
  return formatIskSummary(value);
}

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

function PreviewMetric({ label, value }: { label: string; value: ReactNode }) {
  return <div><dt className="text-xs text-muted">{label}</dt><dd className="mt-0.5 break-words font-mono">{value}</dd></div>;
}

/** `a - b` for decimal strings, exactly (no float rounding). */
function subtractDecimal(a: string, b: string): string {
  const scale = Math.max(a.split(".")[1]?.length ?? 0, b.split(".")[1]?.length ?? 0);
  const scaled = (value: string) => {
    const [integer, fraction = ""] = value.split(".");
    const negative = integer.startsWith("-");
    const digits = BigInt(`${integer.replace(/^[+-]/, "")}${fraction.padEnd(scale, "0")}`);
    return negative ? -digits : digits;
  };
  const difference = scaled(a) - scaled(b);
  const magnitude = (difference < 0n ? -difference : difference).toString().padStart(scale + 1, "0");
  const integer = scale ? magnitude.slice(0, -scale) : magnitude;
  const fraction = scale ? `.${magnitude.slice(-scale)}` : "";
  return `${difference < 0n ? "-" : ""}${integer}${fraction}`;
}

/**
 * How this posting moves the weighted average unit cost: direction, ISK and
 * percent change, and the average it moves from. Cheaper stock pulls the
 * average down (good), dearer stock pushes it up.
 */
export function AverageCostChange({ current, resulting }: { current: string | null; resulting: string | null }) {
  if (current === null || resulting === null) {
    return <span className="block text-xs text-muted">{current === null ? "No existing stock" : "No remaining stock"}</span>;
  }
  const delta = subtractDecimal(resulting, current);
  const direction = Math.sign(Number(delta));
  if (direction === 0) return <span className="block text-xs text-muted">No change</span>;
  const percent = Number(current) === 0 ? null : (Number(delta) / Number(current)) * 100;
  const Icon = direction > 0 ? ArrowUp : ArrowDown;
  const percentText = percent === null
    ? ""
    : ` (${percent > 0 ? "+" : ""}${new Intl.NumberFormat("en-US", { maximumFractionDigits: 1 }).format(percent)}%)`;
  return (
    <>
      <span
        className={`mt-0.5 flex items-center gap-1 text-xs ${direction > 0 ? "text-warning" : "text-positive"}`}
        data-testid="average-cost-change"
      >
        <Icon aria-hidden="true" className="h-3 w-3 shrink-0" />
        <span className="sr-only">{direction > 0 ? "Increases by" : "Decreases by"}</span>
        {formatIskSummary(delta, { signDisplay: "always" })}{percentText}
      </span>
      <span className="block text-xs text-muted">from {formatMoney(current)}</span>
    </>
  );
}

function postingTitle(kind: InventoryPreview["posting"]["kind"]) {
  if (kind === "openingBalance") return "Opening Balance";
  if (kind === "purchase") return "Purchase";
  return "Adjustment";
}

export function InventoryPreviewPanel({
  preview,
  onSave,
  busy,
  actionLabel = "Record Event",
  busyLabel = "Recording...",
}: {
  preview: InventoryPreview;
  onSave: () => void;
  busy: boolean;
  actionLabel?: string;
  busyLabel?: string;
}) {
  const quantityLabel = preview.posting.quantityDelta >= 0 ? "Quantity added" : "Quantity removed";
  return (
    <div className="rounded-md border border-primary/60 bg-background p-4">
      <p className="iw-eyebrow">Posting preview</p>
      <h3 className="mt-1 font-semibold">{postingTitle(preview.posting.kind)}</h3>
      <dl className="mt-4 grid grid-cols-2 gap-3">
        <PreviewMetric label={quantityLabel} value={formatQuantity(preview.posting.quantityDelta)} />
        <PreviewMetric label="Unit cost" value={preview.posting.unitCost ? formatMoney(preview.posting.unitCost) : "Unknown"} />
        <PreviewMetric label="Historical cost added" value={formatMoney(preview.posting.totalCostDelta)} />
        <PreviewMetric label="Current revision" value={String(preview.current.revision)} />
      </dl>
      <div className="my-4 border-t border-border" />
      <h4 className="text-sm font-semibold">Resulting inventory</h4>
      <dl className="mt-2 grid grid-cols-2 gap-3">
        <PreviewMetric label="Quantity" value={formatQuantity(preview.resulting.quantity)} />
        <PreviewMetric
          label="Average cost"
          value={(
            <>
              {preview.resulting.averageUnitCost ? formatMoney(preview.resulting.averageUnitCost) : "Unavailable"}
              <AverageCostChange current={preview.current.averageUnitCost} resulting={preview.resulting.averageUnitCost} />
            </>
          )}
        />
        <PreviewMetric label="Historical cost" value={formatMoney(preview.resulting.totalHistoricalCost)} />
        <PreviewMetric label="Revision" value={String(preview.resulting.revision)} />
      </dl>
      {preview.warnings.map((warning) => <div className="mt-3" key={warning}><InlineAlert title="Cost warning" tone="info">{warning}</InlineAlert></div>)}
      <button className="iw-button-primary mt-4 w-full" disabled={busy} onClick={onSave} type="button">
        <Plus className="mr-2 h-4 w-4" aria-hidden="true" />
        {busy ? busyLabel : actionLabel}
      </button>
    </div>
  );
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
      className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(760px,calc(100vw-2rem))] overflow-y-auto p-5 text-foreground backdrop:bg-black/70"
      onCancel={(event) => { event.preventDefault(); onCancel(); }}
      ref={dialogRef}
    >
      <div className="mb-4 flex items-start justify-between gap-3">
        <div>
          <p className="iw-eyebrow">{kind === "opening" ? "Starting position" : "Inbound inventory"}</p>
          <h2 className="text-lg font-semibold" id="inventory-posting-title">{kind === "opening" ? "Add Opening Balance" : "Record Purchase"}</h2>
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
          <button className="iw-button-secondary justify-self-start" disabled={busy || unitCostResult.status === "invalid"} onClick={runPreview} type="button">
            <Calculator className="mr-2 h-4 w-4" aria-hidden="true" />
            {busy ? "Calculating..." : "Preview"}
          </button>
        </div>
        <div>
          {preview ? (
            <InventoryPreviewPanel preview={preview} onSave={save} busy={busy} />
          ) : (
            <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted">
              Enter the event details, then preview the exact inventory result before recording it.
            </div>
          )}
        </div>
      </div>
    </dialog>
  );
}

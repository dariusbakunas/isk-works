// The preview of one inventory posting (opening balance, purchase or
// adjustment) and how it moves the weighted average unit cost. Shared by
// Inventory, Finance and the Board's recording dialog.
import { ArrowDown, ArrowUp, Plus } from "lucide-react";
import type { ReactNode } from "react";

import type { InventoryPreview } from "../api/inventory";
import { formatIskSummary } from "./money";
import { InlineAlert } from "./primitives";

function formatQuantity(value: number) {
  return value.toLocaleString("en-US");
}

function formatMoney(value: string) {
  return formatIskSummary(value);
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
  /** Omitted when the host dialog keeps its record button in its footer. */
  onSave?: () => void;
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
      {onSave ? (
        <button className="iw-button-primary mt-4 w-full" disabled={busy} onClick={onSave} type="button">
          <Plus className="mr-2 h-4 w-4" aria-hidden="true" />
          {busy ? busyLabel : actionLabel}
        </button>
      ) : null}
    </div>
  );
}

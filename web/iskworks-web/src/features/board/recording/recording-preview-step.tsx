import type { RecordingPreview, RecordingPreviewLine } from "../../../api/industry";
import { Badge, InlineAlert } from "../../../components/primitives";
import { formatIskSummary } from "../../../components/money";
import { AverageCostChange } from "../../industry/inventory/inventory-posting-panel";

function Change({ label, from, to }: { label: string; from: string; to: string }) {
  return (
    <div className="flex justify-between gap-2">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 text-right font-mono tabular-nums text-foreground">
        {from} <span className="text-muted">→</span> {to}
      </dd>
    </div>
  );
}

function PreviewLine({ line }: { line: RecordingPreviewLine }) {
  const { current, resulting } = line;
  return (
    <li className="space-y-1 py-2">
      <div className="flex items-center gap-2">
        <Badge square tone={line.effect === "added" ? "positive" : "muted"}>
          {line.effect === "added" ? "Added" : "Consumed"}
        </Badge>
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">{line.typeName}</span>
      </div>
      <dl className="space-y-0.5 text-[11px]">
        <Change
          from={current.quantity.toLocaleString()}
          label="Quantity"
          to={resulting.quantity.toLocaleString()}
        />
        <Change
          from={formatIskSummary(current.totalHistoricalCost)}
          label="Historical cost"
          to={formatIskSummary(resulting.totalHistoricalCost)}
        />
        <div className="flex justify-between gap-2">
          <dt className="text-muted">Average cost</dt>
          <dd className="flex min-w-0 flex-1 flex-col items-end font-mono tabular-nums text-foreground">
            {resulting.averageUnitCost ? formatIskSummary(resulting.averageUnitCost) : "—"}
            <AverageCostChange current={current.averageUnitCost} resulting={resulting.averageUnitCost} />
          </dd>
        </div>
      </dl>
    </li>
  );
}

// Step two of recording an outcome: what it does to each inventory item,
// before Record posts it. Back returns to the entered values unchanged.
export function RecordingPreviewStep({
  preview,
  busy,
  error,
  onBack,
  onConfirm,
}: {
  preview: RecordingPreview;
  busy: boolean;
  error: string;
  onBack: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="space-y-2">
      <p className="text-[10px] font-semibold uppercase tracking-wide text-muted">Inventory after recording</p>
      <ul className="divide-y divide-border/70 border-y border-border/70" data-private="">
        {preview.lines.map((line) => (
          <PreviewLine key={`${line.effect}:${line.typeId}`} line={line} />
        ))}
      </ul>
      {error ? <InlineAlert title="Recording did not finish">{error}</InlineAlert> : null}
      <div className="flex justify-end gap-2">
        <button className="iw-button-secondary" disabled={busy} onClick={onBack} type="button">
          Back
        </button>
        <button className="iw-button-primary" disabled={busy} onClick={onConfirm} type="button">
          {busy ? "Recording…" : "Record"}
        </button>
      </div>
    </div>
  );
}

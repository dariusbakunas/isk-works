import { PackagePlus, RotateCcw, Scale, Search, ShoppingCart } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import {
  getInventoryItem,
  reverseInventoryEvent,
  type InventoryEvent,
  type InventoryItem,
  type InventoryReservation,
} from "../../../api/inventory";
import { formatIskSummary, MoneyAmount } from "../../../components/money";
import { ConfirmDialog, EqRow, InlineAlert, StatusBadge } from "../../../components/primitives";
import { PlannerInspectorShell } from "../../../components/planner-inspector-shell";
import { InventoryReservationsTab } from "./inventory-reservations-tab";
import { rowKey } from "./inventory-operational-table";
import { apiMessage } from "./shared";

type DetailState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; events: InventoryEvent[]; reservations: InventoryReservation[] };

type InspectorTab = "overview" | "reservations" | "history";

const TABS: { id: InspectorTab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "reservations", label: "Reservations" },
  { id: "history", label: "History" },
];

export function InventoryInspector({
  item,
  onChanged,
  onClose,
  onOpenPosting,
  onOpenAdjustment,
  onViewEsiHoldings,
  priceSourceId,
}: {
  item: InventoryItem;
  onChanged: () => void;
  onClose: () => void;
  onOpenPosting: (kind: "opening" | "purchase") => void;
  onOpenAdjustment: () => void;
  onViewEsiHoldings: () => void;
  priceSourceId?: string;
  /**
   * Bumped by the parent (never the same value twice) to say "open the
   * Review Discrepancy panel for whichever item is selected right now" --
   * how the ESI holdings modal's "Review adjustment" button reuses this
   * flow instead of duplicating it, including when the modal was opened
   * for the item that's already selected (so `typeId` alone wouldn't
   * change to trigger anything).
   */
}) {
  const typeId = item.balance.key.typeId;
  const [detail, setDetail] = useState<DetailState>({ status: "loading" });
  const [tab, setTab] = useState<InspectorTab>("overview");
  const [reversing, setReversing] = useState(false);
  const [reason, setReason] = useState("");
  const [reversalError, setReversalError] = useState("");

  function loadDetail() {
    setDetail({ status: "loading" });
    getInventoryItem(typeId, priceSourceId)
      .then((item) => setDetail({ status: "ready", events: item.events, reservations: item.reservations }))
      .catch((error) => setDetail({ status: "error", message: apiMessage(error) }));
  }
  useEffect(loadDetail, [typeId, priceSourceId]);
  useEffect(() => {
    setTab("overview");
    setReversalError("");
    setReversing(false);
    setReason("");
  }, [typeId]);

  const events = detail.status === "ready" ? detail.events : [];
  const reservations = detail.status === "ready" ? detail.reservations : [];
  const latest = events.at(-1);

  async function reverse() {
    if (!latest || !reason.trim()) return;
    setReversalError("");
    try {
      await reverseInventoryEvent(typeId, latest.id, item.balance.revision, reason);
      setReversing(false);
      setReason("");
      loadDetail();
      onChanged();
    } catch (error) {
      setReversalError(apiMessage(error));
      setReversing(false);
    }
  }

  return (
    <PlannerInspectorShell onClose={onClose} open returnFocusRowKey={rowKey(item)} title={item.balance.typeName}>
      <div className="px-3 pb-2">
        {item.warnings.map((warning) => (
          <div className="pt-3" key={warning}><InlineAlert title="Inventory warning" tone="info">{warning}</InlineAlert></div>
        ))}
        {reversalError ? <div className="pt-3"><InlineAlert title="Event was not reversed">{reversalError}</InlineAlert></div> : null}

        <div aria-label="Inventory item detail" className="mt-3 flex gap-1 border-b border-border" role="tablist">
          {TABS.map((option) => (
            <button
              aria-selected={tab === option.id}
              className={`relative -mb-px flex items-center gap-1.5 border-b-2 px-2 py-1.5 text-xs font-semibold ${
                tab === option.id ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
              }`}
              key={option.id}
              onClick={() => setTab(option.id)}
              role="tab"
              type="button"
            >
              {option.label}
              {option.id === "reservations" && reservations.length > 0 ? (
                <StatusBadge>{reservations.length}</StatusBadge>
              ) : null}
            </button>
          ))}
        </div>
      </div>

      {tab === "overview" ? (
        <div className="px-3 pb-2">
          <InspectorSection label="Balance">
            <dl className="grid grid-cols-2 gap-x-3 gap-y-1.5">
              <InspectorMetric label="Owned" value={item.balance.quantity.toLocaleString()} />
              <InspectorMetric label="Reserved" value={item.reservedQuantity.toLocaleString()} />
              <InspectorMetric label="Available" value={item.availableQuantity.toLocaleString()} />
            </dl>
          </InspectorSection>

          <InspectorSection label="Cost">
            <InspectorLine label="Average cost" value={item.balance.averageUnitCost ? formatIskSummary(item.balance.averageUnitCost) : "Unavailable"} />
            <InspectorLine label="Historical cost" value={formatIskSummary(item.balance.totalHistoricalCost)} />
            <InspectorLine label="Current value" value={item.currentValue ? formatIskSummary(item.currentValue) : "No price"} />
            <InspectorLine label="Difference" value={item.historicalDifference ? formatIskSummary(item.historicalDifference) : "Incomplete"} />
          </InspectorSection>

          <InspectorSection label="ESI Observation">
            {item.esiObservedQuantity == null ? (
              <p className="text-xs text-muted">No ESI observation exists for this item yet.</p>
            ) : (
              <>
                <EqRow label="ESI observed" value={item.esiObservedQuantity.toLocaleString()} />
                {(item.ignoredEsiQuantity ?? 0) > 0 ? <EqRow label="Ignored" value={(item.ignoredEsiQuantity ?? 0).toLocaleString()} /> : null}
                <EqRow label="Included" value={(item.includedEsiQuantity ?? item.esiObservedQuantity).toLocaleString()} />
                <EqRow label="Accounting owned" value={item.balance.quantity.toLocaleString()} />
                <EqRow
                  emphasis
                  label="Difference"
                  tone={(item.reconciliationDifference ?? 0) === 0 ? "positive" : "warning"}
                  value={
                    (item.reconciliationDifference ?? 0) === 0
                      ? "Match"
                      : signedQuantity(item.reconciliationDifference ?? 0)
                  }
                />
                {item.esiObservedAt ? (
                  <p className="mt-1.5 text-xs text-muted">
                    As of {formatDate(item.esiObservedAt)}, workspace-wide across every connected character,
                    excluding fitted ships and blueprint copies.
                  </p>
                ) : null}
                <button className="iw-button-secondary mt-2 w-full" onClick={onViewEsiHoldings} type="button">
                  <Search aria-hidden="true" className="mr-2 h-4 w-4" />View ESI holdings
                </button>
              </>
            )}
          </InspectorSection>

          <InspectorSection label="Explainability">
            <details>
              <summary className="cursor-pointer text-xs font-semibold">
                Why is the average cost {item.balance.averageUnitCost ? formatIskSummary(item.balance.averageUnitCost) : "unavailable"}?
              </summary>
              <div className="mt-2 text-xs">
                <p className="font-mono">
                  <MoneyAmount mode="detail" value={item.balance.totalHistoricalCost} /> total historical cost ÷ {item.balance.quantity.toLocaleString()} units = <MoneyAmount mode="detail" value={item.balance.averageUnitCost ?? "0"} />
                </p>
                <p className="iw-muted mt-2">Recorded sequence is authoritative. Effective dates are displayed for context and do not reorder accounting history.</p>
              </div>
            </details>
          </InspectorSection>

          <InspectorSection label="Actions">
            <div className="grid gap-1.5">
              <button className="iw-button-primary" onClick={() => onOpenPosting("purchase")} type="button">
                <ShoppingCart aria-hidden="true" className="mr-2 h-4 w-4" />Record Purchase
              </button>
              <button className="iw-button-secondary" onClick={() => onOpenPosting("opening")} type="button">
                <PackagePlus aria-hidden="true" className="mr-2 h-4 w-4" />Opening Balance
              </button>
              <button className="iw-button-secondary" onClick={onOpenAdjustment} type="button">
                <Scale aria-hidden="true" className="mr-2 h-4 w-4" />Adjust Inventory
              </button>
            </div>
          </InspectorSection>
        </div>
      ) : null}

      {tab === "reservations" ? (
        detail.status === "loading" ? (
          <p className="px-3 py-3 text-xs text-muted" role="status">Loading reservations...</p>
        ) : detail.status === "error" ? (
          <div className="px-3 py-3"><InlineAlert title="Reservations unavailable">{detail.message}</InlineAlert></div>
        ) : (
          <InventoryReservationsTab item={item} reservations={reservations} />
        )
      ) : null}

      {tab === "history" ? (
        <div className="px-3 pb-2">
          <InspectorSection label="Event History">
            {detail.status === "loading" ? <p className="text-xs text-muted" role="status">Loading events...</p> : null}
            {detail.status === "error" ? <InlineAlert title="Event history unavailable">{detail.message}</InlineAlert> : null}
            {detail.status === "ready" ? (
              <div>
                {[...events].reverse().map((event, index) => (
                  <div className="border-b border-border py-2 last:border-b-0" key={event.id}>
                    <div className="flex flex-wrap items-center gap-1.5">
                      <strong className="text-xs">{eventLabel(event.kind)}</strong>
                      {event.costQuality === "estimated" ? <StatusBadge>Estimated cost</StatusBadge> : null}
                      {event.costQuality === "zeroCost" ? <StatusBadge>Zero cost</StatusBadge> : null}
                      {event.reversedByEventId ? <StatusBadge>Reversed</StatusBadge> : null}
                    </div>
                    <p className="mt-1 text-xs text-muted">
                      {signedQuantity(event.quantityDelta)} · <MoneyAmount mode="detail" value={event.totalCostDelta} /> total
                    </p>
                    <div className="mt-1.5">
                      <InspectorLine label="Unit cost" value={event.unitCost ? formatIskSummary(event.unitCost) : "Unknown"} />
                      <InspectorLine label="Effective" value={formatDate(event.effectiveAt)} />
                      <InspectorLine label="Recorded" value={formatDate(event.recordedAt)} />
                      <InspectorLine label="Resulting average" value={event.resultingBalance.averageUnitCost ? formatIskSummary(event.resultingBalance.averageUnitCost) : "Unavailable"} />
                    </div>
                    {event.sourceReference || event.note ? (
                      <p className="mt-1 text-xs text-muted" data-private="">{[event.sourceReference, event.note].filter(Boolean).join(" · ")}</p>
                    ) : null}
                    {index === 0 && event.kind !== "reversal" && event.kind !== "consumption" && event.kind !== "productionOutput" && !event.reversedByEventId ? (
                      <button className="iw-button-danger mt-2 w-full" onClick={() => setReversing(true)} type="button">
                        <RotateCcw aria-hidden="true" className="mr-2 h-4 w-4" />Reverse Event
                      </button>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : null}
          </InspectorSection>
        </div>
      ) : null}

      <ConfirmDialog confirmLabel="Reverse Event" onCancel={() => setReversing(false)} onConfirm={reverse} open={reversing} title="Reverse Event">
        <span className="block">The original event remains visible. Only the latest event can be reversed in this version.</span>
        <label className="mt-3 block text-sm font-semibold text-foreground">Reversal reason
          <textarea className="iw-input mt-1 min-h-20" onChange={(event) => setReason(event.target.value)} value={reason} />
        </label>
      </ConfirmDialog>
    </PlannerInspectorShell>
  );
}

function InspectorSection({ children, label }: { children: ReactNode; label: string }) {
  return (
    <section aria-label={label} className="border-b border-border py-2">
      <h3 className="mb-1 text-[10px] font-semibold uppercase text-muted">{label}</h3>
      {children}
    </section>
  );
}

function InspectorMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-[10px] leading-4 text-muted">{label}</dt>
      <dd className="truncate text-right font-mono text-xs font-semibold tabular-nums" title={value}>{value}</dd>
    </div>
  );
}

function InspectorLine({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-start justify-between gap-3 py-1 text-xs">
      <span className="text-muted">{label}</span>
      <strong className="text-right font-mono">{value}</strong>
    </div>
  );
}

function eventLabel(kind: InventoryEvent["kind"]) {
  return kind === "openingBalance" ? "Opening Balance" : kind === "purchase" ? "Purchase" : kind === "consumption" ? "Consumed by Build" : kind === "productionOutput" ? "Production Output" : kind === "adjustment" ? "Adjustment" : "Reversal";
}

function signedQuantity(value: number) {
  const formatted = value.toLocaleString();
  return `${value > 0 ? "+" : ""}${formatted} units`;
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat("en-US", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

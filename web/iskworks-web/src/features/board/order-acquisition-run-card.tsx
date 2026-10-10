import { MapPin, Tag } from "lucide-react";

import type { AcquisitionRun, Ticket } from "../../api/industry";
import { Badge, ProgressBar } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { sumDecimalStrings } from "../industry/shared/decimal";
import { acquisitionPricingIdentity } from "./acquisition-pricing";
import { acquisitionRunStatusMeta } from "./ticket-meta";
import { consolidateOrderTicketsByType } from "./order-ticket-consolidation";
import { useAcquisitionPricingDescriptor } from "./use-acquisition-pricing-descriptor";
import { ACTIVE_CARD_CLASS, useScrollIntoViewWhenActive } from "./active-card";

const ITEM_PREVIEW_LIMIT = 3;

// Standalone-ticket counterpart to AcquisitionRunCard -- shows the run's
// acquisition pricing identity (market scope, or manual price list) in
// place of a plan count, since a run's member tickets are already
// guaranteed to share one identity (possibly drawn from several different
// Orders, which is expected -- see order-acq-group.ts).
export function OrderAcquisitionRunCard({
  run,
  tickets,
  priceSourceName,
  active = false,
  onOpen,
}: {
  run: AcquisitionRun;
  /** The run the Board's drawer is showing. */
  active?: boolean;
  tickets: Ticket[];
  /** Resolved name of the run's manual price list, when it has one. */
  priceSourceName?: string | null;
  onOpen?: (runId: string) => void;
}) {
  const status = acquisitionRunStatusMeta[run.status];
  const descriptor = useAcquisitionPricingDescriptor(acquisitionPricingIdentity(run), priceSourceName);
  const items = consolidateOrderTicketsByType(tickets);
  const itemCount = items.length;
  const totalCost = sumDecimalStrings(tickets.map((ticket) => ticket.estimatedLineTotal)).value;
  const fullyAcquired = tickets.filter(
    (ticket) => (ticket.acquiredQuantity ?? 0) >= (ticket.quantity ?? 0),
  ).length;
  const percentAcquired = tickets.length === 0 ? 0 : (fullyAcquired / tickets.length) * 100;
  const isComplete = run.status === "complete";
  const previewItems = items.slice(0, ITEM_PREVIEW_LIMIT);
  const remainingItems = items.length - previewItems.length;
  const ref = useScrollIntoViewWhenActive<HTMLButtonElement>(active);

  return (
    <button
      aria-current={active ? "true" : undefined}
      className={`relative block w-full overflow-hidden rounded-[2px] border bg-panel-strong py-2 pl-4 pr-2.5 text-left text-sm hover:border-batch/70 ${isComplete ? "border-border opacity-70" : "border-batch/40"} ${active ? ACTIVE_CARD_CLASS : ""}`}
      ref={ref}
      onClick={() => onOpen?.(run.id)}
      type="button"
    >
      <span aria-hidden="true" className={`absolute inset-y-0 left-0 w-[5px] ${isComplete ? "bg-muted" : "bg-batch"}`} />
      <div className="flex items-center gap-2">
        <span className={`font-mono text-[10px] font-medium ${isComplete ? "text-muted" : "text-batch"}`}>
          {run.displayId}
        </span>
        <Badge square tone="batch">
          ACQ RUN
        </Badge>
        <span className="ml-auto">
          <Badge square tone={status.tone}>
            {status.label}
          </Badge>
        </span>
      </div>
      <p className={`mt-1 truncate text-xs font-semibold ${isComplete ? "text-muted" : "text-foreground"}`}>
        {run.name}
      </p>
      <p className="mt-1 flex items-center gap-1.5 text-[10px] text-muted">
        <span>{itemCount} items</span>
        <span className="text-border">·</span>
        <span>{tickets.length} tickets</span>
        <span className="text-border">·</span>
        <span className="flex min-w-0 items-center gap-1 truncate">
          {descriptor.icon === "location" ? (
            <MapPin aria-hidden="true" size={9} />
          ) : descriptor.icon === "list" ? (
            <Tag aria-hidden="true" size={9} />
          ) : null}
          <span className="truncate">{descriptor.label}</span>
        </span>
      </p>

      {run.status === "inProgress" ? (
        <div className="mt-2 space-y-1">
          <div className="flex items-center justify-between text-[10px]">
            <span className="text-muted">
              {fullyAcquired}/{itemCount} items fully acquired
            </span>
            <span className="font-mono tabular-nums text-warning">{Math.round(percentAcquired)}%</span>
          </div>
          <ProgressBar percent={percentAcquired} tone="warning" />
        </div>
      ) : null}

      {isComplete ? (
        <p className="mt-2 text-[10px] text-positive">
          All {itemCount} items acquired · {tickets.length} tickets closed
        </p>
      ) : null}

      {run.status === "ready" ? (
        <>
          <div className="mt-1.5 h-px bg-border/50" />
          <div className="mt-1.5 space-y-0.5">
            {previewItems.map((item) => (
              <div className="flex items-baseline justify-between gap-2 text-[10px]" key={item.typeId}>
                <span className="min-w-0 truncate text-muted">{item.capturedName}</span>
                <span className="shrink-0 font-mono tabular-nums text-foreground">
                  {item.neededQuantity.toLocaleString()}
                </span>
              </div>
            ))}
            {remainingItems > 0 ? (
              <p className="text-[10px] text-muted">+ {remainingItems} more items</p>
            ) : null}
          </div>
        </>
      ) : null}

      {totalCost ? (
        <p className={`mt-2 border-t border-border pt-1.5 font-mono text-[11px] font-medium ${isComplete ? "text-muted" : "text-warning"}`}>
          <MoneyAmount value={totalCost} />
        </p>
      ) : null}
    </button>
  );
}

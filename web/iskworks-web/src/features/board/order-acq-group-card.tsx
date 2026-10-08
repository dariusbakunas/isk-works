import { ChevronDown, ChevronUp, MapPin, Tag } from "lucide-react";
import { useEffect, useRef } from "react";

import type { TicketStatus } from "../../api/industry";
import { Badge, type Tone } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { sumDecimalStrings } from "../industry/shared/decimal";
import { acquisitionPricingIdentity, type AcquisitionPricingIcon } from "./acquisition-pricing";
import type { OrderAcquisitionGroup } from "./order-acq-group";
import { isBatchableOrderTicket } from "./order-ticket-card";
import { isTicketDraggable, TICKET_DRAG_MIME } from "./ticket-drag";
import { useAcquisitionPricingDescriptor } from "./use-acquisition-pricing-descriptor";

function PricingIcon({ icon }: { icon: AcquisitionPricingIcon }) {
  if (icon === "location") return <MapPin aria-hidden="true" size={9} />;
  if (icon === "list") return <Tag aria-hidden="true" size={9} />;
  return null;
}

const PREVIEW_LIMIT = 3;

const laneLabel: Record<TicketStatus, string> = {
  todo: "To Do",
  inProgress: "In Progress",
  complete: "Complete",
  canceled: "Canceled",
};

// One Board lane's price-source band for its unbatched standalone
// Acquisition tickets, grouped by the real batching-compatibility key
// rather than by Order (see order-acq-group.ts). Membership is derived, not persisted:
// every member shares this card's `status` (the caller passes one lane's
// slice already).
export function OrderAcqGroupCard({
  group,
  priceSourceName,
  expanded,
  onToggleExpand,
  selecting = false,
  selectedIds,
  onToggleSelect,
  onSelectAll,
  onOpenTicket,
  onTicketDragStart,
  onTicketDragEnd,
}: {
  group: OrderAcquisitionGroup;
  /** Resolved name of the group's manual price list, when it has one --
   * market-scope groups resolve their own region/location name internally
   * and ignore this. */
  priceSourceName?: string | null;
  expanded: boolean;
  onToggleExpand: () => void;
  selecting?: boolean;
  selectedIds?: Set<string>;
  onToggleSelect?: (ticketId: string) => void;
  onSelectAll?: (ticketIds: string[]) => void;
  onOpenTicket?: (ticketId: string) => void;
  onTicketDragStart?: (ticketId: string) => void;
  onTicketDragEnd?: () => void;
}) {
  const status = group.tickets[0]?.status ?? "todo";
  const isComplete = status === "complete";
  const tone: Tone = isComplete ? "positive" : "primary";
  const count = group.tickets.length;

  const identity = acquisitionPricingIdentity(group);
  const descriptor = useAcquisitionPricingDescriptor(identity, priceSourceName);

  // A `none` group (no market scope, no price list) is not batch-compatible
  // per the backend `BatchKey` -- never offer a select/batch affordance for
  // it, even though its member tickets are individually "batchable" by
  // kind/status.
  const selectableIds = group.batchable
    ? group.tickets.filter(isBatchableOrderTicket).map((ticket) => ticket.id)
    : [];
  const selectedCount = selectedIds ? selectableIds.filter((id) => selectedIds.has(id)).length : 0;
  const allSelected = selectableIds.length > 0 && selectedCount === selectableIds.length;
  const someSelected = selectedCount > 0 && !allSelected;

  const headerCheckboxRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (headerCheckboxRef.current) headerCheckboxRef.current.indeterminate = someSelected;
  }, [someSelected]);

  const previewTickets = group.tickets.slice(0, PREVIEW_LIMIT);
  const remainingPreview = group.tickets.length - previewTickets.length;
  // `actualLineTotal` is only present on tickets completed before completion
  // stopped pricing anything; prefer it when present, otherwise the
  // estimate.
  const totalCost = sumDecimalStrings(
    group.tickets.map((ticket) => ticket.actualLineTotal ?? ticket.estimatedLineTotal),
  ).value;

  return (
    <div>
      <div
        className={`relative overflow-hidden rounded-[2px] border bg-panel-strong py-2 pl-3.5 pr-2 text-sm ${isComplete ? "border-border opacity-70" : "border-primary/40"}`}
        onClick={selecting ? undefined : onToggleExpand}
        onKeyDown={
          selecting
            ? undefined
            : (event) => {
                if (event.key === "Enter" || event.key === " ") onToggleExpand();
              }
        }
        role={selecting ? undefined : "button"}
        tabIndex={selecting ? undefined : 0}
      >
        <span aria-hidden="true" className={`absolute inset-y-0 left-0 w-[3px] ${isComplete ? "bg-muted" : "bg-primary"}`} />
        <div className="flex items-center gap-1.5">
          {selecting && selectableIds.length > 0 ? (
            <input
              aria-label={`Select all ${descriptor.label} ${laneLabel[status]} materials`}
              checked={allSelected}
              className="h-3.5 w-3.5"
              onChange={() => onSelectAll?.(selectableIds)}
              onClick={(event) => event.stopPropagation()}
              ref={headerCheckboxRef}
              type="checkbox"
            />
          ) : null}
          <Badge square tone={tone}>
            {isComplete ? "ACQUIRED" : "ACQ GROUP"}
          </Badge>
          <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-foreground">
            {descriptor.label}
          </span>
          <button
            aria-expanded={expanded}
            aria-label={`${expanded ? "Collapse" : "Expand"} ${descriptor.label} ${laneLabel[status]} group`}
            className="shrink-0 text-muted hover:text-foreground"
            onClick={(event) => {
              event.stopPropagation();
              onToggleExpand();
            }}
            type="button"
          >
            {expanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
          </button>
        </div>
        <p className="mt-0.5 text-[10px] text-muted">
          {count} material{count === 1 ? "" : "s"}
        </p>

        {!group.batchable ? (
          <p className="mt-0.5 text-[10px] text-warning">
            No market scope or price list — can’t be added to an Acquisition Run.
          </p>
        ) : null}

        {!expanded ? (
          <>
            <div className="mt-1.5 space-y-0.5">
              {previewTickets.map((ticket) => (
                <div className="flex items-baseline justify-between gap-2 text-[10px]" key={ticket.id}>
                  <span className="min-w-0 truncate text-muted">{ticket.capturedName}</span>
                  <span className="shrink-0 font-mono tabular-nums text-foreground">
                    ×{(ticket.quantity ?? 0).toLocaleString()}
                  </span>
                </div>
              ))}
              {remainingPreview > 0 ? <p className="text-[10px] text-muted">+ {remainingPreview} more</p> : null}
            </div>
            <p className="mt-1 flex items-center gap-1 text-[10px] text-muted">
              <PricingIcon icon={descriptor.icon} />
              {descriptor.label}
            </p>
          </>
        ) : null}

        {totalCost ? (
          <p className={`mt-1.5 border-t border-border pt-1.5 font-mono text-[11px] font-medium ${isComplete ? "text-muted" : "text-warning"}`}>
            <MoneyAmount value={totalCost} />
          </p>
        ) : null}
      </div>

      {expanded ? (
        <div className="ml-2 mt-1 space-y-1.5 border-l border-border pl-2">
          <div className="flex items-center justify-between py-0.5">
            <span className="flex items-center gap-1 text-[10px] text-muted">
              <PricingIcon icon={descriptor.icon} />
              {descriptor.label}
            </span>
            {selecting && selectableIds.length > 0 ? (
              <button
                className="text-[10px] text-primary hover:opacity-80"
                onClick={() => onSelectAll?.(selectableIds)}
                type="button"
              >
                Select {selectableIds.length} actionable
              </button>
            ) : null}
          </div>
          {group.tickets.map((ticket) => {
            // Rendered inline here rather than reusing OrderTicketCard's
            // own component to keep this preview compact (no kind badge --
            // every member here is Acquisition by construction). Selection
            // still shares the exact same eligibility rule and handlers.
            const selected = selectedIds?.has(ticket.id) ?? false;
            // A `none` group can't be batched at all -- suppress the whole
            // selection affordance for its rows (not just disable it).
            const batchable = group.batchable && isBatchableOrderTicket(ticket);
            const draggable = !selecting && isTicketDraggable(ticket);
            return (
              <div
                className={`relative overflow-hidden rounded-[2px] border border-border bg-panel py-1.5 pl-3 pr-1.5 text-sm ${selecting && !batchable ? "opacity-50" : ""} ${draggable ? "cursor-grab active:cursor-grabbing" : ""}`}
                draggable={draggable}
                key={ticket.id}
                onClick={selecting ? undefined : () => onOpenTicket?.(ticket.id)}
                onDragEnd={draggable ? () => onTicketDragEnd?.() : undefined}
                onDragStart={
                  draggable
                    ? (event) => {
                        if (event.dataTransfer) {
                          event.dataTransfer.setData(TICKET_DRAG_MIME, ticket.id);
                          event.dataTransfer.effectAllowed = "move";
                        }
                        onTicketDragStart?.(ticket.id);
                      }
                    : undefined
                }
                role={selecting ? undefined : "button"}
                tabIndex={selecting ? undefined : 0}
              >
                <div className="flex items-center gap-1.5">
                  {selecting && group.batchable ? (
                    <input
                      aria-label={`Select ${ticket.displayId}`}
                      checked={selected}
                      className="h-3.5 w-3.5"
                      disabled={!batchable}
                      onChange={() => onToggleSelect?.(ticket.id)}
                      onClick={(event) => event.stopPropagation()}
                      type="checkbox"
                    />
                  ) : null}
                  <span className="font-mono text-[10px] text-muted">{ticket.displayId}</span>
                </div>
                <p className="mt-1 truncate text-[11px] font-medium text-foreground">{ticket.capturedName}</p>
                <p className="mt-0.5 font-mono text-[11px] tabular-nums text-muted">
                  ×{(ticket.quantity ?? 0).toLocaleString()}
                </p>
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

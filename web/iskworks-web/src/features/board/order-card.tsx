import type { OrderSummary } from "../../api/industry";
import { Badge, ProgressBar } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { orderStatusMeta } from "./order-meta";
import { ACTIVE_CARD_CLASS, useScrollIntoViewWhenActive } from "./active-card";

// An Epic (backend: Order) opens the Epic Inspector in the Board's own
// right rail rather than navigating to a dedicated page -- the Board is the
// workspace; inspecting never leaves it (only the inspector's explicit
// "Open build" action navigates). Deliberately a distinct component from
// OrderTicketCard, not a `Ticket` with an `isOrder` flag: Order and Ticket
// stay genuinely separate types on the Board, sharing only the same
// derived status/lane vocabulary.
export function OrderCard({
  order,
  active = false,
  onOpen,
}: {
  order: OrderSummary;
  /** The Epic the Board's inspector is showing. */
  active?: boolean;
  onOpen?: (orderId: string) => void;
}) {
  const ref = useScrollIntoViewWhenActive<HTMLDivElement>(active);
  const status = orderStatusMeta[order.status];
  const percentSatisfied = order.rollup.total === 0 ? 100 : (order.rollup.satisfied / order.rollup.total) * 100;
  const isComplete = order.status === "complete";

  return (
    <div
      aria-current={active ? "true" : undefined}
      className={`relative overflow-hidden rounded-[2px] border bg-panel-strong py-2 pl-4 pr-2.5 text-sm hover:border-primary/70 ${isComplete ? "border-border opacity-70" : "border-primary/40"} ${active ? ACTIVE_CARD_CLASS : ""}`}
      ref={ref}
      onClick={() => onOpen?.(order.id)}
      role="button"
      tabIndex={0}
    >
      <span aria-hidden="true" className={`absolute inset-y-0 left-0 w-[5px] ${isComplete ? "bg-muted" : "bg-primary"}`} />
      <div className="flex items-center gap-2">
        <Badge square tone="primary">
          EPIC
        </Badge>
        {order.archivedAt ? (
          <Badge square tone="muted">
            Archived
          </Badge>
        ) : null}
        <span className="ml-auto">
          <Badge square tone={status.tone}>
            {status.label}
          </Badge>
        </span>
      </div>
      <p className={`mt-1 truncate text-xs font-semibold ${isComplete ? "text-muted" : "text-foreground"}`}>
        {order.displayName}
      </p>
      <p className="mt-1 flex items-center gap-1.5 text-[10px] text-muted">
        <span>Runs {order.runs.toLocaleString()}</span>
        <span className="text-border">·</span>
        <span>
          {order.rollup.satisfied}/{order.rollup.total} satisfied
        </span>
        {order.rollup.needsAction > 0 ? (
          <>
            <span className="text-border">·</span>
            <span className="text-danger">{order.rollup.needsAction} need action</span>
          </>
        ) : null}
      </p>

      {!isComplete ? (
        <div className="mt-2 space-y-1">
          <ProgressBar percent={percentSatisfied} tone={order.rollup.needsAction > 0 ? "danger" : "positive"} />
        </div>
      ) : (
        <p className="mt-2 text-[10px] text-positive">All {order.rollup.total} dependencies satisfied</p>
      )}

      {order.estimatedMaterialCost ? (
        <p className={`mt-2 border-t border-border pt-1.5 font-mono text-[11px] font-medium ${isComplete ? "text-muted" : "text-warning"}`}>
          <MoneyAmount value={order.estimatedMaterialCost} />
        </p>
      ) : null}
    </div>
  );
}

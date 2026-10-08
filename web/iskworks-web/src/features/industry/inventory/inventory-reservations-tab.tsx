import { Link } from "react-router";

import type { InventoryItem, InventoryReservation, OrderReservationStatus } from "../../../api/inventory";
import { Badge, EmptyState, InlineAlert, StatusDot, type Tone } from "../../../components/primitives";
import { orderTicketStatusMeta } from "../../board/order-meta";

const orderReservationStatusMeta: Record<OrderReservationStatus, { label: string; tone: Tone }> = {
  notStarted: { label: "Not started", tone: "muted" },
  inProgress: { label: "In Progress", tone: "primary" },
  complete: { label: "Complete", tone: "positive" },
  canceled: { label: "Canceled", tone: "muted" },
};

export function InventoryReservationsTab({
  item,
  reservations,
}: {
  item: InventoryItem;
  reservations: InventoryReservation[];
}) {
  if (reservations.length === 0) {
    return (
      <div className="px-3 py-3">
        <EmptyState title="No active reservations">
          Nothing is currently reserving this item against an Order or Ticket.
        </EmptyState>
      </div>
    );
  }
  return (
    <div className="grid gap-2 px-3 py-3">
      <p className="text-xs text-muted">
        <span className="font-mono font-semibold text-foreground">{item.reservedQuantity.toLocaleString()}</span> reserved
      </p>
      <div className="grid gap-2">
        {reservations.map((reservation) => (
          <ReservationCard key={reservation.allocationId} reservation={reservation} />
        ))}
      </div>
      {item.availableQuantity < 0 ? (
        <InlineAlert title={`Shortfall: ${Math.abs(item.availableQuantity).toLocaleString()} units`} tone="warning">
          Reservations exceed owned quantity. Owned {item.balance.quantity.toLocaleString()}, reserved{" "}
          {item.reservedQuantity.toLocaleString()}.
        </InlineAlert>
      ) : null}
    </div>
  );
}

function ReservationCard({ reservation }: { reservation: InventoryReservation }) {
  const { source } = reservation;
  const meta = source.kind === "order" ? orderReservationStatusMeta[source.status] : orderTicketStatusMeta[source.status];
  const reference = source.kind === "order" ? source.displayName : source.displayId;
  const destination = source.kind === "order" ? `/orders/${source.orderId}` : `/board?ticket=${source.ticketId}`;
  return (
    <div className="rounded-md border border-border bg-panel-strong p-2">
      <div className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          <Badge square tone={source.kind === "order" ? "primary" : "warning"}>
            {source.kind === "order" ? "ORD" : "TKT"}
          </Badge>
          <span className="truncate text-xs font-medium" title={reference}>{reference}</span>
        </span>
        <span className="whitespace-nowrap font-mono text-xs font-semibold text-warning">
          {reservation.quantity.toLocaleString()}
        </span>
      </div>
      <div className="mt-1.5 flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 text-xs text-muted">
          <StatusDot tone={meta.tone} />
          {meta.label}
        </span>
        <Link className="text-xs text-primary hover:underline" to={destination}>
          Navigate to {source.kind === "order" ? "order" : "ticket"} →
        </Link>
      </div>
    </div>
  );
}

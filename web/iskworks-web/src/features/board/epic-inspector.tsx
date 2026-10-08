import { useEffect, useState } from "react";

import {
  archiveOrder,
  bulkCreateTickets,
  cancelOrder,
  completeOrder,
  createTicketForRequirement,
  deleteOrder,
  getOrder,
  restoreOrder,
  startOrder,
  type OrderDetail,
  type OrderSummary,
  type TicketSummary,
} from "../../api/industry";
import { Badge, ButtonLink, ConfirmDialog, InlineAlert, ProgressBar } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { apiMessage } from "../industry/shared/api-error";
import { formatDate } from "../industry/shared/formatting";
import { useAsyncAction } from "../../hooks/use-async-action";
import {
  orderStatusMeta,
  orderTicketKindMeta,
  orderTicketStatusMeta,
  requirementKindMeta,
} from "./order-meta";
import { recordingStateMeta } from "./recording/recording-meta";

function SectionHeading({ children }: { children: React.ReactNode }) {
  return <h3 className="text-[10px] font-semibold uppercase tracking-wide text-muted">{children}</h3>;
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex justify-between gap-2">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 truncate text-right text-foreground">{children}</dd>
    </div>
  );
}

// A ticket linked to this Epic, resolved against Board's already-loaded
// TicketSummary list -- a compact row, not a reproduction of OrderTicketCard.
// Clicking pushes the Ticket Inspector on top of this one (see board-page's
// tiny in-rail navigation stack); it never navigates.
function LinkedTicketRow({ onOpen, ticket }: { onOpen: (ticketId: string) => void; ticket: TicketSummary }) {
  const kindMeta = orderTicketKindMeta[ticket.kind];
  const statusMeta = orderTicketStatusMeta[ticket.status];
  const recording = ticket.recording ? recordingStateMeta[ticket.recording.state] : null;
  return (
    <button
      className="flex w-full items-center gap-1.5 rounded-[2px] border border-border bg-panel px-2 py-1.5 text-left text-[11px] hover:border-primary/60"
      onClick={() => onOpen(ticket.id)}
      type="button"
    >
      <span className="font-mono text-[10px] text-muted">{ticket.displayId}</span>
      <span className="min-w-0 flex-1 truncate text-foreground">{ticket.capturedName}</span>
      {recording ? (
        <span aria-hidden="true" title={recording.label}>
          {recording.glyph}
        </span>
      ) : null}
      <Badge square tone={kindMeta.tone}>
        {kindMeta.label}
      </Badge>
      <Badge square tone={statusMeta.tone}>
        {statusMeta.label}
      </Badge>
    </button>
  );
}

// The Board's right-rail inspector for an Epic (backend: Order) -- the
// organizational objective, never the execution engine. Deliberately shows
// no reserved-inventory/allocation/consumption fields (the Order lifecycle
// is inventory-neutral; that accounting lives entirely on Tickets now) and
// never infers this Epic's own lifecycle from its Tickets' progress or vice
// versa -- the two are independent by design.
export function EpicInspector({
  order,
  tickets,
  onClose,
  onChanged,
  onCreateTicket,
  onOpenTicket,
}: {
  order: OrderSummary;
  /** Board's already-loaded ticket list -- linked tickets are resolved
   * against this, never refetched/reconstructed here. */
  tickets: TicketSummary[];
  onClose: () => void;
  onChanged?: () => void;
  /** Opens the canonical Ticket editor prefilled with this Epic -- still
   * editable/removable there, never a second Epic-specific creation form. */
  onCreateTicket?: () => void;
  onOpenTicket: (ticketId: string) => void;
}) {
  // One additional fetch when the inspector opens, for the requirement/
  // linked-ticket detail the Board's lightweight OrderSummary doesn't carry
  // -- mirrors OrderAcquisitionRunDrawer's own detail-fetch pattern. Falls
  // back to the summary already in hand if it fails, so the header/
  // lifecycle controls still work.
  const [detail, setDetail] = useState<OrderDetail | null>(null);
  const [detailError, setDetailError] = useState("");
  const { busy, error, run } = useAsyncAction();
  const [rowBusyId, setRowBusyId] = useState<string | null>(null);
  const [rowError, setRowError] = useState("");
  const [confirmCancel, setConfirmCancel] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  function loadDetail() {
    setDetailError("");
    getOrder(order.id)
      .then(setDetail)
      .catch((requestError) => setDetailError(apiMessage(requestError)));
  }

  useEffect(() => {
    setDetail(null);
    loadDetail();
    // `loadDetail` only reads `order.id`; refetch when a different Epic is inspected.
  }, [order.id]);

  const statusMeta = orderStatusMeta[order.status];

  // Explicit organizational membership -- `ticket.orderId` is now the
  // authoritative answer to "which tickets does this Epic contain?",
  // including the root Manufacturing/Reaction ticket (it carries
  // `orderId` too, set at creation time). Deliberately NOT resolved via
  // requirement fulfillments or `sourceBuildId` equality anymore: a
  // shared `sourceBuildId` is Build *context*, and a fulfillment link
  // can point at a ticket that belongs to a different Epic entirely (see
  // `link_ticket_to_requirement`) -- neither is Epic membership.
  const linkedTickets = tickets.filter((ticket) => ticket.orderId === order.id);

  const ticketTotal = linkedTickets.length;
  const ticketComplete = linkedTickets.filter((ticket) => ticket.status === "complete").length;
  const kindBreakdown = { acquisition: 0, manufacturing: 0, reaction: 0, generic: 0 };
  for (const ticket of linkedTickets) kindBreakdown[ticket.kind] += 1;
  const recordedCount = linkedTickets.filter((ticket) => ticket.recording?.state === "recorded").length;

  const needsActionRequirements = detail?.requirements.filter((requirement) => requirement.state === "needsAction") ?? [];

  async function start() {
    await run(async () => {
      const updated = await startOrder(order.id);
      setDetail(updated);
      onChanged?.();
    });
  }

  async function complete() {
    await run(async () => {
      const updated = await completeOrder(order.id);
      setDetail(updated);
      onChanged?.();
    });
  }

  async function cancel() {
    setConfirmCancel(false);
    await run(async () => {
      const updated = await cancelOrder(order.id);
      setDetail(updated);
      onChanged?.();
    });
  }

  async function archive() {
    await run(async () => {
      const updated = await archiveOrder(order.id);
      setDetail(updated);
      onChanged?.();
    });
  }

  async function restore() {
    await run(async () => {
      const updated = await restoreOrder(order.id);
      setDetail(updated);
      onChanged?.();
    });
  }

  async function remove() {
    setConfirmDelete(false);
    await run(async () => {
      await deleteOrder(order.id);
      onChanged?.();
      onClose();
    });
  }

  async function createTicket(requirementId: string) {
    setRowError("");
    setRowBusyId(requirementId);
    try {
      await createTicketForRequirement(order.id, requirementId);
      loadDetail();
      onChanged?.();
    } catch (requestError) {
      setRowError(apiMessage(requestError));
    } finally {
      setRowBusyId(null);
    }
  }

  async function createAllTickets(requirementIds: string[]) {
    setRowError("");
    setRowBusyId("__bulk__");
    try {
      await bulkCreateTickets(order.id, requirementIds);
      loadDetail();
      onChanged?.();
    } catch (requestError) {
      setRowError(apiMessage(requestError));
    } finally {
      setRowBusyId(null);
    }
  }

  return (
    <PlannerInspectorShell
      closeLabel="Close Epic inspector"
      eyebrow="Epic"
      onClose={onClose}
      open
      title={order.displayName}
      width="wide"
    >
      <div className="flex-1 space-y-4 px-4 py-3">
        <div className="flex items-center gap-2">
          <Badge square tone={statusMeta.tone}>
            {statusMeta.label}
          </Badge>
          {order.archivedAt ? (
            <Badge square tone="muted">
              Archived
            </Badge>
          ) : null}
        </div>

        {error ? <InlineAlert title="Action did not finish">{error}</InlineAlert> : null}
        {rowError ? <InlineAlert title="Action did not finish">{rowError}</InlineAlert> : null}
        {detailError ? <InlineAlert title="Some details did not load">{detailError}</InlineAlert> : null}

        <dl className="space-y-1 text-xs">
          <SectionHeading>Details</SectionHeading>
          <Row label="Runs">{order.runs.toLocaleString()}</Row>
          <Row label="Source Build revision">{order.sourceBuildRevision}</Row>
          <Row label="Created">{formatDate(order.createdAt)}</Row>
          {order.startedAt ? <Row label="Started">{formatDate(order.startedAt)}</Row> : null}
          {order.completedAt ? <Row label="Completed">{formatDate(order.completedAt)}</Row> : null}
          {order.canceledAt ? <Row label="Canceled">{formatDate(order.canceledAt)}</Row> : null}
          {order.estimatedMaterialCost ? (
            <Row label="Estimated material cost">
              <MoneyAmount value={order.estimatedMaterialCost} />
            </Row>
          ) : null}
        </dl>

        <div className="space-y-1.5">
          <SectionHeading>Progress</SectionHeading>
          {ticketTotal > 0 ? (
            <>
              <p className="text-xs text-foreground">
                {ticketComplete}/{ticketTotal} tickets complete
              </p>
              <ProgressBar percent={(ticketComplete / ticketTotal) * 100} tone="positive" />
              <p className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] text-muted">
                {kindBreakdown.acquisition > 0 ? <span>{kindBreakdown.acquisition} acquisition</span> : null}
                {kindBreakdown.manufacturing > 0 ? <span>{kindBreakdown.manufacturing} manufacturing</span> : null}
                {kindBreakdown.reaction > 0 ? <span>{kindBreakdown.reaction} reaction</span> : null}
                {kindBreakdown.generic > 0 ? <span>{kindBreakdown.generic} generic</span> : null}
              </p>
              <p className="text-[10px] text-muted">
                {recordedCount}/{ticketTotal} recorded
              </p>
            </>
          ) : (
            <p className="text-xs text-muted">No tickets yet.</p>
          )}
        </div>

        {order.sourceBuildId ? (
          <div className="space-y-1.5">
            <SectionHeading>Linked Build</SectionHeading>
            <ButtonLink to={`/builds/${order.sourceBuildId}`}>Open build</ButtonLink>
          </div>
        ) : null}

        <div className="space-y-1.5">
          <div className="flex items-center justify-between gap-2">
            <SectionHeading>Tickets</SectionHeading>
            {onCreateTicket ? (
              <button
                className="text-[10px] font-semibold uppercase text-primary hover:underline"
                onClick={onCreateTicket}
                type="button"
              >
                + Create ticket
              </button>
            ) : null}
          </div>
          {linkedTickets.length > 0 ? (
            <div className="space-y-1">
              {linkedTickets.map((ticket) => (
                <LinkedTicketRow key={ticket.id} onOpen={onOpenTicket} ticket={ticket} />
              ))}
            </div>
          ) : null}
        </div>

        {needsActionRequirements.length > 0 ? (
          <div className="space-y-1.5">
            <SectionHeading>Needs Action</SectionHeading>
            {needsActionRequirements.length > 1 ? (
              <button
                className="iw-button-secondary w-full"
                disabled={rowBusyId !== null}
                onClick={() => void createAllTickets(needsActionRequirements.map((requirement) => requirement.id))}
                type="button"
              >
                {rowBusyId === "__bulk__" ? "Creating…" : `Create ${needsActionRequirements.length} tickets`}
              </button>
            ) : null}
            <div className="space-y-1">
              {needsActionRequirements.map((requirement) => {
                const kindMeta = requirementKindMeta[requirement.kind];
                return (
                  <div
                    className="flex items-center gap-1.5 rounded-[2px] border border-border bg-panel px-2 py-1.5 text-[11px]"
                    key={requirement.id}
                  >
                    <Badge square tone={kindMeta.tone}>
                      {kindMeta.label}
                    </Badge>
                    <span className="min-w-0 flex-1 truncate text-foreground">{requirement.capturedName}</span>
                    <button
                      className="iw-button-secondary shrink-0 px-1.5 py-0.5 text-[10px]"
                      disabled={rowBusyId !== null}
                      onClick={() => void createTicket(requirement.id)}
                      type="button"
                    >
                      {rowBusyId === requirement.id ? "Creating…" : "Create ticket"}
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        <div className="flex flex-wrap justify-end gap-2 border-t border-border pt-3">
          {order.status === "ready" ? (
            <button className="iw-button-primary" disabled={busy} onClick={() => void start()} type="button">
              Start Epic
            </button>
          ) : null}
          {order.status === "inProgress" ? (
            <button className="iw-button-primary" disabled={busy} onClick={() => void complete()} type="button">
              Mark Epic complete
            </button>
          ) : null}
          {order.status === "blocked" || order.status === "ready" || order.status === "inProgress" ? (
            <button
              className="iw-button-secondary"
              disabled={busy}
              onClick={() => setConfirmCancel(true)}
              type="button"
            >
              Cancel Epic
            </button>
          ) : null}
          {order.archivedAt ? (
            <button className="iw-button-secondary" disabled={busy} onClick={() => void restore()} type="button">
              Restore Epic
            </button>
          ) : (
            <button className="iw-button-secondary" disabled={busy} onClick={() => void archive()} type="button">
              Archive Epic
            </button>
          )}
          <button
            className="iw-button-danger"
            disabled={busy}
            onClick={() => setConfirmDelete(true)}
            type="button"
          >
            Delete Epic
          </button>
        </div>
      </div>

      <ConfirmDialog
        confirmLabel="Cancel Epic"
        onCancel={() => setConfirmCancel(false)}
        onConfirm={() => void cancel()}
        open={confirmCancel}
        title="Cancel this Epic?"
      >
        Canceling is organizational only — it does not touch inventory and does not cancel tickets already created
        for it, which may still be needed by other work. This cannot be undone.
      </ConfirmDialog>

      <ConfirmDialog
        confirmLabel="Delete Epic"
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => void remove()}
        open={confirmDelete}
        title="Delete this Epic?"
      >
        This removes the Epic and its frozen plan. Its tickets will remain on the Board. Inventory already
        recorded from those tickets will be kept. This cannot be undone.
      </ConfirmDialog>
    </PlannerInspectorShell>
  );
}

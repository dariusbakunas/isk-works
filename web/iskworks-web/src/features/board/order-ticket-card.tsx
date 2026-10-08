import { AlertCircle } from "lucide-react";

import type { Ticket, TicketRecordingSummary, TicketSummary } from "../../api/industry";
import { Badge } from "../../components/primitives";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { MoneyAmount } from "../../components/money";
import { CharacterName } from "../../observability/private";
import { orderTicketKindMeta, orderTicketStatusMeta } from "./order-meta";
import { recordingStateMeta } from "./recording/recording-meta";
import { isTicketDraggable, TICKET_DRAG_MIME } from "./ticket-drag";

// A small, secondary "how much of this has actually been recorded" line --
// item quantities for Acquisition, runs for Manufacturing/Reaction. It is
// NOT a Board lane or status: the card's lane and status badge are driven
// entirely by workflow `status`, never by recording state.
function RecordingIndicator({
  recording,
  isProduction,
}: {
  recording: TicketRecordingSummary;
  isProduction: boolean;
}) {
  const meta = recordingStateMeta[recording.state];
  const unit = isProduction ? " runs" : "";
  return (
    <p
      className="mt-1 flex items-center gap-1 text-[10px] text-muted"
      title={`${meta.label} — ${recording.recordedQuantity.toLocaleString()} of ${recording.requestedQuantity.toLocaleString()}${unit} recorded`}
    >
      <span aria-hidden="true">{meta.glyph}</span>
      <span className="font-mono tabular-nums">
        {recording.recordedQuantity.toLocaleString()} / {recording.requestedQuantity.toLocaleString()}
      </span>
      <span>recorded</span>
      <span className="sr-only">— {meta.label}</span>
    </p>
  );
}

// A ticket can join a new standalone Acquisition Run only while it's a
// To Do, unbatched Acquisition ticket -- Manufacturing/Reaction tickets are
// never batchable. There is no per-ticket "acquisition location" preview
// field, so the create call remains the authoritative check.
export function isBatchableOrderTicket(ticket: Ticket): boolean {
  return ticket.kind === "acquisition" && ticket.status === "todo" && ticket.acquisitionRunId === null;
}

// A standalone order::Ticket's own Board card -- deliberately a distinct
// component from OrderCard rather than a shared/polymorphic one.
export function OrderTicketCard({
  ticket,
  assignee,
  selectable = false,
  selected = false,
  onToggleSelect,
  onOpen,
  onDragStart,
  onDragEnd,
}: {
  ticket: TicketSummary;
  /** Resolved from `ticket.assigneeCharacterId` against Board's
   * already-loaded character roster -- no per-card fetch. `null`/omitted
   * when unassigned or the assignee isn't resolvable. */
  assignee?: { characterName: string; eveCharacterId: number } | null;
  selectable?: boolean;
  selected?: boolean;
  onToggleSelect?: (ticketId: string) => void;
  onOpen?: (ticketId: string) => void;
  onDragStart?: (ticketId: string) => void;
  onDragEnd?: () => void;
}) {
  const kindMeta = orderTicketKindMeta[ticket.kind];
  const statusMeta = orderTicketStatusMeta[ticket.status];
  // `actualLineTotal` is only present on tickets completed before completion
  // stopped pricing anything; prefer it when present, otherwise the
  // estimate.
  const cost = ticket.actualLineTotal ?? ticket.estimatedLineTotal;
  // Derived from the dependency read model, independent of workflow status
  // -- a ticket in any lane (including Complete) can still show blockers.
  const blocked = ticket.blockedBy.length > 0;
  const firstBlocker = ticket.blockedBy[0] ?? null;
  const remainingBlockers = ticket.blockedBy.length - 1;
  const isBatched = ticket.acquisitionRunId !== null;
  const batchable = isBatchableOrderTicket(ticket);
  // Drag moves the card between workflow lanes -- a bare status write, both
  // directions (see ticket-drag.ts). Disabled during selection mode
  // (checkbox clicks own that gesture) and for a batched ticket (must move
  // through its Run).
  const draggable = !selectable && isTicketDraggable(ticket);

  return (
    <div
      className={`relative overflow-hidden rounded-[2px] border bg-panel py-1.5 pl-3 pr-1.5 text-sm ${blocked ? "border-danger/50" : "border-border"} ${selectable && !batchable ? "opacity-50" : ""} ${draggable ? "cursor-grab active:cursor-grabbing" : ""}`}
      draggable={draggable}
      onClick={selectable ? undefined : () => onOpen?.(ticket.id)}
      onDragEnd={draggable ? () => onDragEnd?.() : undefined}
      onDragStart={
        draggable
          ? (event) => {
              if (event.dataTransfer) {
                event.dataTransfer.setData(TICKET_DRAG_MIME, ticket.id);
                event.dataTransfer.effectAllowed = "move";
              }
              onDragStart?.(ticket.id);
            }
          : undefined
      }
      role={selectable ? undefined : "button"}
      tabIndex={selectable ? undefined : 0}
      title={isBatched ? "Batched into an Acquisition Run — start or complete it from the Run instead." : undefined}
    >
      <span aria-hidden="true" className="absolute inset-y-0 left-0 w-[3px] bg-primary" />
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5">
          {selectable ? (
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
        </span>
        <Badge square tone={statusMeta.tone}>
          {statusMeta.label}
        </Badge>
      </div>
      <p
        className="mt-1 truncate text-[11px] font-medium leading-snug text-foreground"
        // Generic tickets have a free-form user-authored title; structured
        // (manufacturing/reaction) tickets carry a canonical product name.
        data-private={ticket.kind === "generic" ? "" : undefined}
      >
        {ticket.capturedName}
      </p>
      {ticket.kind !== "generic" ? (
        <p className="mt-0.5 font-mono text-[11px] tabular-nums text-muted">
          ×{(ticket.quantity ?? 0).toLocaleString()}
          {cost ? (
            <>
              {" · "}
              <MoneyAmount value={cost} />
            </>
          ) : null}
        </p>
      ) : null}
      <div className="mt-1 h-px bg-border/50" />
      <div className="mt-1 flex items-center justify-between gap-2">
        <Badge square tone={kindMeta.tone}>
          {kindMeta.label}
        </Badge>
        {assignee ? (
          <span className="flex min-w-0 items-center gap-1 text-[10px] text-muted">
            <EveCharacterPortrait characterId={assignee.eveCharacterId} characterName={assignee.characterName} size={32} className="h-4 w-4" />
            <CharacterName className="truncate" name={assignee.characterName} />
          </span>
        ) : null}
      </div>
      {ticket.recording ? (
        <RecordingIndicator
          isProduction={ticket.kind === "manufacturing" || ticket.kind === "reaction"}
          recording={ticket.recording}
        />
      ) : null}
      {isBatched ? (
        <p className="mt-1 text-[10px] text-batch">Batched into an Acquisition Run</p>
      ) : blocked && firstBlocker ? (
        <p className="mt-1 flex items-start gap-1 text-[10px] text-danger">
          <AlertCircle aria-hidden="true" className="mt-px shrink-0" size={10} />
          <span className="leading-tight">
            Blocked by{" "}
            {firstBlocker.representativeFulfillingTicketDisplayId ?? firstBlocker.capturedName}
            {remainingBlockers > 0 ? ` (+${remainingBlockers} more)` : ""}
          </span>
        </p>
      ) : null}
    </div>
  );
}

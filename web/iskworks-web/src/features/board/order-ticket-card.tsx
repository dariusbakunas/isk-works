import { AlertCircle } from "lucide-react";

import type { Ticket, TicketRecordingSummary, TicketSummary } from "../../api/industry";
import { dotToneClasses, textToneClasses, type Tone } from "../../components/primitives";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { MoneyAmount } from "../../components/money";
import { CharacterName } from "../../observability/private";
import { orderTicketKindMeta, orderTicketStatusMeta } from "./order-meta";
import { recordingStateMeta } from "./recording/recording-meta";
import { isTicketDraggable, TICKET_DRAG_MIME } from "./ticket-drag";
import type { TicketJobSummary } from "./ticket-job-progress";
import { groupBlockersByType } from "./blockers";
import { isBatchableOrderTicket } from "./ticket-selection";
import { ACTIVE_CARD_CLASS, useScrollIntoViewWhenActive } from "./active-card";

// A small, secondary "how much of this has actually been recorded" readout
// -- item quantities for Acquisition, runs for Manufacturing/Reaction. It is
// NOT a Board lane or status: the card's lane is driven entirely by
// workflow `status`, never by recording state. `glyphOnly` is the one-line
// compact row, where the counts live in the tooltip.
function RecordingIndicator({
  recording,
  isProduction,
  glyphOnly = false,
}: {
  recording: TicketRecordingSummary;
  isProduction: boolean;
  glyphOnly?: boolean;
}) {
  const meta = recordingStateMeta[recording.state];
  const unit = isProduction ? " runs" : "";
  return (
    <span
      className="flex shrink-0 items-center gap-1 text-[10px] text-muted"
      title={`${meta.label} — ${recording.recordedQuantity.toLocaleString()} of ${recording.requestedQuantity.toLocaleString()}${unit} recorded`}
    >
      <span aria-hidden="true" className={textToneClasses[meta.tone]}>
        {meta.glyph}
      </span>
      {glyphOnly ? null : (
        <span className="font-mono tabular-nums">
          {recording.recordedQuantity.toLocaleString()} / {recording.requestedQuantity.toLocaleString()}
        </span>
      )}
      <span className="sr-only">— {meta.label}</span>
    </span>
  );
}

// Progress for the ESI job(s) matched to an In Progress production ticket,
// drawn without adding card height: a 2px strip along the card's bottom
// edge plus a remaining-time label in the existing meta row. Display only
// -- it never moves the card between lanes.
function jobProgressTitle(summary: TicketJobSummary, requestedRuns: number | null): string {
  const jobs = `${summary.jobCount} ESI job${summary.jobCount === 1 ? "" : "s"}`;
  const runs =
    requestedRuns !== null && summary.jobRuns < requestedRuns
      ? ` · ${summary.jobRuns.toLocaleString()} of ${requestedRuns.toLocaleString()} runs started`
      : "";
  return `${jobs}${runs} · ${summary.percent}% · ${summary.label}`;
}

function JobProgressEdge({ summary, tone, title }: { summary: TicketJobSummary; tone: Tone; title: string }) {
  const barTone: Tone = summary.state === "ready" ? "positive" : summary.state === "paused" ? "muted" : tone;
  return (
    <div
      aria-label={`Job progress ${summary.percent}%`}
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={summary.percent}
      className="absolute inset-x-0 bottom-0 h-[2px] bg-border/60"
      role="progressbar"
      title={title}
    >
      <div className={`h-full ${dotToneClasses[barTone]}`} style={{ width: `${summary.percent}%` }} />
    </div>
  );
}

function JobRemainingLabel({
  summary,
  requestedRuns,
  title,
}: {
  summary: TicketJobSummary;
  requestedRuns: number | null;
  title: string;
}) {
  const partial = requestedRuns !== null && summary.jobRuns < requestedRuns;
  return (
    <span
      className={`shrink-0 font-mono text-[10px] tabular-nums ${summary.state === "ready" ? "text-positive" : "text-muted"}`}
      title={title}
    >
      {partial ? `${summary.jobRuns.toLocaleString()}/${requestedRuns.toLocaleString()} · ` : ""}
      {summary.label}
    </span>
  );
}

// A ticket can join a new standalone Acquisition Run only while it's a
// To Do, unbatched Acquisition ticket. There is no per-ticket "acquisition
// location" preview field, so the create call remains the authoritative
// check. Lives with the rest of the selection rules.
export { isBatchableOrderTicket } from "./ticket-selection";

// A standalone order::Ticket's own Board card -- deliberately a distinct
// component from OrderCard rather than a shared/polymorphic one.
//
// Kept dense on purpose: the left stripe carries the kind (BUY/BUILD/RXN)
// and the workflow status *is* the lane, so neither needs a badge row.
export function OrderTicketCard({
  ticket,
  assignee,
  jobProgress = null,
  selectable = false,
  selectionEnabled,
  selected = false,
  active = false,
  compact = false,
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
  /** The assignee's ESI job(s) matched to this ticket, summarized at the
   * Board's current tick. `null` when nothing is matched. */
  jobProgress?: TicketJobSummary | null;
  selectable?: boolean;
  /** Whether this card's checkbox is enabled in selection mode -- the
   * Board decides from what is already selected (see ticket-selection.ts).
   * Defaults to Acquisition Run eligibility. */
  selectionEnabled?: boolean;
  selected?: boolean;
  /** The ticket the Board's inspector is showing. */
  active?: boolean;
  /** One-line row (the Complete lane): identity and quantity only, with
   * recording collapsed to its glyph. Blocker/batch hints still show. */
  compact?: boolean;
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
  const blockers = groupBlockersByType(ticket.blockedBy);
  const blocked = blockers.length > 0;
  const firstBlocker = blockers[0] ?? null;
  const remainingBlockers = blockers.length - 1;
  const isBatched = ticket.acquisitionRunId !== null;
  const isProduction = ticket.kind === "manufacturing" || ticket.kind === "reaction";
  const quantity = ticket.kind !== "generic" ? `×${(ticket.quantity ?? 0).toLocaleString()}` : null;
  const batchable = selectionEnabled ?? isBatchableOrderTicket(ticket);
  const combinedEpicCount = ticket.orderId ? 0 : new Set((ticket.operations ?? []).map((op) => op.orderId)).size;
  // Drag moves the card between workflow lanes -- a bare status write, both
  // directions (see ticket-drag.ts). Disabled during selection mode
  // (checkbox clicks own that gesture) and for a batched ticket (must move
  // through its Run).
  const draggable = !selectable && isTicketDraggable(ticket);
  const ref = useScrollIntoViewWhenActive<HTMLDivElement>(active);
  const showJobProgress = !compact && jobProgress !== null;
  const requestedRuns = isProduction ? (ticket.recording?.requestedQuantity ?? null) : null;
  const jobTitle = jobProgress ? jobProgressTitle(jobProgress, requestedRuns) : "";

  return (
    <div
      aria-current={active ? "true" : undefined}
      className={`relative overflow-hidden rounded-[2px] border bg-panel ${compact ? "py-1" : "py-1.5"} pl-3 pr-1.5 text-sm ${blocked ? "border-danger/50" : "border-border"} ${selectable && !batchable ? "opacity-50" : ""} ${draggable ? "cursor-grab active:cursor-grabbing" : ""} ${active ? ACTIVE_CARD_CLASS : ""}`}
      ref={ref}
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
      <span aria-hidden="true" className={`absolute inset-y-0 left-0 w-[3px] ${dotToneClasses[kindMeta.tone]}`} />
      <span className="sr-only">{statusMeta.label}</span>
      <div className="flex items-center gap-1.5">
        {selectable ? (
          <input
            aria-label={`Select ${ticket.displayId}`}
            checked={selected}
            className="h-3.5 w-3.5 shrink-0"
            disabled={!batchable}
            onChange={() => onToggleSelect?.(ticket.id)}
            onClick={(event) => event.stopPropagation()}
            type="checkbox"
          />
        ) : null}
        <span className="shrink-0 font-mono text-[10px] text-muted">{ticket.displayId}</span>
        <span
          className="min-w-0 flex-1 truncate text-[11px] font-medium leading-snug text-foreground"
          // Generic tickets have a free-form user-authored title; structured
          // (manufacturing/reaction) tickets carry a canonical product name.
          data-private={ticket.kind === "generic" ? "" : undefined}
        >
          {ticket.capturedName}
        </span>
        {compact && quantity ? (
          <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted">{quantity}</span>
        ) : null}
        {compact && ticket.recording ? (
          <RecordingIndicator glyphOnly isProduction={isProduction} recording={ticket.recording} />
        ) : null}
        {assignee ? (
          <span className="flex shrink-0" title={assignee.characterName}>
            <EveCharacterPortrait
              characterId={assignee.eveCharacterId}
              characterName={assignee.characterName}
              size={16}
            />
            <CharacterName className="sr-only" name={assignee.characterName} />
          </span>
        ) : null}
      </div>
      {compact ? null : (
        <div className="mt-0.5 flex items-center justify-between gap-2">
          <span className="flex min-w-0 items-center gap-1.5 font-mono text-[10px] tabular-nums text-muted">
            <span className={`font-semibold ${textToneClasses[kindMeta.tone]}`}>{kindMeta.label}</span>
            {combinedEpicCount > 1 ? (
              <span
                className="shrink-0 rounded-[2px] border border-border px-1 text-[10px] text-muted"
                title={`Combined ticket: does this step for ${combinedEpicCount} Epics`}
              >
                ⧉ {combinedEpicCount} epics
              </span>
            ) : null}
            {quantity ? (
              <span className="truncate">
                {quantity}
                {cost ? (
                  <>
                    {" · "}
                    <MoneyAmount value={cost} />
                  </>
                ) : null}
              </span>
            ) : null}
          </span>
          <span className="flex shrink-0 items-center gap-1.5">
            {showJobProgress ? (
              <JobRemainingLabel requestedRuns={requestedRuns} summary={jobProgress} title={jobTitle} />
            ) : null}
            {ticket.recording ? <RecordingIndicator isProduction={isProduction} recording={ticket.recording} /> : null}
          </span>
        </div>
      )}
      {showJobProgress ? <JobProgressEdge summary={jobProgress} title={jobTitle} tone={kindMeta.tone} /> : null}
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

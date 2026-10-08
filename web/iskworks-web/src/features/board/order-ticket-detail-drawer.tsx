import { AlertTriangle } from "lucide-react";
import { useState } from "react";

import type { CharacterRosterEntry } from "../../api/characters";
import {
  deleteTicket,
  updateTicketMetadata,
  updateTicketStatus,
  type OrderSummary,
  type TicketStatus,
  type TicketSummary,
} from "../../api/industry";
import { Badge, ConfirmDialog, InlineAlert } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { apiMessage } from "../industry/shared/api-error";
import { formatDuration } from "../industry/shared/formatting";
import { orderTicketKindMeta, orderTicketStatusMeta } from "./order-meta";
import { prerequisiteSourcePresentation } from "./prerequisite-source";
import { RecordingSection } from "./recording/recording-section";

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

// A ticket's full detail view. The Workflow "Status" <select> is the
// keyboard/click path for the same organizational status write the Board's
// drag gesture performs (ticket-drag.ts) -- any direction, no gating. A
// ticket batched into an Acquisition Run hides the control; it advances
// only via the Run. Dependencies ("Blocked by") render separately and are
// purely derived -- never a workflow status.
export function OrderTicketDetailDrawer({
  ticket,
  epicName,
  orders,
  characters,
  onBack,
  onClose,
  onChanged,
  onOpenEpic,
}: {
  ticket: TicketSummary;
  /** The owning Epic's display name, when `ticket.orderId` is set and the
   * Board already has it loaded -- no extra fetch. `undefined`/`null` (or
   * `ticket.orderId` itself being `null`, a standalone ticket) hides the
   * Epic context section entirely. */
  epicName?: string | null;
  /** Board's already-loaded Epic list, for the Details section's Epic
   * dropdown -- omit to render Details without an Epic control (e.g. a
   * caller that hasn't loaded Orders at all). */
  orders?: OrderSummary[];
  /** Already-loaded/cached connected-character roster for the Assignee
   * dropdown -- no ESI call made here. */
  characters?: CharacterRosterEntry[];
  /** When provided (e.g. this ticket was opened from within an Epic
   * Inspector), renders a back button that returns to that inspector
   * instead of closing entirely. */
  onBack?: () => void;
  onClose: () => void;
  onChanged?: () => void;
  /** Switches to that Epic's own Inspector, staying on Board -- never a
   * navigation to `/orders/:id`. Omitted (or `ticket.orderId` null) hides
   * the Epic context section's click affordance. */
  onOpenEpic?: () => void;
}) {
  const kindMeta = orderTicketKindMeta[ticket.kind];
  const statusMeta = orderTicketStatusMeta[ticket.status];
  const snapshot = ticket.executionSnapshot;
  const blueprint = snapshot?.blueprint ?? null;
  const facility = snapshot?.facility ?? null;
  // `actual*` is only present on tickets completed before completion
  // stopped pricing anything; completing a ticket never sets it. Prefer it
  // when present, otherwise fall back to the Build/Order-frozen estimate
  // regardless of status.
  const unitCost = ticket.actualUnitCost ?? ticket.estimatedUnitCost;
  const lineTotal = ticket.actualLineTotal ?? ticket.estimatedLineTotal;
  const isBatched = ticket.acquisitionRunId !== null;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);

  async function remove() {
    setConfirmDelete(false);
    setError("");
    setBusy(true);
    try {
      await deleteTicket(ticket.id);
      onChanged?.();
      onClose();
    } catch (requestError) {
      setError(apiMessage(requestError));
      setBusy(false);
    }
  }

  // Organizational metadata editing -- title/notes/Epic/assignee, all via
  // the same `PATCH /api/tickets/:id` metadata path. Never touches status,
  // recording, execution snapshot, source Build, or requirement
  // fulfillments (see `updateTicketMetadata`'s own doc). Local field state
  // so typing doesn't round-trip on every keystroke; committed on blur/
  // change, same convention as the rest of this drawer's async actions.
  //
  // The Board keeps this component mounted and only swaps the `ticket` prop
  // when a different card is clicked (board-page.tsx's in-rail navigation
  // stack), so these ticket-derived drafts must be re-seeded whenever the
  // *selected ticket identity* changes -- otherwise Ticket B shows Ticket
  // A's Title/Notes. Keying the reset off `ticket.id` (not the `ticket`
  // object) is deliberate: a background refetch that produces a fresh
  // object for the *same* ticket must not clobber an in-progress unsaved
  // edit, and a still-pending metadata save for the previous ticket (its
  // closure captured the old id) can never write the old values into the
  // newly-selected ticket's drafts. This is the React "adjust state while
  // rendering" pattern -- cheaper and safer here than a `key` remount,
  // which would also restart the inspector shell and RecordingSection.
  const [draftTicketId, setDraftTicketId] = useState(ticket.id);
  const [title, setTitle] = useState(ticket.capturedName);
  const [notes, setNotes] = useState(ticket.notes);
  const [metaBusy, setMetaBusy] = useState(false);
  const [metaError, setMetaError] = useState("");

  if (draftTicketId !== ticket.id) {
    setDraftTicketId(ticket.id);
    setTitle(ticket.capturedName);
    setNotes(ticket.notes);
    // Errors are per-ticket too -- a failed save banner from the ticket we
    // just navigated away from must not linger over the new one.
    setMetaError("");
    setError("");
  }

  async function saveMetadata(input: Parameters<typeof updateTicketMetadata>[1]) {
    setMetaError("");
    setMetaBusy(true);
    try {
      await updateTicketMetadata(ticket.id, input);
      onChanged?.();
    } catch (requestError) {
      setMetaError(apiMessage(requestError));
    } finally {
      setMetaBusy(false);
    }
  }

  function commitTitle() {
    const trimmed = title.trim();
    if (!trimmed) {
      setTitle(ticket.capturedName);
      return;
    }
    if (trimmed === ticket.capturedName) return;
    void saveMetadata({ capturedName: trimmed });
  }

  function commitNotes() {
    if (notes === ticket.notes) return;
    void saveMetadata({ notes });
  }

  // One workflow-only write, any direction -- the same generic PATCH the
  // Board drag uses. No dependency gating: a blocked ticket can be moved to
  // Complete just like any other.
  async function changeStatus(next: TicketStatus) {
    if (next === ticket.status) return;
    setError("");
    setBusy(true);
    try {
      await updateTicketStatus(ticket.id, next);
      onChanged?.();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <PlannerInspectorShell
      backLabel="Back to Epic"
      closeLabel="Close ticket details"
      eyebrow={ticket.displayId}
      onBack={onBack}
      onClose={onClose}
      open
      privateTitle={ticket.kind === "generic"}
      title={ticket.capturedName}
      width="wide"
    >
      <div className="flex-1 space-y-4 px-4 py-3">
        <div className="flex items-center gap-2">
          <Badge square tone={statusMeta.tone}>
            {statusMeta.label}
          </Badge>
          <Badge square tone={kindMeta.tone}>
            {kindMeta.label}
          </Badge>
        </div>

        {error ? <InlineAlert title="Action did not finish">{error}</InlineAlert> : null}
        {metaError ? <InlineAlert title="Change did not save">{metaError}</InlineAlert> : null}

        {/* Workflow status -- purely organizational, user-controlled, any
            direction. Deliberately independent of the Dependencies section
            below and of Recording: no gating, no "conflicts with
            dependencies" warning. */}
        {isBatched ? (
          <p className="iw-muted text-xs">
            Batched into an Acquisition Run — advance it from the Run.
          </p>
        ) : (
          <div className="space-y-1">
            <label className="block text-[10px] text-muted" htmlFor="ticket-workflow-status">
              Status
            </label>
            <select
              className="iw-input w-full text-sm"
              disabled={busy}
              id="ticket-workflow-status"
              onChange={(event) => void changeStatus(event.target.value as TicketStatus)}
              value={ticket.status}
            >
              <option value="todo">To Do</option>
              <option value="inProgress">In Progress</option>
              <option value="complete">Complete</option>
              <option value="canceled">Canceled</option>
            </select>
          </div>
        )}

        <div className="space-y-2">
          <SectionHeading>Details</SectionHeading>
          <div className="space-y-1">
            <label className="block text-[10px] text-muted" htmlFor="ticket-title">
              Title
            </label>
            <input
              className="iw-input w-full text-sm"
              disabled={metaBusy}
              id="ticket-title"
              onBlur={commitTitle}
              onChange={(event) => setTitle(event.target.value)}
              value={title}
            />
          </div>

          {orders ? (
            <div className="space-y-1">
              <label className="block text-[10px] text-muted" htmlFor="ticket-epic">
                Epic
              </label>
              <div className="flex items-center gap-2">
                <select
                  className="iw-input w-full text-sm"
                  disabled={metaBusy}
                  id="ticket-epic"
                  onChange={(event) => {
                    const value = event.target.value;
                    void saveMetadata({ orderId: value || null });
                  }}
                  value={ticket.orderId ?? ""}
                >
                  <option value="">No Epic</option>
                  {orders.map((order) => (
                    <option key={order.id} value={order.id}>
                      {order.displayName}
                    </option>
                  ))}
                </select>
                {ticket.orderId && epicName && onOpenEpic ? (
                  <button
                    className="shrink-0 text-[10px] font-semibold uppercase text-primary hover:underline"
                    onClick={onOpenEpic}
                    type="button"
                  >
                    Open
                  </button>
                ) : null}
              </div>
            </div>
          ) : null}

          {characters ? (
            <div className="space-y-1">
              <label className="block text-[10px] text-muted" htmlFor="ticket-assignee">
                Assignee
              </label>
              <select
                className="iw-input w-full text-sm"
                disabled={metaBusy}
                id="ticket-assignee"
                onChange={(event) => {
                  const value = event.target.value;
                  void saveMetadata({ assigneeCharacterId: value || null });
                }}
                value={ticket.assigneeCharacterId ?? ""}
              >
                <option value="">Unassigned</option>
                {characters.map((character) => (
                  <option key={character.connectionId} value={character.connectionId}>
                    {character.characterName}
                  </option>
                ))}
              </select>
            </div>
          ) : null}

          <div className="space-y-1">
            <label className="block text-[10px] text-muted" htmlFor="ticket-notes">
              Notes
            </label>
            <textarea
              className="iw-input w-full text-sm"
              disabled={metaBusy}
              id="ticket-notes"
              onBlur={commitNotes}
              onChange={(event) => setNotes(event.target.value)}
              rows={3}
              value={notes}
            />
          </div>
        </div>

        {ticket.kind !== "generic" ? (
          <div className="space-y-1">
            <SectionHeading>Output</SectionHeading>
            <p className="text-sm font-semibold text-foreground">{ticket.capturedName}</p>
            <p className="text-xs text-muted">
              ×{(ticket.quantity ?? 0).toLocaleString()} units
              {snapshot ? ` from ${snapshot.runs.toLocaleString()} run${snapshot.runs === 1 ? "" : "s"}` : ""}
            </p>
          </div>
        ) : null}

        {blueprint ? (
          <div className="space-y-1.5">
            <SectionHeading>Blueprint</SectionHeading>
            <dl className="space-y-1 text-xs">
              <p className="text-sm font-medium text-foreground">{blueprint.blueprintName}</p>
              <Row label="ME / TE">
                {blueprint.materialEfficiency} / {blueprint.timeEfficiency}
              </Row>
              {blueprint.sourceLocationName ? <Row label="Location">{blueprint.sourceLocationName}</Row> : null}
            </dl>
          </div>
        ) : null}

        {facility ? (
          <div className="space-y-1.5">
            <SectionHeading>Installation</SectionHeading>
            <dl className="space-y-1 text-xs">
              <Row label="Facility">{facility.name}</Row>
              {facility.solarSystemName ? <Row label="System">{facility.solarSystemName}</Row> : null}
            </dl>
          </div>
        ) : null}

        {snapshot ? (
          <dl className="space-y-1 text-xs">
            <SectionHeading>Time</SectionHeading>
            <Row label="Duration">{formatDuration(snapshot.durationSeconds)}</Row>
            <Row label="Runs">{snapshot.runs.toLocaleString()}</Row>
          </dl>
        ) : null}

        {ticket.prerequisites.length > 0 ? (
          <div className="space-y-1">
            <SectionHeading>Required Materials</SectionHeading>
            <div className="flex items-center gap-2 pb-1 text-[10px] uppercase text-muted">
              <span className="flex-1">Material</span>
              <span className="w-16 shrink-0 text-right">Req.</span>
              <span className="w-16 shrink-0 text-right">Fresh</span>
              <span className="w-36 shrink-0 text-right">Source</span>
            </div>
            <div className="divide-y divide-border/50">
              {ticket.prerequisites.map((prerequisite) => {
                // SOURCE folds the frozen Model-B reuse split into the
                // sourcing kind: a fully inventory-covered row (fresh 0)
                // reads "Use Inventory", a partial one "<Kind> · Missing
                // (N)" -- never a bare "BUY" that implies buying the whole
                // quantity. Shared with the Worksheet/Graph vocabulary.
                const source = prerequisiteSourcePresentation(prerequisite);
                return (
                  <div className="flex items-center gap-2 py-0.5 text-[11px]" key={prerequisite.id}>
                    <span className="min-w-0 flex-1 truncate text-foreground">{prerequisite.capturedName}</span>
                    <span className="w-16 shrink-0 text-right font-mono tabular-nums text-foreground">
                      {prerequisite.requiredQuantity.toLocaleString()}
                    </span>
                    <span
                      className={`w-16 shrink-0 text-right font-mono tabular-nums ${prerequisite.freshQuantity > 0 ? "text-danger" : "text-positive"}`}
                    >
                      {prerequisite.freshQuantity.toLocaleString()}
                    </span>
                    <span className="flex w-36 shrink-0 justify-end">
                      <Badge square tone={source.tone}>
                        {source.label}
                      </Badge>
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        {unitCost || lineTotal ? (
          <dl className="space-y-1 text-xs">
            <SectionHeading>Economics</SectionHeading>
            {unitCost ? (
              <Row label="Unit cost">
                <MoneyAmount value={unitCost} />
              </Row>
            ) : null}
            {lineTotal ? (
              <Row label={ticket.actualLineTotal != null ? "Actual cost" : "Estimated cost"}>
                <MoneyAmount value={lineTotal} />
              </Row>
            ) : null}
          </dl>
        ) : null}

        {/* Explicit inventory recording -- what ISK Works knows actually
            happened. Independent of both workflow status and dependency
            state; it never moves the ticket between Board lanes. */}
        <RecordingSection onRecorded={() => onChanged?.()} ticket={ticket} />

        {/* Dependencies -- derived, informational. Shown whenever the ticket
            has unmet prerequisites, regardless of its workflow status
            (a Complete ticket can still list blockers). */}
        {ticket.blockedBy.length > 0 ? (
          <div>
            <SectionHeading>Blocked by</SectionHeading>
            <ul className="mt-1.5 space-y-1">
              {ticket.blockedBy.map((blocker) => (
                <li
                  className="flex items-start gap-1.5 rounded-[2px] border border-border bg-panel px-2 py-1 text-[11px]"
                  key={blocker.prerequisiteId}
                >
                  <AlertTriangle aria-hidden="true" className="mt-px shrink-0 text-danger" size={11} />
                  <span className="min-w-0 flex-1">
                    <span className="truncate text-foreground">{blocker.capturedName}</span>
                    <span className="ml-1 font-mono tabular-nums text-muted">
                      ×{blocker.outstandingQuantity.toLocaleString()}
                    </span>
                    {blocker.representativeFulfillingTicketDisplayId ? (
                      <span className="ml-1 font-mono text-primary">
                        — {blocker.representativeFulfillingTicketDisplayId}
                      </span>
                    ) : null}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        <div className="flex justify-end border-t border-border pt-3">
          <button
            className="iw-button-danger"
            disabled={busy}
            onClick={() => setConfirmDelete(true)}
            type="button"
          >
            Delete ticket
          </button>
        </div>
      </div>

      <ConfirmDialog
        confirmLabel="Delete ticket"
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => void remove()}
        open={confirmDelete}
        title="Delete this ticket?"
      >
        This removes the ticket from the Board. Inventory already recorded from this ticket will be kept. This
        cannot be undone.
      </ConfirmDialog>
    </PlannerInspectorShell>
  );
}

import { useMemo, useState } from "react";
import { useSearchParams } from "react-router";

import {
  type AcquisitionRun,
  type OrderSummary,
  type Ticket,
  type TicketStatus,
  type TicketSummary,
} from "../../api/industry";
import { Badge, InlineAlert, LoadingState, PageHeader, StatusDot, type Tone } from "../../components/primitives";
import {
  boardEpicCardIsVisible,
  filterBoardTickets,
  hasActiveBoardFilters,
  parseBoardFiltersFromParams,
  TICKET_TYPE_FILTER_OPTIONS,
  withBoardFilterParam,
  withoutBoardFilters,
} from "./board-filters";
import { CreateOrderAcquisitionRunDialog } from "./create-order-acquisition-run-dialog";
import { EpicInspector } from "./epic-inspector";
import { deriveOrderAcquisitionGroups, isGroupableOrderAcquisitionTicket } from "./order-acq-group";
import { OrderAcqGroupCard } from "./order-acq-group-card";
import { OrderAcquisitionRunCard } from "./order-acquisition-run-card";
import { OrderAcquisitionRunDrawer } from "./order-acquisition-run-drawer";
import { OrderCard } from "./order-card";
import { combineSuggestions } from "./combine-suggestions";
import { CombineSuggestionsDialog } from "./combine-suggestions-dialog";
import { CombineTicketsDialog } from "./combine-tickets-dialog";
import { OrderSelectionBar } from "./order-selection-bar";
import { isSelectable, selectionKind } from "./ticket-selection";
import { OrderTicketCard } from "./order-ticket-card";
import { OrderTicketDetailDrawer } from "./order-ticket-detail-drawer";
import { summarizeTicketJobs } from "./ticket-job-progress";
import { TicketEditor } from "./ticket-editor";
import { useBoardData } from "./use-board-data";
import { useBoardDrag } from "./use-board-drag";
import { useBoardDrawers } from "./use-board-drawers";
import { useBoardSelection } from "./use-board-selection";

// The three organizational workflow lanes. There is no "Blocked" or
// "Ready" lane -- a ticket with unmet dependencies stays in whatever lane
// the user put it in and shows a derived blocker indicator instead (see
// OrderTicketCard). `canceled` has no lane (canceled cards don't render on
// the Board).
const lanes: { status: TicketStatus; label: string; tone: Tone }[] = [
  { status: "todo", label: "To Do", tone: "muted" },
  { status: "inProgress", label: "In Progress", tone: "primary" },
  { status: "complete", label: "Complete", tone: "muted" },
];

// Epic cards carry the derived `OrderStatus` (`blocked`/`ready` = some /
// no requirement outstanding), not a Ticket workflow status -- both fold
// into the "To Do" lane.
function epicLaneStatus(orderStatus: OrderSummary["status"]): TicketStatus {
  if (orderStatus === "blocked" || orderStatus === "ready") return "todo";
  return orderStatus;
}

// The Complete lane is history, not a work queue: it shows this many
// finished Epics and standalone tickets (most recently completed first) as
// one-line rows until expanded.
const COMPLETE_LANE_PREVIEW = 10;

// One row of the Complete lane: Epics and tickets share one history.
type CompleteLaneEntry =
  | { kind: "epic"; order: OrderSummary }
  | { kind: "ticket"; ticket: TicketSummary };

function completeEntryTime(entry: CompleteLaneEntry): string {
  return (entry.kind === "epic" ? entry.order.completedAt : entry.ticket.completedAt) ?? "";
}

function byMostRecentlyCompletedEntry(a: CompleteLaneEntry, b: CompleteLaneEntry): number {
  if (a.kind === "ticket" && b.kind === "ticket") return byMostRecentlyCompleted(a.ticket, b.ticket);
  return completeEntryTime(b).localeCompare(completeEntryTime(a));
}

// Most recently completed first. Ties (and any ticket without a
// `completedAt`) fall back to the newest display id, comparing its numeric
// part so ISK-1000 sorts after ISK-999.
function displayIdNumber(displayId: string): number {
  return Number(displayId.replace(/\D/g, "")) || 0;
}
function byMostRecentlyCompleted(a: TicketSummary, b: TicketSummary): number {
  const completed = (b.completedAt ?? "").localeCompare(a.completedAt ?? "");
  return completed !== 0 ? completed : displayIdNumber(b.displayId) - displayIdNumber(a.displayId);
}

// Lane drop-target highlight, keyed by the lane's own Tone (so "In
// Progress" tints primary, "Complete" tints muted, etc.) -- two intensities,
// an accent bar plus a tinted background.
const laneEligibleClasses: Record<Tone, string> = {
  primary: "border-primary/40 bg-primary/5",
  positive: "border-positive/40 bg-positive/5",
  danger: "border-danger/40 bg-danger/5",
  warning: "border-warning/40 bg-warning/5",
  muted: "border-foreground/30 bg-panel-strong",
  batch: "border-batch/40 bg-batch/5",
  reaction: "border-reaction/40 bg-reaction/5",
};
const laneHoverClasses: Record<Tone, string> = {
  primary: "border-primary bg-primary/15",
  positive: "border-positive bg-positive/15",
  danger: "border-danger bg-danger/15",
  warning: "border-warning bg-warning/15",
  muted: "border-foreground/60 bg-panel-strong",
  batch: "border-batch bg-batch/15",
  reaction: "border-reaction bg-reaction/15",
};

// What the ticket drawer shows about combining: a combined ticket's Epics
// and members, or a merged member's combined ticket. Derived from the
// already-loaded tickets and Epics.
function combinedTicketContext(ticket: Ticket, tickets: Ticket[], orders: { id: string; displayName: string }[]) {
  const nameOf = (orderId: string) => orders.find((order) => order.id === orderId)?.displayName ?? "Unknown Epic";
  const epicOf = (member: Ticket) => member.operations?.[0]?.orderId ?? member.orderId;
  const epicIds = ticket.orderId ? [] : [...new Set((ticket.operations ?? []).map((op) => op.orderId))];
  const combinedInto = ticket.mergedIntoTicketId
    ? (tickets.find((other) => other.id === ticket.mergedIntoTicketId) ?? null)
    : null;
  return {
    epics: epicIds.map((id) => ({ id, name: nameOf(id) })),
    combinedFrom: tickets
      .filter((member) => member.mergedIntoTicketId === ticket.id)
      .map((member) => {
        const epic = epicOf(member);
        return {
          id: member.id,
          displayId: member.displayId,
          epicName: epic ? nameOf(epic) : "No Epic",
          quantity: member.quantity,
        };
      }),
    mergedInto: combinedInto ? { id: combinedInto.id, displayId: combinedInto.displayId } : null,
  };
}

export function BoardPage() {
  const [params, setParams] = useSearchParams();
  const { state, reload, patchTicketStatus, jobsByTicketId, nowMs, facilityById } = useBoardData();
  const loadedTickets = state.status === "ready" ? state.tickets : null;
  const selection = useBoardSelection(loadedTickets ?? []);
  const { selectionMode, selectedTicketIds, selectedTickets } = selection;
  const drag = useBoardDrag({ tickets: loadedTickets, patchTicketStatus, reload });
  const drawers = useBoardDrawers(reload);
  const { openRunId, openOrderId, openTicketId } = drawers;
  const [showArchivedOrders, setShowArchivedOrders] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [combineDialogOpen, setCombineDialogOpen] = useState(false);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  // Keyed `${priceSourceId}:${status}` -- collapsed by default, the whole
  // point of grouping being to cut Board noise down from one card per
  // ticket.
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [showAllComplete, setShowAllComplete] = useState(false);

  // Board filters -- a distinct state dimension from Inspector selection
  // (see `useBoardDrawers`). Derived live from the URL on every render
  // (never mirrored into local state) so they can never drift from what's
  // bookmarked/shared/reloaded; `?epicFilter=`/`?assigneeFilter=`/
  // `?typeFilter=`/`?q=` are deliberately distinct from the `?epic=`/
  // `?ticket=` Inspector-selection params -- see board-filters.ts's own doc
  // for why those were never renamed or overloaded.
  const filters = parseBoardFiltersFromParams(params);

  function setFilterParam(key: "epicFilter" | "assigneeFilter" | "typeFilter" | "search", value: string) {
    // `replace: true` -- a filter/search change updates the current
    // history entry rather than pushing a new one, so typing a search
    // query doesn't turn "back" into an undo-one-keystroke button.
    setParams(withBoardFilterParam(params, key, value), { replace: true });
  }

  function clearFilters() {
    setParams(withoutBoardFilters(params), { replace: true });
  }

  function toggleGroup(key: string) {
    setExpandedGroups((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }

  // Every group of Board tickets that could be combined right now.
  const suggestions = useMemo(
    () =>
      state.status === "ready"
        ? combineSuggestions(state.tickets, {
            hasJobs: (ticket) => (jobsByTicketId.get(ticket.id)?.length ?? 0) > 0,
            ordersById: new Map(state.orders.map((order) => [order.id, order])),
          })
        : [],
    [state, jobsByTicketId],
  );
  const selectedGroup = selectedTickets[0]
    ? suggestions.find((group) => group.tickets.some((ticket) => ticket.id === selectedTickets[0].id))
    : undefined;

  const view = useMemo(() => {
    if (state.status !== "ready") return null;
    const { tickets, runs, orders, priceSources, characters } = state;

    const priceSourceNameById = new Map(priceSources.map((source) => [source.id, source.name]));
    // For the card's own subtle assignee indicator -- resolved from the
    // already-loaded roster, never a per-card fetch.
    const characterById = new Map(characters.map((character) => [character.connectionId, character]));
    // Search-field lookups only (Epic title / assignee name) -- see
    // board-filters.ts's own `matchesSearch`.
    const orderNameById = new Map(orders.map((order) => [order.id, order.displayName]));
    const characterNameById = new Map(characters.map((character) => [character.connectionId, character.characterName]));
    const filterContext = { orderNameById, characterNameById };


    const runById = new Map(runs.map((run) => [run.id, run]));
    const ticketsByRun = new Map<string, Ticket[]>();
    for (const ticket of tickets) {
      if (!ticket.acquisitionRunId) continue;
      const list = ticketsByRun.get(ticket.acquisitionRunId) ?? [];
      list.push(ticket);
      ticketsByRun.set(ticket.acquisitionRunId, list);
    }

    // Canceled orders never render on the Board at all (no lane for them --
    // a canceled Order isn't active work and has no "uncancel"; still
    // reachable via a direct link). Archived orders are hidden unless
    // "Show archived" is on. This scoping is orthogonal to Board filters
    // (Epic/Assignee/Type/Search) -- applied first, same as before.
    const archivedInScope = orders.filter((order) => order.archivedAt !== null);
    const scopedOrders = orders.filter(
      (order) => order.status !== "canceled" && (showArchivedOrders || order.archivedAt === null),
    );
    const hiddenArchivedOrderCount = showArchivedOrders ? 0 : archivedInScope.length;

    const nonArchivedTickets = tickets.filter((ticket) => ticket.archivedAt === null);
    // Filtering happens once, here, before any lane/grouping derivation --
    // every downstream card list (runs, ACQ groups, lane counts) is a pure
    // function of this already-filtered set, never re-filtered per lane.
    const ticketCards: TicketSummary[] = filterBoardTickets(nonArchivedTickets, filters, filterContext);
    // An Epic card's visibility is a function of its *matching children*,
    // not its own fields -- see `boardEpicCardIsVisible`'s own doc for why
    // this isn't simply "apply the same predicate to Orders too".
    const orderCards: OrderSummary[] = scopedOrders.filter((order) =>
      boardEpicCardIsVisible(order, nonArchivedTickets, filters, filterContext),
    );

    const runCards = Array.from(
      new Set(ticketCards.filter((ticket) => ticket.acquisitionRunId).map((ticket) => ticket.acquisitionRunId as string)),
    )
      .map((runId) => ({ run: runById.get(runId), tickets: ticketsByRun.get(runId) ?? [] }))
      .filter((entry): entry is { run: AcquisitionRun; tickets: Ticket[] } => entry.run !== undefined);

    const laneCounts = new Map<TicketStatus, number>();
    for (const order of orderCards) {
      // `canceled` is already filtered out above; `blocked`/`ready` fold
      // into the "To Do" lane.
      const laneStatus = epicLaneStatus(order.status);
      laneCounts.set(laneStatus, (laneCounts.get(laneStatus) ?? 0) + 1);
    }
    for (const ticket of ticketCards) {
      // A canceled ticket simply never renders (no lane for it, same as a
      // canceled Order).
      if (ticket.status === "canceled") continue;
      laneCounts.set(ticket.status, (laneCounts.get(ticket.status) ?? 0) + 1);
    }
    for (const { run } of runCards) {
      // Acquisition runs use the same 3 lanes -- `ready` folds into "To Do".
      const laneStatus: TicketStatus = run.status === "ready" ? "todo" : run.status;
      laneCounts.set(laneStatus, (laneCounts.get(laneStatus) ?? 0) + 1);
    }

    // Filter dropdown options -- sourced from the same already-loaded
    // Order/character lists, never re-fetched. `orders` (not `scopedOrders`)
    // so an archived Epic remains choosable in the filter even while "Show
    // archived" is off; a person can still filter by name, they just won't
    // see its card until they also toggle archived visibility.
    const epicFilterOptions = orders
      .filter((order) => order.status !== "canceled")
      .slice()
      .sort((a, b) => a.displayName.localeCompare(b.displayName));
    const assigneeFilterOptions = characters
      .slice()
      .sort((a, b) => a.characterName.localeCompare(b.characterName));

    return {
      hiddenArchivedOrderCount,
      orderCards,
      ticketCards,
      runCards,
      priceSourceNameById,
      characterById,
      orderNameById,
      characterNameById,
      laneCounts,
      epicFilterOptions,
      assigneeFilterOptions,
      hasAnyVisibleCards: orderCards.length > 0 || ticketCards.length > 0,
      distinctRunIds: runs.length,
      totalTickets: tickets.length,
      totalOrders: orders.length,
    };
  }, [state, showArchivedOrders, filters.epicFilter, filters.assigneeFilter, filters.typeFilter, filters.search]);

  if (state.status === "loading") return <LoadingState>Loading Board…</LoadingState>;
  if (state.status === "error") {
    return (
      <div className="iw-page-container">
        <PageHeader eyebrow="Production Planning" title="Board">
          Operational work across every Epic and Acquisition Run.
        </PageHeader>
        <InlineAlert title="Could not load the Board">{state.message}</InlineAlert>
      </div>
    );
  }
  if (!view) return null;

  // Sourced from the full (unfiltered) ticket/run lists, not `view`, so the
  // drawer still has complete data even if search narrows what's currently
  // visible on the board underneath it.
  const openRun = state.runs.find((run) => run.id === openRunId);
  const openRunTickets = openRun ? state.tickets.filter((ticket) => ticket.acquisitionRunId === openRun.id) : [];
  const openTicket = state.tickets.find((ticket) => ticket.id === openTicketId);
  const openRunPriceSourceName = openRun?.priceSourceId
    ? (view.priceSourceNameById.get(openRun.priceSourceId) ?? null)
    : null;
  // The Ticket Inspector takes over the rail whenever a ticket is open,
  // hiding the Epic Inspector underneath it (see openTicketFromEpic) --
  // Back returns to it, Close dismisses both (see closeTicket).
  const openOrder = !openTicket ? state.orders.find((order) => order.id === openOrderId) : undefined;

  return (
    <div className="iw-page-container pb-16">
      <PageHeader eyebrow="Production Planning" title="Board">
        What should I do next? Operational work across every Epic and Acquisition Run.
      </PageHeader>

      <div className="mb-3 flex flex-wrap items-center gap-2 border-b border-border pb-3" data-testid="board-toolbar">
        <input
          aria-label="Search tickets"
          className="iw-input w-56"
          onChange={(event) => setFilterParam("search", event.target.value)}
          placeholder="Search tickets…"
          type="text"
          value={filters.search}
        />
        <select
          aria-label="Epic"
          className="iw-input w-40"
          onChange={(event) => setFilterParam("epicFilter", event.target.value)}
          value={filters.epicFilter}
        >
          <option value="">All Epics</option>
          <option value="none">No Epic</option>
          {view.epicFilterOptions.map((order) => (
            <option key={order.id} value={order.id}>
              {order.displayName}
            </option>
          ))}
        </select>
        <select
          aria-label="Assignee"
          className="iw-input w-40"
          onChange={(event) => setFilterParam("assigneeFilter", event.target.value)}
          value={filters.assigneeFilter}
        >
          <option value="">All assignees</option>
          <option value="unassigned">Unassigned</option>
          {view.assigneeFilterOptions.map((character) => (
            <option key={character.connectionId} value={character.connectionId}>
              {character.characterName}
            </option>
          ))}
        </select>
        <select
          aria-label="Ticket type"
          className="iw-input w-36"
          onChange={(event) => setFilterParam("typeFilter", event.target.value)}
          value={filters.typeFilter}
        >
          {TICKET_TYPE_FILTER_OPTIONS.map((option) => (
            <option key={option.value || "all"} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        {hasActiveBoardFilters(filters) ? (
          <button className="iw-button-secondary" onClick={clearFilters} type="button">
            Clear filters
          </button>
        ) : null}
        <div className="ml-auto flex items-center gap-3 text-xs">
          {lanes.map((lane) => (
            <span className="flex items-center gap-1" key={lane.status}>
              <StatusDot tone={lane.tone} />
              <span className="iw-muted">
                {lane.label} {view.laneCounts.get(lane.status) ?? 0}
              </span>
            </span>
          ))}
        </div>
        {view.hiddenArchivedOrderCount > 0 ? (
          <Badge square tone="muted">
            {view.hiddenArchivedOrderCount} archived hidden
          </Badge>
        ) : null}
        <button
          aria-pressed={showArchivedOrders}
          className={showArchivedOrders ? "iw-button-primary" : "iw-button-secondary"}
          onClick={() => setShowArchivedOrders((current) => !current)}
          type="button"
        >
          {showArchivedOrders ? "Hide archived" : "Show archived"}
        </button>
        {suggestions.length > 0 ? (
          <button className="iw-button-secondary" onClick={() => setSuggestionsOpen(true)} type="button">
            {suggestions.length} combine suggestion{suggestions.length === 1 ? "" : "s"}
          </button>
        ) : null}
        <button
          aria-pressed={selectionMode}
          className={selectionMode ? "iw-button-primary" : "iw-button-secondary"}
          onClick={() => (selectionMode ? selection.exitSelectionMode() : selection.startSelection())}
          type="button"
        >
          {selectionMode ? "Cancel Selection" : "Select"}
        </button>
        <button className="iw-button-primary" onClick={() => drawers.openTicketEditor()} type="button">
          Create ticket
        </button>
      </div>

      {drag.dragError ? (
        <div className="mb-3">
          <InlineAlert title="Could not update ticket status">{drag.dragError}</InlineAlert>
        </div>
      ) : null}

      {hasActiveBoardFilters(filters) && !view.hasAnyVisibleCards ? (
        <div className="mb-3 flex items-center justify-between gap-2 rounded-[2px] border border-border bg-panel px-3 py-2 text-sm">
          <span className="iw-muted">No tickets match these filters.</span>
          <button className="iw-button-secondary" onClick={clearFilters} type="button">
            Clear filters
          </button>
        </div>
      ) : null}

      <div className="flex flex-col gap-3 sm:flex-row" data-testid="board-lanes">
        {lanes.map((lane) => {
          const laneOrderCards = view.orderCards.filter(
            (order) => epicLaneStatus(order.status) === lane.status,
          );
          const laneRunCards = view.runCards.filter(
            (entry) => (entry.run.status === "ready" ? "todo" : entry.run.status) === lane.status,
          );
          const laneTicketsAll = view.ticketCards.filter((ticket) => ticket.status === lane.status);
          const nonAcqTickets = laneTicketsAll.filter(
            (ticket) => ticket.acquisitionRunId === null && !isGroupableOrderAcquisitionTicket(ticket),
          );
          const acqGroups = deriveOrderAcquisitionGroups(laneTicketsAll);
          const laneCardCount = laneOrderCards.length + laneRunCards.length + nonAcqTickets.length + acqGroups.length;
          const isCompleteLane = lane.status === "complete";
          // The Complete lane files finished Epics among the tickets, by
          // completion time; the other lanes keep Epics on top.
          const laneEntries: CompleteLaneEntry[] = isCompleteLane
            ? [
                ...laneOrderCards.map((order) => ({ kind: "epic" as const, order })),
                ...nonAcqTickets.map((ticket) => ({ kind: "ticket" as const, ticket })),
              ].sort(byMostRecentlyCompletedEntry)
            : nonAcqTickets.map((ticket) => ({ kind: "ticket" as const, ticket }));
          const hiddenEntryCount =
            isCompleteLane && !showAllComplete ? Math.max(0, laneEntries.length - COMPLETE_LANE_PREVIEW) : 0;
          const visibleEntries =
            hiddenEntryCount > 0 ? laneEntries.slice(0, COMPLETE_LANE_PREVIEW) : laneEntries;
          const laneDrag = drag.lane(lane.status);
          const laneHighlight = laneDrag.hovered
            ? laneHoverClasses[lane.tone]
            : laneDrag.eligible
              ? laneEligibleClasses[lane.tone]
              : "border-border";
          return (
            <div
              // An empty lane gives its width to the lanes that have cards.
              className={`rounded-[2px] border transition-colors sm:min-w-0 ${laneCardCount === 0 ? "sm:flex-[0.5]" : "sm:flex-1"} ${laneHighlight}`}
              data-drag-eligible={laneDrag.eligible ? "true" : "false"}
              data-testid={`board-lane-${lane.status}`}
              key={lane.status}
              onDragLeave={laneDrag.onDragLeave}
              onDragOver={laneDrag.onDragOver}
              onDrop={laneDrag.onDrop}
            >
              <div className="flex items-center justify-between border-b border-border px-2 py-1.5">
                <span className="flex items-center gap-1.5 text-xs font-medium text-foreground">
                  <StatusDot tone={lane.tone} />
                  {lane.label}
                </span>
                <span className="rounded-[2px] border border-border bg-panel-strong px-1.5 py-px font-mono text-[10px] text-muted">
                  {laneCardCount}
                </span>
              </div>
              <div className="flex flex-col gap-1.5 p-1.5">
                {laneCardCount === 0 ? <p className="iw-muted text-xs">No tickets.</p> : null}
                {(isCompleteLane ? [] : laneOrderCards).map((order) => (
                  <OrderCard
                    // A ticket opened from the Epic takes over the rail;
                    // the Epic stays only as "Back".
                    active={order.id === openOrderId && openTicketId === null}
                    key={order.id} onOpen={drawers.openEpic} order={order} />
                ))}
                {laneRunCards.map(({ run, tickets }) => (
                  <OrderAcquisitionRunCard
                    active={run.id === openRunId}
                    key={run.id}
                    onOpen={drawers.setOpenRunId}
                    priceSourceName={
                      run.priceSourceId ? (view.priceSourceNameById.get(run.priceSourceId) ?? null) : null
                    }
                    run={run}
                    tickets={tickets}
                  />
                ))}
                {visibleEntries.map((entry) => {
                  if (entry.kind === "epic") {
                    const order = entry.order;
                    return (
                      <OrderCard
                        active={order.id === openOrderId && openTicketId === null}
                        compact
                        key={order.id}
                        onOpen={drawers.openEpic}
                        order={order}
                      />
                    );
                  }
                  const ticket = entry.ticket;
                  return (
                    <OrderTicketCard
                      active={ticket.id === openTicketId}
                      assignee={
                        ticket.assigneeCharacterId
                          ? (view.characterById.get(ticket.assigneeCharacterId) ?? null)
                          : null
                      }
                      compact={isCompleteLane}
                      jobProgress={summarizeTicketJobs(jobsByTicketId.get(ticket.id) ?? [], nowMs)}
                      key={ticket.id}
                      onDragEnd={drag.endDrag}
                      onDragStart={drag.startDrag}
                      onOpen={drawers.openTicketFromCard}
                      onToggleSelect={selection.toggleSelected}
                      selectable={selectionMode}
                      selectionEnabled={isSelectable(ticket, selectedTickets, {
                        hasJobs: (jobsByTicketId.get(ticket.id)?.length ?? 0) > 0,
                      })}
                      selected={selectedTicketIds.has(ticket.id)}
                      ticket={ticket}
                    />
                  );
                })}
                {acqGroups.map((group) => {
                  const key = `${group.key}:${lane.status}`;
                  return (
                    <OrderAcqGroupCard
                      activeTicketId={openTicketId}
                      group={group}
                      key={key}
                      expanded={expandedGroups.has(key)}
                      onOpenTicket={drawers.openTicketFromCard}
                      onSelectAll={selection.selectMany}
                      onTicketDragEnd={drag.endDrag}
                      onTicketDragStart={drag.startDrag}
                      onToggleExpand={() => toggleGroup(key)}
                      onToggleSelect={selection.toggleSelected}
                      priceSourceName={
                        group.priceSourceId
                          ? (view.priceSourceNameById.get(group.priceSourceId) ?? null)
                          : null
                      }
                      selectedIds={selectedTicketIds}
                      selecting={selectionMode}
                      selectionLocked={selectedTickets.length > 0 && selectionKind(selectedTickets[0]) !== "acquisition"}
                    />
                  );
                })}
                {isCompleteLane && (hiddenEntryCount > 0 || (showAllComplete && laneEntries.length > COMPLETE_LANE_PREVIEW)) ? (
                  <button
                    className="py-1 text-[11px] text-primary hover:opacity-80"
                    onClick={() => setShowAllComplete((current) => !current)}
                    type="button"
                  >
                    {showAllComplete ? "Show fewer" : `Show all ${laneEntries.length}`}
                  </button>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>

      <p className="iw-muted mt-4 text-xs">
        {view.distinctRunIds} runs · {view.totalTickets} tickets · {view.totalOrders} epics
      </p>

      {selectionMode && selectedTickets.length > 0 ? (
        <OrderSelectionBar
          onClear={selection.clearSelection}
          onCombine={() => setCombineDialogOpen(true)}
          onCreateRun={() => setCreateDialogOpen(true)}
          groupSize={selectedGroup?.tickets.length}
          onSelectGroup={
            selectedGroup?.tickets.some((ticket) => !selectedTicketIds.has(ticket.id))
              ? () => selection.addToSelection(selectedGroup.tickets.map((ticket) => ticket.id))
              : undefined
          }
          selectedTickets={selectedTickets}
        />
      ) : null}

      <CreateOrderAcquisitionRunDialog
        onCancel={() => setCreateDialogOpen(false)}
        onCreated={() => {
          setCreateDialogOpen(false);
          selection.exitSelectionMode();
          void reload();
        }}
        open={createDialogOpen}
        priceSourceNameById={view.priceSourceNameById}
        tickets={selectedTickets}
      />

      <CombineTicketsDialog
        characterNameById={view.characterNameById}
        epicNameById={view.orderNameById}
        onCancel={() => setCombineDialogOpen(false)}
        onCombined={(combined) => {
          setCombineDialogOpen(false);
          selection.exitSelectionMode();
          void reload().then(() => drawers.openTicketFromCard(combined.id));
        }}
        open={combineDialogOpen}
        tickets={selectedTickets}
      />

      <CombineSuggestionsDialog
        onClose={(changed) => {
          setSuggestionsOpen(false);
          if (changed) {
            selection.exitSelectionMode();
            void reload();
          }
        }}
        open={suggestionsOpen}
        suggestions={suggestions}
      />

      {openRun ? (
        <OrderAcquisitionRunDrawer
          onChanged={() => void reload()}
          onClose={() => drawers.setOpenRunId(null)}
          priceSourceName={openRunPriceSourceName}
          run={openRun}
          tickets={openRunTickets}
        />
      ) : null}

      {drawers.editorOpen ? (
        <TicketEditor
          builds={state.builds}
          characters={state.characters}
          initialEpicId={drawers.editorInitialEpicId}
          onClose={drawers.closeTicketEditor}
          onCreated={(ticket) => void drawers.handleTicketCreated(ticket)}
          orders={state.orders}
        />
      ) : openTicket ? (
        <OrderTicketDetailDrawer
          characters={state.characters}
          epicName={
            openTicket.orderId
              ? (state.orders.find((order) => order.id === openTicket.orderId)?.displayName ?? null)
              : null
          }
          onBack={drawers.ticketBackToOrderId ? drawers.backFromTicketToEpic : undefined}
          onChanged={() => void reload()}
          onClose={drawers.closeTicket}
          // Switches to that Epic's own Inspector, staying on Board -- the
          // ticket drawer's own Epic-context section triggers this, never
          // a navigation to `/orders/:id`.
          onOpenEpic={openTicket.orderId ? () => drawers.openEpic(openTicket.orderId as string) : undefined}
          jobs={jobsByTicketId.get(openTicket.id)}
          nowMs={nowMs}
          facilityById={facilityById}
          ticket={openTicket}
          {...combinedTicketContext(openTicket, state.tickets, state.orders)}
          onOpenEpicById={drawers.openEpic}
          onOpenTicket={drawers.openTicketFromCard}
        />
      ) : openOrder ? (
        <EpicInspector
          onChanged={() => void reload()}
          onClose={drawers.closeEpic}
          onCreateTicket={() => drawers.openTicketEditor(openOrder.id, openOrder.id)}
          onOpenTicket={drawers.openTicketFromEpic}
          order={openOrder}
          tickets={state.tickets}
        />
      ) : null}
    </div>
  );
}

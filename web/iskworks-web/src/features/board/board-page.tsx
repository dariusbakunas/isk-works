import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";

import { listCharacters, type CharacterRosterEntry } from "../../api/characters";
import {
  listAcquisitionRuns,
  listBuilds,
  listOrders,
  listPriceSources,
  listTickets,
  updateTicketStatus,
  type AcquisitionRun,
  type Build,
  type OrderSummary,
  type PriceSource,
  type Ticket,
  type TicketStatus,
  type TicketSummary,
} from "../../api/industry";
import { Badge, InlineAlert, LoadingState, PageHeader, StatusDot, type Tone } from "../../components/primitives";
import { apiMessage } from "../industry/shared/api-error";
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
import { OrderSelectionBar } from "./order-selection-bar";
import { OrderTicketCard } from "./order-ticket-card";
import { OrderTicketDetailDrawer } from "./order-ticket-detail-drawer";
import { TicketEditor } from "./ticket-editor";
import { canDropTicket, isTicketDraggable } from "./ticket-drag";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | {
      status: "ready";
      tickets: TicketSummary[];
      runs: AcquisitionRun[];
      orders: OrderSummary[];
      priceSources: PriceSource[];
      characters: CharacterRosterEntry[];
      builds: Build[];
    };

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

export function BoardPage() {
  // `?ticket=<id>` / `?epic=<id>` deep-links -- read once at mount to seed
  // which Ticket Inspector or Epic Inspector opens (e.g. "Navigate to
  // ticket" from the Inventory Reservations tab, which has no other route
  // to a bare Ticket). Not kept in sync afterward: every other way of
  // opening one (a card click) is plain state, same as `openRunId`, so this
  // only seeds the initial value and is cleared on close.
  const [params, setParams] = useSearchParams();
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [showArchivedOrders, setShowArchivedOrders] = useState(false);
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedTicketIds, setSelectedTicketIds] = useState<Set<string>>(new Set());
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [openRunId, setOpenRunId] = useState<string | null>(null);
  const [openOrderId, setOpenOrderId] = useState<string | null>(() => params.get("epic"));
  const [openTicketId, setOpenTicketId] = useState<string | null>(() => params.get("ticket"));
  // Set only when the currently-open ticket was opened from within the Epic
  // Inspector named here -- the tiny in-rail navigation stack's one frame of
  // "back" state. Null when a ticket was opened directly from a Board card,
  // which is what keeps that path's Close going to no inspector at all
  // (rather than resurrecting an Epic Inspector that was never open).
  const [ticketBackToOrderId, setTicketBackToOrderId] = useState<string | null>(null);
  // The canonical Ticket editor -- opening it always replaces whatever else
  // is in the rail (same "one thing in the rail" rule as Epic/Ticket).
  // `editorBackToEpicId` is set only when opened from within an Epic
  // Inspector's own "Create ticket" action, so Cancel/Close returns there
  // (mirrors the Ticket Inspector's own onBack) and a successful create
  // opens the new Ticket with that same Epic as its Back target.
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorInitialEpicId, setEditorInitialEpicId] = useState<string | null>(null);
  const [editorBackToEpicId, setEditorBackToEpicId] = useState<string | null>(null);
  const [draggingTicketId, setDraggingTicketId] = useState<string | null>(null);
  const [dragOverLane, setDragOverLane] = useState<TicketStatus | null>(null);
  const [dragError, setDragError] = useState<string | null>(null);
  // Keyed `${priceSourceId}:${status}` -- collapsed by default, the whole
  // point of grouping being to cut Board noise down from one card per
  // ticket.
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  // Board filters -- a distinct state dimension from Inspector selection
  // (`openOrderId`/`openTicketId` above). Derived live from the URL on
  // every render (never mirrored into local state) so they can never drift
  // from what's bookmarked/shared/reloaded; `?epicFilter=`/`?assigneeFilter=`/
  // `?typeFilter=`/`?q=` are deliberately distinct from the existing
  // `?epic=`/`?ticket=` Inspector-selection params -- see board-filters.ts's
  // own doc for why those were never renamed or overloaded.
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

  function load() {
    // Fetches everything once (small, personal-scale dataset), including
    // every archived Order -- "Show archived" applies purely client-side
    // afterward, so toggling it is instant with no refetch. `listCharacters`
    // reads only already-synced/cached connected-character data (no ESI
    // call), the same roster the Ticket editor's Assignee picker needs --
    // loaded here so opening the editor never waits on its own fetch.
    // `listBuilds` is the same Build picker data source the Manufacturing/
    // Reaction TicketEditor needs -- loaded here for the same reason (no
    // per-open fetch), never recalculated per dropdown row.
    return Promise.all([
      listTickets(),
      listAcquisitionRuns(),
      listOrders("all"),
      listPriceSources(),
      listCharacters(),
      listBuilds(),
    ])
      .then(([tickets, runs, orders, priceSources, characters, builds]) => {
        setState({ status: "ready", tickets, runs, orders, priceSources, characters, builds });
      })
      .catch((requestError) => {
        setState({ status: "error", message: apiMessage(requestError) });
      });
  }

  useEffect(() => {
    void load();
  }, []);

  // The Board's tiny in-rail navigation stack (Epic <-> Ticket) -- one frame
  // deep, plain state, no history/router involvement. Clicking an Epic or
  // Ticket card never navigates (see order-card.tsx / order-ticket-card.tsx);
  // it only ever changes which of these panels is open, so every Board
  // filter/search/scroll/loaded-data stays exactly as it was.
  function openEpic(orderId: string) {
    setOpenOrderId(orderId);
    setOpenTicketId(null);
    setTicketBackToOrderId(null);
  }

  function closeEpic() {
    setOpenOrderId(null);
    if (params.has("epic")) {
      const next = new URLSearchParams(params);
      next.delete("epic");
      setParams(next);
    }
  }

  // A ticket opened directly from a Board card (not via an Epic) has no
  // "back" frame -- closing it goes to no inspector at all, so any Epic
  // Inspector that happened to be open underneath is cleared too.
  function openTicketFromCard(ticketId: string) {
    setOpenTicketId(ticketId);
    setOpenOrderId(null);
    setTicketBackToOrderId(null);
  }

  // A ticket opened from within the currently-open Epic Inspector's own
  // Tickets list -- `openOrderId` is left as-is (it's what "Back" returns
  // to); only the Ticket Inspector takes over the rail in the meantime.
  function openTicketFromEpic(ticketId: string) {
    setTicketBackToOrderId(openOrderId);
    setOpenTicketId(ticketId);
  }

  function backFromTicketToEpic() {
    setOpenTicketId(null);
    setTicketBackToOrderId(null);
    if (params.has("ticket")) {
      const next = new URLSearchParams(params);
      next.delete("ticket");
      setParams(next);
    }
  }

  function closeTicket() {
    setOpenTicketId(null);
    setOpenOrderId(null);
    setTicketBackToOrderId(null);
    if (params.has("ticket") || params.has("epic")) {
      const next = new URLSearchParams(params);
      next.delete("ticket");
      next.delete("epic");
      setParams(next);
    }
  }

  // `backToEpicId` is only passed by Epic Inspector's own "Create ticket"
  // action -- everywhere else (Board-level "Create ticket") it's omitted,
  // so Cancel/Close goes to no inspector at all, same as a ticket opened
  // directly from a Board card.
  function openTicketEditor(initialEpicId: string | null = null, backToEpicId: string | null = null) {
    setOpenTicketId(null);
    setOpenOrderId(null);
    setTicketBackToOrderId(null);
    setEditorInitialEpicId(initialEpicId);
    setEditorBackToEpicId(backToEpicId);
    setEditorOpen(true);
  }

  function closeTicketEditor() {
    setEditorOpen(false);
    if (editorBackToEpicId) openEpic(editorBackToEpicId);
  }

  // Refetches first so the newly created ticket is already present in
  // `state.tickets` by the time the Ticket Inspector opens for it.
  async function handleTicketCreated(ticket: Ticket) {
    setEditorOpen(false);
    await load();
    setTicketBackToOrderId(editorBackToEpicId);
    setOpenOrderId(editorBackToEpicId);
    setOpenTicketId(ticket.id);
  }

  function exitSelectionMode() {
    setSelectionMode(false);
    setSelectedTicketIds(new Set());
  }

  function toggleSelected(ticketId: string) {
    setSelectedTicketIds((current) => {
      const next = new Set(current);
      if (next.has(ticketId)) {
        next.delete(ticketId);
      } else {
        next.add(ticketId);
      }
      return next;
    });
  }

  // Group-level "select all" / header checkbox: already-all-selected clears
  // the set, otherwise selects every id -- shared by an ACQ GROUP's header
  // checkbox and its expanded "Select N actionable" button.
  function selectMany(ticketIds: string[]) {
    setSelectedTicketIds((current) => {
      const next = new Set(current);
      const allSelected = ticketIds.every((id) => next.has(id));
      for (const id of ticketIds) {
        if (allSelected) next.delete(id);
        else next.add(id);
      }
      return next;
    });
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

  function patchTicketStatus(ticketId: string, status: TicketStatus) {
    setState((current) => {
      if (current.status !== "ready") return current;
      return { ...current, tickets: current.tickets.map((t) => (t.id === ticketId ? { ...t, status } : t)) };
    });
  }

  // Moving a card between lanes is a bare workflow-status write and nothing
  // else (see ticket-drag.ts): it
  // never calls /start, /complete, or /cancel, so it has no inventory,
  // cascade, or Order/Run side effect, and it works in both directions.
  // Optimistically repaints the card, fires `PATCH /api/tickets/:id`, then
  // resyncs (ACQ-group re-derivation etc. is still cheaper to reload than
  // to predict). On failure the optimistic patch is rolled back and the
  // existing InlineAlert surfaces the message.
  async function handleTicketDrop(ticketId: string, targetStatus: TicketStatus) {
    if (state.status !== "ready") return;
    const ticket = state.tickets.find((t) => t.id === ticketId);
    if (!ticket || !isTicketDraggable(ticket)) return;
    if (!canDropTicket(ticket.status, targetStatus)) return;

    const originalStatus = ticket.status;
    setDragError(null);
    patchTicketStatus(ticketId, targetStatus);
    try {
      await updateTicketStatus(ticketId, targetStatus);
      void load();
    } catch (requestError) {
      patchTicketStatus(ticketId, originalStatus);
      setDragError(apiMessage(requestError));
    }
  }

  const selectedTickets: Ticket[] =
    state.status === "ready" ? state.tickets.filter((ticket) => selectedTicketIds.has(ticket.id)) : [];

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

  const draggingTicket = draggingTicketId ? state.tickets.find((t) => t.id === draggingTicketId) ?? null : null;
  const draggingTicketDraggable = draggingTicket !== null && isTicketDraggable(draggingTicket);

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
        <button
          aria-pressed={selectionMode}
          className={selectionMode ? "iw-button-primary" : "iw-button-secondary"}
          onClick={() => (selectionMode ? exitSelectionMode() : setSelectionMode(true))}
          type="button"
        >
          {selectionMode ? "Cancel Selection" : "Select"}
        </button>
        <button className="iw-button-primary" onClick={() => openTicketEditor()} type="button">
          Create ticket
        </button>
      </div>

      {dragError ? (
        <div className="mb-3">
          <InlineAlert title="Could not update ticket status">{dragError}</InlineAlert>
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

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3" data-testid="board-lanes">
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
          const isEligibleLane =
            draggingTicket !== null &&
            draggingTicketDraggable &&
            canDropTicket(draggingTicket.status, lane.status);
          const isHoverLane = isEligibleLane && dragOverLane === lane.status;
          const laneHighlight = isHoverLane
            ? laneHoverClasses[lane.tone]
            : isEligibleLane
              ? laneEligibleClasses[lane.tone]
              : "border-border";
          return (
            <div
              className={`rounded-[2px] border transition-colors ${laneHighlight}`}
              data-drag-eligible={isEligibleLane ? "true" : "false"}
              data-testid={`board-lane-${lane.status}`}
              key={lane.status}
              onDragLeave={() => setDragOverLane((current) => (current === lane.status ? null : current))}
              onDragOver={(event) => {
                if (!isEligibleLane) return;
                event.preventDefault();
                if (dragOverLane !== lane.status) setDragOverLane(lane.status);
              }}
              onDrop={(event) => {
                event.preventDefault();
                setDragOverLane(null);
                if (isEligibleLane && draggingTicket) {
                  void handleTicketDrop(draggingTicket.id, lane.status);
                }
              }}
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
                {laneOrderCards.map((order) => (
                  <OrderCard key={order.id} onOpen={openEpic} order={order} />
                ))}
                {laneRunCards.map(({ run, tickets }) => (
                  <OrderAcquisitionRunCard
                    key={run.id}
                    onOpen={setOpenRunId}
                    priceSourceName={
                      run.priceSourceId ? (view.priceSourceNameById.get(run.priceSourceId) ?? null) : null
                    }
                    run={run}
                    tickets={tickets}
                  />
                ))}
                {nonAcqTickets.map((ticket) => (
                  <OrderTicketCard
                    assignee={
                      ticket.assigneeCharacterId
                        ? (view.characterById.get(ticket.assigneeCharacterId) ?? null)
                        : null
                    }
                    key={ticket.id}
                    onDragEnd={() => {
                      setDraggingTicketId(null);
                      setDragOverLane(null);
                    }}
                    onDragStart={setDraggingTicketId}
                    onOpen={openTicketFromCard}
                    onToggleSelect={toggleSelected}
                    selectable={selectionMode}
                    selected={selectedTicketIds.has(ticket.id)}
                    ticket={ticket}
                  />
                ))}
                {acqGroups.map((group) => {
                  const key = `${group.key}:${lane.status}`;
                  return (
                    <OrderAcqGroupCard
                      group={group}
                      key={key}
                      expanded={expandedGroups.has(key)}
                      onOpenTicket={openTicketFromCard}
                      onSelectAll={selectMany}
                      onTicketDragEnd={() => {
                        setDraggingTicketId(null);
                        setDragOverLane(null);
                      }}
                      onTicketDragStart={setDraggingTicketId}
                      onToggleExpand={() => toggleGroup(key)}
                      onToggleSelect={toggleSelected}
                      priceSourceName={
                        group.priceSourceId
                          ? (view.priceSourceNameById.get(group.priceSourceId) ?? null)
                          : null
                      }
                      selectedIds={selectedTicketIds}
                      selecting={selectionMode}
                    />
                  );
                })}
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
          onClear={() => setSelectedTicketIds(new Set())}
          onCreateRun={() => setCreateDialogOpen(true)}
          selectedTickets={selectedTickets}
        />
      ) : null}

      <CreateOrderAcquisitionRunDialog
        onCancel={() => setCreateDialogOpen(false)}
        onCreated={() => {
          setCreateDialogOpen(false);
          exitSelectionMode();
          void load();
        }}
        open={createDialogOpen}
        priceSourceNameById={view.priceSourceNameById}
        tickets={selectedTickets}
      />

      {openRun ? (
        <OrderAcquisitionRunDrawer
          onChanged={() => void load()}
          onClose={() => setOpenRunId(null)}
          priceSourceName={openRunPriceSourceName}
          run={openRun}
          tickets={openRunTickets}
        />
      ) : null}

      {editorOpen ? (
        <TicketEditor
          builds={state.builds}
          characters={state.characters}
          initialEpicId={editorInitialEpicId}
          onClose={closeTicketEditor}
          onCreated={(ticket) => void handleTicketCreated(ticket)}
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
          orders={state.orders}
          onBack={ticketBackToOrderId ? backFromTicketToEpic : undefined}
          onChanged={() => void load()}
          onClose={closeTicket}
          // Switches to that Epic's own Inspector, staying on Board -- the
          // ticket drawer's own Epic-context section triggers this, never
          // a navigation to `/orders/:id`.
          onOpenEpic={openTicket.orderId ? () => openEpic(openTicket.orderId as string) : undefined}
          ticket={openTicket}
        />
      ) : openOrder ? (
        <EpicInspector
          onChanged={() => void load()}
          onClose={closeEpic}
          onCreateTicket={() => openTicketEditor(openOrder.id, openOrder.id)}
          onOpenTicket={openTicketFromEpic}
          order={openOrder}
          tickets={state.tickets}
        />
      ) : null}
    </div>
  );
}

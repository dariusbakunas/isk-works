import type { OrderSummary, TicketKind, TicketSummary } from "../../api/industry";

// Board filters only ever change what the Board *shows* -- they never
// read/write status, Epic membership, assignee, Build links, recording
// state, inventory, or AcquisitionRuns. See board-page.tsx's own
// separation of "Board filters" (this module) from "Inspector selection"
// (`?ticket=`/`?epic=` -- which entity's drawer is open), a distinct state
// dimension that must never be coupled to these.
//
// `""` always means "no restriction from this filter" (All Epics / All
// assignees / All types / no search) -- never a sentinel needing a second
// null check.
export interface BoardFilterState {
  /** `""` = All Epics, `"none"` = No Epic, else an Order/Epic id. */
  epicFilter: string;
  /** `""` = All assignees, `"unassigned"` = Unassigned, else a connected
   * character's connectionId. */
  assigneeFilter: string;
  /** `""` = All types, else a canonical `TicketKind`. */
  typeFilter: TicketKind | "";
  /** Free text, matched case-insensitively as a trimmed substring. */
  search: string;
}

export const EMPTY_BOARD_FILTERS: BoardFilterState = {
  epicFilter: "",
  assigneeFilter: "",
  typeFilter: "",
  search: "",
};

// Distinct, unambiguous param names -- chosen specifically so they never
// collide with the Board's existing `?epic=`/`?ticket=` *inspector*
// selection params (which mean "open this Epic/Ticket's inspector", a
// different state dimension entirely; see module doc above). No rename or
// migration of those params was needed.
const PARAM_EPIC_FILTER = "epicFilter";
const PARAM_ASSIGNEE_FILTER = "assigneeFilter";
const PARAM_TYPE_FILTER = "typeFilter";
const PARAM_SEARCH = "q";

const VALID_TICKET_KINDS: ReadonlySet<string> = new Set<TicketKind>([
  "generic",
  "acquisition",
  "manufacturing",
  "reaction",
]);

export function parseBoardFiltersFromParams(params: URLSearchParams): BoardFilterState {
  const typeRaw = params.get(PARAM_TYPE_FILTER) ?? "";
  return {
    epicFilter: params.get(PARAM_EPIC_FILTER) ?? "",
    assigneeFilter: params.get(PARAM_ASSIGNEE_FILTER) ?? "",
    typeFilter: VALID_TICKET_KINDS.has(typeRaw) ? (typeRaw as TicketKind) : "",
    search: params.get(PARAM_SEARCH) ?? "",
  };
}

/**
 * Returns a new `URLSearchParams` with exactly one filter key set (or
 * removed, when `value` is `""`) -- every other param (inspector
 * selection included) passes through untouched. Callers pass the result to
 * `setSearchParams(..., { replace: true })` so typing in Search or
 * changing a select doesn't spam browser history with one entry per
 * keystroke/change.
 */
export function withBoardFilterParam(
  params: URLSearchParams,
  key: "epicFilter" | "assigneeFilter" | "typeFilter" | "search",
  value: string,
): URLSearchParams {
  const paramName = {
    epicFilter: PARAM_EPIC_FILTER,
    assigneeFilter: PARAM_ASSIGNEE_FILTER,
    typeFilter: PARAM_TYPE_FILTER,
    search: PARAM_SEARCH,
  }[key];
  const next = new URLSearchParams(params);
  if (value === "") next.delete(paramName);
  else next.set(paramName, value);
  return next;
}

/** Strips every filter param, leaving inspector-selection params (and
 * anything else) untouched. */
export function withoutBoardFilters(params: URLSearchParams): URLSearchParams {
  const next = new URLSearchParams(params);
  next.delete(PARAM_EPIC_FILTER);
  next.delete(PARAM_ASSIGNEE_FILTER);
  next.delete(PARAM_TYPE_FILTER);
  next.delete(PARAM_SEARCH);
  return next;
}

export function hasActiveBoardFilters(filters: BoardFilterState): boolean {
  return (
    filters.epicFilter !== "" ||
    filters.assigneeFilter !== "" ||
    filters.typeFilter !== "" ||
    filters.search.trim() !== ""
  );
}

export interface BoardFilterContext {
  /** Order/Epic id -> display name, for the Epic-title search field. */
  orderNameById: Map<string, string>;
  /** Connected-character connectionId -> character name, for the
   * assignee-name search field. */
  characterNameById: Map<string, string>;
}

function matchesEpic(ticket: TicketSummary, epicFilter: string): boolean {
  if (epicFilter === "") return true;
  if (epicFilter === "none") return ticket.orderId === null;
  return ticket.orderId === epicFilter;
}

function matchesAssignee(ticket: TicketSummary, assigneeFilter: string): boolean {
  if (assigneeFilter === "") return true;
  if (assigneeFilter === "unassigned") return ticket.assigneeCharacterId === null;
  return ticket.assigneeCharacterId === assigneeFilter;
}

function matchesType(ticket: TicketSummary, typeFilter: TicketKind | ""): boolean {
  if (typeFilter === "") return true;
  return ticket.kind === typeFilter;
}

// Deliberately narrow -- captured name, notes, display id, the assignee's
// name, and the containing Epic's title (so searching "Weekend" surfaces
// every ticket under the "Weekend Production" Epic even when the ticket's
// own title doesn't mention it). Never the execution snapshot or any
// nested planning field -- see the task's own "keep it predictable" note.
function matchesSearch(ticket: TicketSummary, needle: string, context: BoardFilterContext): boolean {
  if (needle === "") return true;
  const haystacks = [
    ticket.capturedName,
    ticket.notes,
    ticket.displayId,
    ticket.assigneeCharacterId ? (context.characterNameById.get(ticket.assigneeCharacterId) ?? "") : "",
    ticket.orderId ? (context.orderNameById.get(ticket.orderId) ?? "") : "",
  ];
  return haystacks.some((haystack) => haystack.toLowerCase().includes(needle));
}

function ticketMatchesBoardFilters(
  ticket: TicketSummary,
  filters: BoardFilterState,
  context: BoardFilterContext,
): boolean {
  const needle = filters.search.trim().toLowerCase();
  return (
    matchesEpic(ticket, filters.epicFilter) &&
    matchesAssignee(ticket, filters.assigneeFilter) &&
    matchesType(ticket, filters.typeFilter) &&
    matchesSearch(ticket, needle, context)
  );
}

export function filterBoardTickets(
  tickets: TicketSummary[],
  filters: BoardFilterState,
  context: BoardFilterContext,
): TicketSummary[] {
  return tickets.filter((ticket) => ticketMatchesBoardFilters(ticket, filters, context));
}

/**
 * Whether an Epic/Order card should render under the active filters.
 * Deliberately NOT "does this Order's own metadata match" -- Epic cards
 * have no assignee/kind/search-relevant body of their own here, so once a
 * filter is engaged, visibility becomes a function of whether at least one
 * child Ticket qualifies:
 *
 * - No filter active at all: unconditionally visible -- an Epic is a real
 *   work item in its own right and must keep rendering exactly as it does
 *   today (including the rare case of zero Tickets, e.g. immediately after
 *   creation), regardless of this module's existence.
 * - `epicFilter: "none"` (No Epic) hides every Epic card outright -- an
 *   Epic can never satisfy "no Epic".
 * - An Epic-specific filter hides every *other* Epic's card.
 * - Otherwise (Search/Assignee/Type engaged), an Epic's card is visible
 *   only if it has >=1 Ticket matching every *other* active filter -- an
 *   Epic filter is itself irrelevant to that check since `order` already
 *   is (or isn't) the selected one by the rule above. This keeps "no
 *   unrelated/empty Epic cards floating" true, including when an Epic is
 *   explicitly selected but happens to have zero Tickets matching the
 *   *other* filters.
 */
export function boardEpicCardIsVisible(
  order: OrderSummary,
  allTickets: TicketSummary[],
  filters: BoardFilterState,
  context: BoardFilterContext,
): boolean {
  if (!hasActiveBoardFilters(filters)) return true;
  if (filters.epicFilter === "none") return false;
  if (filters.epicFilter !== "" && filters.epicFilter !== order.id) return false;
  const filtersIgnoringEpic: BoardFilterState = { ...filters, epicFilter: "" };
  return allTickets.some(
    (ticket) => ticket.orderId === order.id && ticketMatchesBoardFilters(ticket, filtersIgnoringEpic, context),
  );
}

export const TICKET_TYPE_FILTER_OPTIONS: { value: TicketKind | ""; label: string }[] = [
  { value: "", label: "All types" },
  { value: "generic", label: "Generic" },
  { value: "acquisition", label: "Acquisition" },
  { value: "manufacturing", label: "Manufacturing" },
  { value: "reaction", label: "Reaction" },
];

import { useEffect, useState } from "react";

import { listCharacters, type CharacterRosterEntry, type IndustryJob } from "../../api/characters";
import {
  listAcquisitionRuns,
  listBuilds,
  listFacilities,
  listOrders,
  listPriceSources,
  listTicketJobProgress,
  listTickets,
  type AcquisitionRun,
  type Build,
  type FacilityProfile,
  type OrderSummary,
  type PriceSource,
  type TicketStatus,
  type TicketSummary,
} from "../../api/industry";
import { apiMessage } from "../../api/api-error";

export type BoardLoadState =
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

/**
 * Everything the Board shows, loaded once and reloaded after each change.
 * A reload keeps the current Board on screen until the new data lands, and
 * callers can await it (e.g. to open a ticket they just created).
 *
 * Two optional extras load beside it and never blank the Board when they
 * fail: ESI job progress for In Progress tickets (refetched every few
 * minutes, its bars ticking locally) and the facility names the ticket
 * drawer shows.
 */
export function useBoardData() {
  const [state, setState] = useState<BoardLoadState>({ status: "loading" });
  // ESI jobs matched to In Progress production tickets, by ticket id, and
  // the clock their progress bars are drawn at.
  const [jobsByTicketId, setJobsByTicketId] = useState<Map<string, IndustryJob[]>>(new Map());
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [facilityById, setFacilityById] = useState<Map<string, FacilityProfile>>(new Map());

  function loadJobProgress() {
    return listTicketJobProgress()
      .then((rows) => setJobsByTicketId(new Map(rows.map((row) => [row.ticketId, row.jobs]))))
      .catch(() => {
        // Keep the last known bars; the Board itself is unaffected.
      });
  }

  // Bars tick locally from each job's fixed dates; the matches themselves
  // are refetched every few minutes to pick up newly started jobs.
  useEffect(() => {
    const tick = window.setInterval(() => setNowMs(Date.now()), 30_000);
    const refetch = window.setInterval(() => void loadJobProgress(), 5 * 60_000);
    return () => {
      window.clearInterval(tick);
      window.clearInterval(refetch);
    };
  }, []);

  // Names the planned facility on generated step tickets (see the ticket
  // drawer). Optional context, so a failure never blanks the Board.
  function loadFacilities() {
    return listFacilities()
      .then((facilities) => setFacilityById(new Map(facilities.map((facility) => [facility.id, facility]))))
      .catch(() => {
        // The drawer just omits the Installation section.
      });
  }

  function reload() {
    void loadJobProgress();
    void loadFacilities();
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
    void reload();
    // Loads once on mount; every later load is an explicit `reload()`.
  }, []);

  /** Repaints one ticket's status locally, ahead of (or to undo) a save. */
  function patchTicketStatus(ticketId: string, status: TicketStatus) {
    setState((current) => {
      if (current.status !== "ready") return current;
      return { ...current, tickets: current.tickets.map((t) => (t.id === ticketId ? { ...t, status } : t)) };
    });
  }

  return { state, reload, patchTicketStatus, jobsByTicketId, nowMs, facilityById };
}

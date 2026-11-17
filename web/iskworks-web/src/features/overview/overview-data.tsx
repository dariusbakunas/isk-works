import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import { getCalendarMilestones, type CalendarMilestone } from "../../api/calendar";
import { listCharacters, type CharacterRosterEntry } from "../../api/characters";
import { getFinanceAnalytics, type FinanceAnalytics } from "../../api/finance-analytics";
import { getInventoryRunway, type InventoryRunway } from "../../api/inventory-analytics";
import { listOrders, type OrderSummary } from "../../api/industry/orders";
import { getActiveJobs, type ActiveJobs } from "../../api/jobs";
import { getMarketOrders, type MarketOrdersOverview } from "../../api/market-orders";
import { getPlanetary, type PlanetaryOverview } from "../../api/planetary";
import { useLoad, type Loaded } from "../../hooks/use-load";

const AGENDA_WINDOW_MS = 48 * 3_600_000;
/** The Money panel's window: the last 30 days, today included. */
const FINANCE_DAYS = 30;

const isoDate = (ms: number) => new Date(ms).toISOString().slice(0, 10);

export interface OverviewSources {
  characters: Loaded<CharacterRosterEntry[]>;
  jobs: Loaded<ActiveJobs>;
  market: Loaded<MarketOrdersOverview>;
  planetary: Loaded<PlanetaryOverview>;
  runway: Loaded<InventoryRunway>;
  calendar: Loaded<CalendarMilestone[]>;
  finance: Loaded<FinanceAnalytics>;
  orders: Loaded<OrderSummary[]>;
}

interface OverviewData {
  sources: OverviewSources;
  /** `null` until the roster loads; the other sources wait for it. */
  hasCharacters: boolean | null;
  reloadAll: () => void;
  /** When every source last finished loading, or `null` before that. */
  updatedAt: number | null;
}

const OverviewDataContext = createContext<OverviewData | null>(null);

/** Loads each Overview source once and shares it with every panel that reads
 * it (the attention strip and the panels read the same jobs, market, …).
 * Nothing but the roster is fetched until the workspace has a character. */
export function OverviewDataProvider({ children }: { children: ReactNode }) {
  const characters = useLoad(() => listCharacters(), "characters");
  const hasCharacters = characters.data === null ? null : characters.data.length > 0;
  const gate = <T,>(load: (signal: AbortSignal) => Promise<T>) => (hasCharacters ? load : null);

  const jobs = useLoad(gate((signal) => getActiveJobs(signal)), "jobs");
  const market = useLoad(gate((signal) => getMarketOrders(signal)), "market");
  const planetary = useLoad(gate(() => getPlanetary()), "planetary");
  const runway = useLoad(gate((signal) => getInventoryRunway(null, signal)), "runway");
  // The window starts when the request does, so the key stays stable.
  const calendar = useLoad(
    gate(() => {
      const from = Date.now();
      return getCalendarMilestones(new Date(from), new Date(from + AGENDA_WINDOW_MS));
    }),
    "calendar",
  );
  // Inventory buys are build inputs, not spending (as on Finance › Analytics).
  const finance = useLoad(
    gate((signal) => {
      const today = Date.now();
      return getFinanceAnalytics(
        {
          connectionIds: [],
          dateFrom: isoDate(today - (FINANCE_DAYS - 1) * 86_400_000),
          dateTo: isoDate(today),
          granularity: "day",
          comparePrevious: true,
          category: null,
          excludeInventoryBuys: true,
        },
        signal,
      );
    }),
    "finance",
  );
  const orders = useLoad(gate(() => listOrders("active")), "orders");

  const sources = useMemo(
    () => ({ characters, jobs, market, planetary, runway, calendar, finance, orders }),
    [characters, jobs, market, planetary, runway, calendar, finance, orders],
  );
  const all = Object.values(sources) as Loaded<unknown>[];
  const pending = all.some((source) => source.loading || source.refreshing) || hasCharacters === null;

  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  useEffect(() => {
    if (!pending) setUpdatedAt(Date.now());
  }, [pending]);

  const reloadAll = useCallback(() => {
    for (const source of [characters, jobs, market, planetary, runway, calendar, finance, orders]) source.reload();
  }, [characters, jobs, market, planetary, runway, calendar, finance, orders]);

  const value = useMemo(
    () => ({ sources, hasCharacters, reloadAll, updatedAt }),
    [sources, hasCharacters, reloadAll, updatedAt],
  );
  return <OverviewDataContext.Provider value={value}>{children}</OverviewDataContext.Provider>;
}

export function useOverviewData(): OverviewData {
  const value = useContext(OverviewDataContext);
  if (!value) throw new Error("useOverviewData must be used inside OverviewDataProvider");
  return value;
}

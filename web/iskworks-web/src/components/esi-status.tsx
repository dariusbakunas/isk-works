import { Cloud } from "lucide-react";
import { useEffect, useState } from "react";

import { getEsiStatus } from "../api/health";

/** How often to check while ESI is up. The API answers from memory then. */
const ESI_STATUS_POLL_MS = 60_000;
/** Bounds on how soon to check again during downtime. */
const DOWNTIME_POLL_MIN_MS = 15_000;
const DOWNTIME_POLL_MAX_MS = 60_000;

const DOWNTIME_TITLE =
  "EVE's servers are down (daily downtime or maintenance). ESI updates are paused and resume automatically when Tranquility is back.";

/**
 * Whether EVE's servers are down: the daily downtime (11:00 UTC, usually a
 * few minutes) or an unscheduled one. ESI requests are paused during it.
 * Polls the API, which answers from memory while ESI is up, and clears on its
 * own once the API sees ESI healthy again.
 */
export function useEsiDowntime(): boolean {
  const [downtime, setDowntime] = useState(false);

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const check = async () => {
      let next = ESI_STATUS_POLL_MS;
      try {
        const status = await getEsiStatus();
        if (!active) return;
        setDowntime(status.downtime);
        if (status.downtime) {
          const suggested = (status.retryAfterSeconds ?? 30) * 1000;
          next = Math.min(Math.max(suggested, DOWNTIME_POLL_MIN_MS), DOWNTIME_POLL_MAX_MS);
        }
      } catch {
        /* an API outage is surfaced elsewhere; keep the last known state */
      }
      if (active) timer = setTimeout(check, next);
    };
    void check();
    return () => {
      active = false;
      if (timer) clearTimeout(timer);
    };
  }, []);

  return downtime;
}

/**
 * The navigation rail's ESI row: always present, so a paused feed is noticed
 * where people look for it. The label stays in the DOM for screen readers
 * when the rail is collapsed; the dot carries the state visually.
 */
export function EsiStatusItem({ expanded }: { expanded: boolean }) {
  const downtime = useEsiDowntime();
  const label = downtime ? "ESI data paused" : "ESI connected";

  return (
    <div className="flex h-8 w-full items-center" role="status" title={downtime ? DOWNTIME_TITLE : label}>
      <span className="relative grid w-[51px] shrink-0 place-items-center text-muted">
        <Cloud aria-hidden="true" className="h-4 w-4" />
        {downtime ? <i aria-hidden="true" className="absolute right-[15px] top-[1px] h-1.5 w-1.5 rounded-full bg-warning" /> : null}
      </span>
      <span className={expanded ? `truncate text-[10px] ${downtime ? "text-warning" : "text-foreground"}` : "sr-only"}>{label}</span>
    </div>
  );
}

import { ServerOff } from "lucide-react";
import { useEffect, useState } from "react";

import { getEsiStatus } from "../api/health";

/** How often to check while ESI is up. The API answers from memory then. */
const ESI_STATUS_POLL_MS = 60_000;
/** Bounds on how soon to check again during downtime. */
const DOWNTIME_POLL_MIN_MS = 15_000;
const DOWNTIME_POLL_MAX_MS = 60_000;

/**
 * Header badge shown while EVE's servers are in their daily downtime
 * (11:00 UTC, usually a few minutes). ESI requests are paused during it, so
 * this explains why EVE data isn't refreshing; it disappears on its own once
 * the API sees ESI healthy again.
 */
export function EsiDowntimeBadge() {
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

  if (!downtime) return null;

  return (
    <span
      className="inline-flex shrink-0 items-center gap-1 rounded border border-warning/50 bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-warning"
      role="status"
      title="EVE's servers are in their daily downtime. ESI updates are paused and resume automatically when Tranquility is back, usually within a few minutes."
    >
      <ServerOff aria-hidden="true" className="h-3.5 w-3.5" />
      <span className="hidden sm:inline">EVE downtime</span>
      <span className="sm:hidden">DT</span>
    </span>
  );
}

import { Globe2 } from "lucide-react";
import { Link } from "react-router";

import type { PlanetaryOverview } from "../../api/planetary";
import { IskCompact } from "../../components/money";
import { CharacterName } from "../../observability/private";
import { ago } from "../jobs/jobs-format";
import { useNow } from "../jobs/use-jobs";
import { inLabel } from "./agenda-panel";
import { useOverviewData } from "./overview-data";
import { panelState, PanelFrame } from "./panel-frame";
import { Stat } from "./stat";

const DAY_MS = 24 * 3_600_000;

const ALERTS = [
  { key: "expired", label: "Expired", className: "border-danger/50 bg-danger/10 text-danger" },
  { key: "storageFull", label: "Storage full", className: "border-warning/50 bg-warning/10 text-warning" },
  { key: "starved", label: "Starved", className: "border-warning/50 bg-warning/10 text-warning" },
] as const;

function noScope(overview: PlanetaryOverview): boolean {
  return overview.characters.length > 0 && overview.characters.every((character) => !character.scopeGranted);
}

/** PI income, colonies, the next extractor to restart and colony alerts
 * (spec §3.7, §9.7). */
export function PlanetaryPanel({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const nowMs = useNow();
  const planetary = sources.planetary;
  const data = planetary.data;
  const next = data?.summary.nextAction ?? null;
  const nextAt = next ? Date.parse(next.at) : null;
  const empty = (
    <span>
      No PI scope granted ·{" "}
      <Link className="text-primary hover:underline" to="/characters">Connect</Link>
    </span>
  );

  return (
    <PanelFrame
      className={className}
      link={{ to: "/planetary" }}
      stamp={data?.priceObservedAt ? `prices ${ago(data.priceObservedAt, nowMs)}` : undefined}
      state={panelState(planetary, noScope, empty)}
      title="Planetary"
    >
      {data ? (
        <>
          <div aria-label="Planetary totals" className="flex flex-wrap border-b border-border" role="group">
            <Stat label="Income / month"><IskCompact value={data.summary.iskPerMonth} /></Stat>
            <Stat label="Planets">{data.summary.planetCount}</Stat>
            <Stat label="Characters">{data.summary.characterCount}</Stat>
          </div>
          <div className="grid gap-2 p-3">
            {next && nextAt !== null ? (
              <p
                className={`flex min-h-9 items-center gap-2 rounded-[2px] border px-3 text-xs ${nextAt - nowMs < DAY_MS ? "border-warning/50 bg-warning/10 text-warning" : "border-border text-foreground"}`}
              >
                <Globe2 aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                <span className="min-w-0 truncate">
                  <b className="font-semibold">Extractor expires:</b> <CharacterName name={next.characterName} /> · {next.planetName} ·{" "}
                  <span className="font-mono">{inLabel(nextAt, nowMs)}</span>
                </span>
              </p>
            ) : (
              <p className="text-xs text-muted">No running extractors.</p>
            )}
            <div className="flex flex-wrap gap-2">
              {ALERTS.map((alert) => {
                const count = data.summary.alerts[alert.key];
                return (
                  <span
                    key={alert.key}
                    className={`rounded-[2px] border px-2 py-0.5 text-[0.6875rem] ${count > 0 ? alert.className : "border-border text-muted"}`}
                  >
                    <span>{alert.label}</span> <b className="font-mono">{count}</b>
                  </span>
                );
              })}
            </div>
          </div>
        </>
      ) : null}
    </PanelFrame>
  );
}

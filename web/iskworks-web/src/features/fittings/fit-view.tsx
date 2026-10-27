import { ChevronLeft, CircleCheck, CircleHelp, CircleX, Copy, CopyPlus, Trash2, type LucideIcon } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import { createFit, deleteFit, exportFitText, type FitDetail, type FitPilot, type FitStatsView, type FlyabilityStatus } from "../../api/fittings";
import { EveTypeImage } from "../../components/eve-type-image";
import { ConfirmDialog, InlineAlert } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import { EngineUnavailableHint, PilotPortrait, useEngineAvailable, useFittingsContext } from "./fit-shared";
import { capacitorLabel, formatPercent, formatStat } from "./fittings-format";
import { ModuleTable } from "./module-table";
import { PilotsPanel } from "./pilots-panel";
import { StatsRail } from "./stats-rail";
import { useFit, useFitParams, useFitPilots, useFitStats } from "./use-fittings";

const STATUS: Record<FlyabilityStatus | "unknown", { label: string; icon: LucideIcon; className: string }> = {
  canUseAll: { label: "can use all", icon: CircleCheck, className: "text-positive" },
  canFlyHull: { label: "hull only", icon: CircleCheck, className: "text-warning" },
  cannot: { label: "can't fly", icon: CircleX, className: "text-danger" },
  unknown: { label: "skills unknown", icon: CircleHelp, className: "text-muted" },
};

export function statusOf(pilot: Pick<FitPilot, "status">) {
  return STATUS[pilot.status ?? "unknown"];
}

const chipClass = (selected: boolean) =>
  `flex h-8 shrink-0 items-center gap-1.5 rounded-md border px-1.5 text-xs ${
    selected ? "border-primary bg-primary/15 text-foreground" : "border-border bg-background text-muted hover:text-foreground"
  }`;

function PilotChips({ pilots, selected, onSelect }: { pilots: FitPilot[]; selected: string; onSelect: (pilot: string) => void }) {
  return (
    <div aria-label="Pilot" className="flex items-center gap-1 overflow-x-auto" role="group">
      {pilots.map((pilot) => {
        const status = statusOf(pilot);
        const Icon = status.icon;
        return (
          <button
            aria-label={`${pilot.name}: ${status.label}`}
            aria-pressed={selected === pilot.connectionId}
            className={chipClass(selected === pilot.connectionId)}
            key={pilot.connectionId}
            onClick={() => onSelect(pilot.connectionId)}
            type="button"
          >
            <PilotPortrait connectionId={pilot.connectionId} name={pilot.name} />
            <span {...PRIVATE_ATTR}>{pilot.name}</span>
            <Icon aria-hidden="true" className={`h-3 w-3 ${status.className}`} />
          </button>
        );
      })}
      {(["allV", "allZero"] as const).map((choice) => (
        <button
          aria-pressed={selected === choice}
          className={`${chipClass(selected === choice)} font-mono`}
          key={choice}
          onClick={() => onSelect(choice)}
          type="button"
        >
          {choice === "allV" ? "All V" : "All 0"}
        </button>
      ))}
    </div>
  );
}

/** What a create needs from a saved fit: the items without their names. */
function copyDraft(fit: FitDetail) {
  return {
    name: `${fit.name} (copy)`,
    description: fit.description,
    shipTypeId: fit.ship.typeId,
    items: fit.items.map(({ typeId, slotKind, slotIndex, quantity, state, chargeTypeId, mutation }) => ({
      typeId,
      slotKind,
      slotIndex,
      quantity,
      state,
      chargeTypeId,
      ...(mutation ? { mutation } : {}),
    })),
  };
}

function Header({
  fit,
  pilots,
  onChanged,
}: {
  fit: FitDetail;
  pilots: FitPilot[];
  onChanged: () => void;
}) {
  const navigate = useNavigate();
  const { meta } = useFittingsContext();
  const engineAvailable = useEngineAvailable();
  const { pilot, profile, setPilot, setProfile } = useFitParams();
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shipName = fit.ship.name ?? `Type ${fit.ship.typeId}`;

  const run = async (action: () => Promise<void>) => {
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "That didn't work.");
    }
  };

  return (
    <div className="space-y-2" data-testid="fit-header">
      <div className="flex flex-wrap items-center gap-3">
        <Link className="flex items-center gap-0.5 text-xs text-muted hover:text-foreground" to="/fittings">
          <ChevronLeft aria-hidden="true" className="h-3.5 w-3.5" />
          Fits
        </Link>
        <EveTypeImage size={64} typeId={fit.ship.typeId} typeName={shipName} variation="render" />
        <div className="min-w-0">
          <p className="text-[0.6875rem] text-muted">{shipName}</p>
          <h1 {...PRIVATE_ATTR} className="iw-title">{fit.name}</h1>
          <p className="text-[0.625rem] uppercase tracking-wide text-muted">{fit.ship.groupName}</p>
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-2">
          <button
            className="iw-button-secondary gap-1.5"
            disabled={!engineAvailable}
            onClick={() =>
              void run(async () => {
                await navigator.clipboard.writeText(await exportFitText(fit.id));
                setCopied(true);
                window.setTimeout(() => setCopied(false), 2000);
              })
            }
            type="button"
          >
            <Copy aria-hidden="true" className="h-3.5 w-3.5" />
            Copy for in-game import
          </button>
          {copied ? <span className="text-xs text-positive" role="status">Copied</span> : null}
          <button
            className="iw-button-secondary gap-1.5"
            onClick={() =>
              void run(async () => {
                const copy = await createFit(copyDraft(fit));
                onChanged();
                navigate(`/fittings/${copy.id}`);
              })
            }
            type="button"
          >
            <CopyPlus aria-hidden="true" className="h-3.5 w-3.5" />
            Duplicate
          </button>
          <button className="iw-button-danger gap-1.5" onClick={() => setConfirmDelete(true)} type="button">
            <Trash2 aria-hidden="true" className="h-3.5 w-3.5" />
            Delete
          </button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <PilotChips onSelect={setPilot} pilots={pilots} selected={pilot} />
        <label className="ml-auto flex items-center gap-1.5 text-xs text-muted">
          Damage profile
          <select className="iw-input" onChange={(event) => setProfile(event.target.value)} value={profile}>
            {(meta?.damageProfiles ?? []).map((preset) => (
              <option key={preset.id} value={preset.id}>{preset.name}</option>
            ))}
          </select>
        </label>
      </div>
      {error ? <InlineAlert title="That didn't work">{error}</InlineAlert> : null}
      <ConfirmDialog
        confirmLabel="Delete fit"
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() =>
          void run(async () => {
            setConfirmDelete(false);
            await deleteFit(fit.id);
            onChanged();
            navigate("/fittings");
          })
        }
        open={confirmDelete}
        title={`Delete ${fit.name}?`}
      >
        The fit is removed for everyone in the workspace. Skill plans made from it stay.
      </ConfirmDialog>
    </div>
  );
}

/** The headline stats in one line, for screens too narrow for the rail
 * beside the table (the rail follows below there). */
function SummaryStrip({ view }: { view: FitStatsView }) {
  const { defense, offense, capacitor, navigation } = view.stats;
  const cap = capacitor.state.kind === "stable"
    ? formatPercent(capacitor.state.level)
    : capacitorLabel(capacitor.state).replace("Depletes in ", "");
  return (
    <p
      className="iw-panel flex flex-wrap gap-x-3 gap-y-1 px-3 py-2 font-mono text-xs text-muted xl:hidden"
      data-testid="stats-summary"
    >
      <span>EHP <span className="text-foreground">{formatStat(defense.ehp)}</span></span>
      <span>DPS <span className="text-foreground">{formatStat(offense.totalDps, 1)}</span></span>
      <span>Cap <span className="text-foreground">{cap}</span></span>
      <span className="text-foreground">{formatStat(navigation.maxVelocity, 1)} m/s</span>
    </p>
  );
}

export function FitView({ onChanged }: { onChanged: () => void }) {
  const { fitId = "" } = useParams();
  const engineAvailable = useEngineAvailable();
  const { pilot, profile, setPilot } = useFitParams();
  const fit = useFit(fitId);
  const pilots = useFitPilots(fitId, profile);
  const stats = useFitStats(fitId, pilot, profile, engineAvailable);
  const statsView: FitStatsView | null = engineAvailable ? stats.data : null;

  if (fit.error) return <InlineAlert title="Couldn't load the fit">{fit.error.message}</InlineAlert>;
  if (!fit.data) return <p className="text-sm text-muted">Loading fit…</p>;

  return (
    <div className="space-y-3">
      <Header fit={fit.data} onChanged={onChanged} pilots={pilots.data ?? []} />
      {engineAvailable ? null : <EngineUnavailableHint />}
      {stats.error ? <InlineAlert title="Couldn't calculate the fit">{stats.error.message}</InlineAlert> : null}
      {statsView ? <SummaryStrip view={statsView} /> : null}
      <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_320px]">
        <div className="min-w-0 space-y-3">
          <ModuleTable fit={fit.data} stats={statsView} />
          <PilotsPanel
            fit={fit.data}
            onSelect={setPilot}
            pilots={pilots.data ?? []}
            selected={pilot}
          />
        </div>
        <StatsRail fitId={fit.data.id} stats={statsView} />
      </div>
    </div>
  );
}

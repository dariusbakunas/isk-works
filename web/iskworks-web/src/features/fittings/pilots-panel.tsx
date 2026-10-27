import { ChevronDown, Users } from "lucide-react";
import { useId, useState } from "react";
import { Link } from "react-router";

import { createFitSkillPlan, type CreatedSkillPlan, type FitDetail, type FitPilot } from "../../api/fittings";
import { formatRemainingMs, skillLevelNumeral } from "../characters/characters-formatters";
import { PilotPortrait } from "./fit-shared";
import { capacitorLabel, formatStat } from "./fittings-format";

const STATUS_CHIPS = {
  canUseAll: { label: "Can use all", className: "border-positive/60 text-positive" },
  canFlyHull: { label: "Hull only", className: "border-warning/60 text-warning" },
  cannot: { label: "Can't fly", className: "border-danger/60 text-danger" },
  unknown: { label: "Skills not synced", className: "border-border text-muted" },
} as const;

const level = (value: number) => (value > 0 ? skillLevelNumeral(value) : "—");
const duration = (seconds: number | null) => (seconds === null ? "—" : seconds === 0 ? "—" : formatRemainingMs(seconds * 1000));

function MissingSkills({ fit, pilot }: { fit: FitDetail; pilot: FitPilot }) {
  const [plan, setPlan] = useState<CreatedSkillPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const missing = pilot.flyability?.missing ?? [];

  async function create() {
    setSaving(true);
    setError(null);
    try {
      setPlan(await createFitSkillPlan(fit.id, pilot.connectionId));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Couldn't create the plan.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="border-b border-border bg-background/40 px-3 py-2 md:pl-10">
      <ul aria-label={`Missing skills for ${pilot.name}`} className="space-y-1 text-xs">
        {missing.map((skill) => (
          <li className="grid grid-cols-[minmax(0,1fr)_88px_88px] items-center gap-2" key={skill.skillTypeId}>
            <span className={skill.omegaRequired ? "text-danger" : "text-foreground"}>{skill.name}</span>
            <span className="font-mono text-muted">{level(skill.activeLevel)} → {level(skill.requiredLevel)}</span>
            <span className={`text-right font-mono ${skill.omegaRequired ? "text-danger" : "text-muted"}`}>
              {skill.omegaRequired ? "Needs Omega" : duration(skill.seconds)}
            </span>
          </li>
        ))}
      </ul>
      <div className="mt-2 flex items-center justify-end gap-2">
        {pilot.flyability?.remainingSeconds ? (
          <span className="mr-auto text-xs text-warning">Total {duration(pilot.flyability.remainingSeconds)}</span>
        ) : null}
        {error ? <span className="text-xs text-danger">{error}</span> : null}
        {plan ? (
          <Link className="text-xs text-primary hover:underline" to={`/skills/plans/${plan.id}`}>
            Open {plan.name}
          </Link>
        ) : (
          <button className="iw-button-primary" disabled={saving} onClick={() => void create()} type="button">
            Create skill plan
          </button>
        )}
      </div>
    </div>
  );
}

/** Every workspace character against the fit: who can fly it, what each
 * lacks, and the headline stats on its own skills. */
export function PilotsPanel({
  fit,
  pilots,
  selected,
  onSelect,
}: {
  fit: FitDetail;
  pilots: FitPilot[];
  selected: string;
  onSelect: (pilot: string) => void;
}) {
  const headingId = useId();
  const [expanded, setExpanded] = useState<string | null>(null);
  const first = pilots[0];
  const best = first?.status === "canUseAll" ? first : null;
  // Nobody can use it all: point at whoever is closest.
  const closest = best
    ? null
    : pilots
        .filter((pilot) => pilot.flyability?.remainingSeconds)
        .sort((a, b) => (a.flyability!.remainingSeconds ?? 0) - (b.flyability!.remainingSeconds ?? 0))[0];

  return (
    <section aria-labelledby={headingId} className="iw-panel">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <Users aria-hidden="true" className="h-3.5 w-3.5 text-primary" />
        <h2 className="text-xs font-semibold uppercase tracking-wide" id={headingId}>Pilots</h2>
        {best ? (
          <span className="rounded-[2px] border border-positive/60 px-1.5 py-px text-[0.625rem] text-positive">Best: {best.name}</span>
        ) : closest ? (
          <span className="rounded-[2px] border border-warning/60 px-1.5 py-px text-[0.625rem] text-warning">
            Closest: {closest.name}
          </span>
        ) : null}
        <span className="ml-auto hidden text-[0.625rem] text-muted sm:inline">Pick a pilot to recalculate the fit</span>
      </div>
      <div className="hidden grid-cols-[minmax(160px,1fr)_96px_56px_80px_repeat(5,64px)] px-3 py-1 text-[0.5625rem] uppercase tracking-wide text-muted md:grid">
        <span>Pilot</span>
        <span>Status</span>
        <span className="text-right">Missing</span>
        <span className="text-right">Training</span>
        <span className="text-right">EHP</span>
        <span className="text-right">DPS</span>
        <span className="text-right">Cap</span>
        <span className="text-right">Speed</span>
        <span className="text-right">Align</span>
      </div>
      {pilots.map((pilot) => {
        const chip = STATUS_CHIPS[pilot.status ?? "unknown"];
        const missing = pilot.flyability?.missing.length ?? 0;
        const isClosest = closest?.connectionId === pilot.connectionId;
        return (
          <div key={pilot.connectionId}>
            <div className="flex items-stretch border-b border-border">
              <button
                aria-label={`${pilot.name}, ${chip.label}`}
                aria-pressed={selected === pilot.connectionId}
                className={`grid min-h-10 flex-1 grid-cols-[1fr_auto] items-center gap-x-2 px-3 py-1 text-left text-xs hover:bg-panel-strong md:grid-cols-[minmax(160px,1fr)_96px_56px_80px_repeat(5,64px)] ${
                  selected === pilot.connectionId ? "bg-primary/10" : ""
                }`}
                data-pilot={pilot.connectionId}
                onClick={() => onSelect(pilot.connectionId)}
                type="button"
              >
                <span className="flex items-center gap-2">
                  <PilotPortrait connectionId={pilot.connectionId} name={pilot.name} size={32} />
                  <span data-private="">{pilot.name}</span>
                </span>
                <span>
                  <span className={`rounded-[2px] border px-1.5 py-px text-[0.625rem] ${chip.className}`}>{chip.label}</span>
                </span>
                <span className={`hidden text-right font-mono md:block ${missing ? "text-danger" : "text-muted"}`}>
                  {pilot.flyability ? missing : "—"}
                </span>
                <span className={`hidden text-right font-mono md:block ${isClosest ? "text-warning" : "text-muted"}`}>
                  {pilot.flyability ? duration(pilot.flyability.remainingSeconds) : "—"}
                </span>
                {pilot.stats ? (
                  <>
                    <span className="hidden text-right font-mono md:block">{formatStat(pilot.stats.ehp)}</span>
                    <span className="hidden text-right font-mono md:block">{formatStat(pilot.stats.dps, 1)}</span>
                    <span className="hidden text-right font-mono md:block" title={capacitorLabel(pilot.stats.capacitor)}>
                      {pilot.stats.capacitor.kind === "stable"
                        ? `${formatStat(pilot.stats.capacitor.level * 100)}%`
                        : capacitorLabel(pilot.stats.capacitor).replace("Depletes in ", "")}
                    </span>
                    <span className="hidden text-right font-mono md:block">{formatStat(pilot.stats.maxVelocity, 1)}</span>
                    <span className="hidden text-right font-mono md:block">{formatStat(pilot.stats.alignSeconds, 2)}</span>
                  </>
                ) : (
                  <span className="col-span-5 hidden text-right text-muted md:block">—</span>
                )}
              </button>
              {missing ? (
                <button
                  aria-expanded={expanded === pilot.connectionId}
                  aria-label={`Missing skills for ${pilot.name}`}
                  className="px-2 text-muted hover:text-foreground"
                  onClick={() => setExpanded((current) => (current === pilot.connectionId ? null : pilot.connectionId))}
                  type="button"
                >
                  <ChevronDown
                    aria-hidden="true"
                    className={`h-3.5 w-3.5 transition ${expanded === pilot.connectionId ? "rotate-180" : ""}`}
                  />
                </button>
              ) : (
                <span className="w-[30px]" />
              )}
            </div>
            {expanded === pilot.connectionId ? <MissingSkills fit={fit} pilot={pilot} /> : null}
          </div>
        );
      })}
    </section>
  );
}

import { ChevronDown, Users } from "lucide-react";
import { Fragment, useId, useState } from "react";
import { Link } from "react-router";

import { createFitSkillPlan, type CreatedSkillPlan, type FitDetail, type FitPilot } from "../../api/fittings";
import { formatRemainingMs, skillLevelNumeral } from "../../lib/characters-formatters";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../components/operational-table";
import { PilotPortrait } from "./fit-shared";
import { capacitorLabel, formatStat } from "./fittings-format";
import { apiMessage } from "../../api/api-error";

const STATUS_CHIPS = {
  canUseAll: { label: "Can use all", className: "border-positive/60 text-positive" },
  canFlyHull: { label: "Hull only", className: "border-warning/60 text-warning" },
  cannot: { label: "Can't fly", className: "border-danger/60 text-danger" },
  unknown: { label: "Skills not synced", className: "border-border text-muted" },
} as const;

// The stat columns hide below tablet width, leaving pilot, status and the
// missing-skills toggle.
const COLUMNS: OperationalColumn[] = [
  { key: "pilot", label: "Pilot", width: "minmax(10rem,1fr)" },
  { key: "status", label: "Status", width: "7.5rem" },
  { key: "missing", label: "Missing", width: "4.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "training", label: "Training", width: "5.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "ehp", label: "EHP", width: "5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "dps", label: "DPS", width: "4.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "cap", label: "Cap", width: "4.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "speed", label: "Speed", width: "4.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "align", label: "Align", width: "4.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "expand", label: "", width: "2rem", align: "right" },
];

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
      setError(apiMessage(reason));
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
          <Link className="text-xs text-primary hover:underline" to={`/skills/plans/${plan.id}?character=${pilot.connectionId}`}>
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
      <OperationalTable ariaLabel="Pilots" columns={COLUMNS} onSelectRow={onSelect} selectedRowKey={selected}>
        <tbody>
          {pilots.map((pilot) => {
            const chip = STATUS_CHIPS[pilot.status ?? "unknown"];
            const missing = pilot.flyability?.missing.length ?? 0;
            const isClosest = closest?.connectionId === pilot.connectionId;
            const open = expanded === pilot.connectionId;
            const stats = pilot.stats;
            const stat = (value: (s: NonNullable<typeof stats>) => string, title?: string) =>
              stats ? <span className="truncate text-xs" title={title}>{value(stats)}</span> : <span className="text-xs text-muted">—</span>;
            return (
              <Fragment key={pilot.connectionId}>
                <OperationalTableRow
                  cells={{
                    pilot: (
                      <span className="flex min-w-0 items-center gap-2 text-xs">
                        <PilotPortrait connectionId={pilot.connectionId} name={pilot.name} size={32} />
                        <span className="truncate" data-private="">{pilot.name}</span>
                      </span>
                    ),
                    status: (
                      <span className={`truncate rounded-[2px] border px-1.5 py-px text-[0.625rem] ${chip.className}`}>{chip.label}</span>
                    ),
                    missing: (
                      <span className={`text-xs ${missing ? "text-danger" : "text-muted"}`}>{pilot.flyability ? missing : "—"}</span>
                    ),
                    training: (
                      <span className={`truncate text-xs ${isClosest ? "text-warning" : "text-muted"}`}>
                        {pilot.flyability ? duration(pilot.flyability.remainingSeconds) : "—"}
                      </span>
                    ),
                    ehp: stat((s) => formatStat(s.ehp)),
                    dps: stat((s) => formatStat(s.dps, 1)),
                    cap: stat(
                      (s) =>
                        s.capacitor.kind === "stable"
                          ? `${formatStat(s.capacitor.level * 100)}%`
                          : capacitorLabel(s.capacitor).replace("Depletes in ", ""),
                      stats ? capacitorLabel(stats.capacitor) : undefined,
                    ),
                    speed: stat((s) => formatStat(s.maxVelocity, 1)),
                    align: stat((s) => formatStat(s.alignSeconds, 2)),
                    expand: missing ? (
                      <button
                        aria-expanded={open}
                        aria-label={`Missing skills for ${pilot.name}`}
                        className="text-muted hover:text-foreground"
                        // Opening the list doesn't pick the pilot.
                        onClick={(event) => {
                          event.stopPropagation();
                          setExpanded((current) => (current === pilot.connectionId ? null : pilot.connectionId));
                        }}
                        onKeyDown={(event) => event.stopPropagation()}
                        type="button"
                      >
                        <ChevronDown aria-hidden="true" className={`h-3.5 w-3.5 transition ${open ? "rotate-180" : ""}`} />
                      </button>
                    ) : null,
                  }}
                  rowKey={pilot.connectionId}
                />
                {open ? (
                  <tr className="grid grid-cols-[var(--operational-columns)]">
                    <td className="col-span-full p-0">
                      <MissingSkills fit={fit} pilot={pilot} />
                    </td>
                  </tr>
                ) : null}
              </Fragment>
            );
          })}
        </tbody>
      </OperationalTable>
    </section>
  );
}

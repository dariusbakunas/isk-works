import { Battery, Crosshair, Gauge, HelpCircle, Navigation, Shield, Swords, X } from "lucide-react";
import { useEffect, useId, useState, type ReactNode } from "react";

import {
  explainFitAttribute,
  type AttributeView,
  type FitStats,
  type FitStatsView,
  type LayerStats,
  type Usage,
} from "../../api/fittings";
import { capacitorLabel, formatDistance, formatPercent, formatStat } from "./fittings-format";
import { useFitParams } from "./use-fittings";

/** A stat the "Why?" view can explain: its label and dogma attribute. */
interface Explainable {
  title: string;
  attribute: string;
}

const WHY = {
  cpu: { title: "CPU output", attribute: "cpuOutput" },
  powergrid: { title: "Powergrid output", attribute: "powerOutput" },
  calibration: { title: "Calibration", attribute: "upgradeCapacity" },
  capacitor: { title: "Capacitor capacity", attribute: "capacitorCapacity" },
  recharge: { title: "Capacitor recharge time", attribute: "rechargeRate" },
  shieldHp: { title: "Shield HP", attribute: "shieldCapacity" },
  armorHp: { title: "Armor HP", attribute: "armorHP" },
  hullHp: { title: "Hull HP", attribute: "hp" },
  maxTargets: { title: "Max locked targets", attribute: "maxLockedTargets" },
  lockRange: { title: "Lock range", attribute: "maxTargetRange" },
  scanResolution: { title: "Scan resolution", attribute: "scanResolution" },
  signature: { title: "Signature radius", attribute: "signatureRadius" },
  velocity: { title: "Max velocity", attribute: "maxVelocity" },
  inertia: { title: "Inertia modifier", attribute: "agility" },
  warp: { title: "Warp speed multiplier", attribute: "warpSpeedMultiplier" },
} satisfies Record<string, Explainable>;

/** How a modifier changes the value, the way the client words it. */
export function formatModifier(operator: string, value: number): string {
  const amount = formatStat(Math.abs(value), 3);
  switch (operator) {
    case "post_percent":
      return `${value >= 0 ? "+" : "−"}${amount}%`;
    case "pre_mul":
    case "post_mul":
      return `×${formatStat(value, 4)}`;
    case "pre_div":
    case "post_div":
      return `÷${formatStat(value, 4)}`;
    case "mod_add":
      return `${value >= 0 ? "+" : "−"}${amount}`;
    case "mod_sub":
      return `−${amount}`;
    case "pre_assign":
    case "post_assign":
      return `= ${formatStat(value, 3)}`;
    default:
      return `${operator} ${formatStat(value, 3)}`;
  }
}

function WhyPopover({ fitId, explain, onClose }: { fitId: string; explain: Explainable; onClose: () => void }) {
  const titleId = useId();
  const { pilot, profile } = useFitParams();
  const [view, setView] = useState<AttributeView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    explainFitAttribute(fitId, { attribute: explain.attribute, pilot, profile }, controller.signal)
      .then(setView)
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : "Couldn't explain it.");
      });
    return () => controller.abort();
  }, [fitId, explain.attribute, pilot, profile]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <section
      aria-labelledby={titleId}
      className="absolute right-0 z-30 mt-1 w-72 rounded-md border border-border bg-panel p-3 text-xs shadow-2xl"
      role="dialog"
    >
      <div className="flex items-center gap-1.5">
        <HelpCircle aria-hidden="true" className="h-3.5 w-3.5 text-primary" />
        <h3 className="font-semibold" id={titleId}>Why {explain.title}?</h3>
        <button aria-label="Close" className="ml-auto text-muted hover:text-foreground" onClick={onClose} type="button">
          <X aria-hidden="true" className="h-3.5 w-3.5" />
        </button>
      </div>
      {error ? <p className="mt-2 text-danger">{error}</p> : null}
      {view ? (
        <div className="mt-2 space-y-1">
          <div className="flex justify-between text-muted">
            <span>Base</span>
            <span className="font-mono" data-testid="why-base">{formatStat(view.base, 2)}</span>
          </div>
          {view.sources.length === 0 ? <p className="text-muted">Nothing changes it.</p> : null}
          {view.sources.map((source, position) => (
            <div className={source.applied ? "" : "opacity-50"} key={position}>
              <div className="flex justify-between gap-2">
                <span className="truncate">{source.name ?? source.from.kind}</span>
                <span className="font-mono">{formatModifier(source.operator, source.value)}</span>
              </div>
              {source.penalty !== null ? (
                <p className="text-[0.625rem] text-muted">stacking penalised to {formatPercent(source.penalty)}</p>
              ) : null}
              {source.applied ? null : <p className="text-[0.625rem] text-muted">not applied (module state too low)</p>}
            </div>
          ))}
          <div className="flex justify-between border-t border-border pt-1 font-semibold">
            <span>Final</span>
            <span className="font-mono" data-testid="why-final">{formatStat(view.value, 2)}</span>
          </div>
        </div>
      ) : error ? null : (
        <p className="mt-2 text-muted">Loading…</p>
      )}
    </section>
  );
}

/** How a stat moved from the saved fit; nothing when it didn't. Green when
 * the change is good (more EHP, less CPU used, a faster align). */
function Delta({ value, digits = 0, higherIsGood = true, testId }: { value: number | null; digits?: number; higherIsGood?: boolean; testId: string }) {
  if (value === null || Math.abs(value) < 10 ** -(digits + 1)) return null;
  const good = higherIsGood ? value > 0 : value < 0;
  return (
    <span className={`ml-1.5 font-mono text-[0.625rem] ${good ? "text-positive" : "text-danger"}`} data-testid={testId}>
      {value > 0 ? "+" : "−"}
      {formatStat(Math.abs(value), digits)}
    </span>
  );
}

export function Section({ title, icon, children }: { title: string; icon: ReactNode; children: ReactNode }) {
  return (
    <section className="border-b border-border px-3 py-2.5 last:border-b-0">
      <h2 className="mb-1.5 flex items-center gap-1.5 text-[0.625rem] font-semibold uppercase tracking-wide text-muted">
        {icon}
        {title}
      </h2>
      <div className="space-y-1">{children}</div>
    </section>
  );
}

function Line({
  label,
  value,
  why,
  fitId,
}: {
  label: string;
  value: ReactNode;
  why?: Explainable;
  fitId: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative flex items-center justify-between gap-2 text-xs">
      <span className="flex items-center gap-1 text-muted">
        {label}
        {why ? (
          <button
            aria-label={`Why? ${label}`}
            className="text-muted/60 hover:text-primary"
            onClick={() => setOpen((current) => !current)}
            type="button"
          >
            <HelpCircle aria-hidden="true" className="h-3 w-3" />
          </button>
        ) : null}
      </span>
      <span className="font-mono text-foreground">{value}</span>
      {open && why ? <WhyPopover explain={why} fitId={fitId} onClose={() => setOpen(false)} /> : null}
    </div>
  );
}

function Meter({
  label,
  usage,
  unit,
  why,
  fitId,
  delta = null,
}: {
  label: string;
  usage: Usage;
  unit: string;
  why: Explainable;
  fitId: string;
  /** Change in what's used since the saved fit. */
  delta?: number | null;
}) {
  const over = usage.used > usage.total;
  const percent = usage.total > 0 ? Math.min(100, (usage.used / usage.total) * 100) : usage.used > 0 ? 100 : 0;
  return (
    <div className="space-y-0.5">
      <Line
        fitId={fitId}
        label={label}
        value={
          <>
            <span className={over ? "text-danger" : undefined}>{`${formatStat(usage.used, 1)} / ${formatStat(usage.total, 1)} ${unit}`}</span>
            <Delta digits={1} higherIsGood={false} testId={`delta-${label.toLowerCase()}`} value={delta} />
          </>
        }
        why={why}
      />
      <div
        aria-label={label}
        aria-valuemax={usage.total}
        aria-valuemin={0}
        aria-valuenow={usage.used}
        className="h-1 rounded bg-panel-strong"
        data-over={over ? "true" : "false"}
        role="meter"
      >
        <div className={`h-1 rounded ${over ? "bg-danger" : "bg-primary"}`} style={{ width: `${percent}%` }} />
      </div>
    </div>
  );
}

const DAMAGE_COLUMNS = [
  { key: "em", label: "EM", className: "bg-sky-500/25" },
  { key: "thermal", label: "Th", className: "bg-red-500/25" },
  { key: "kinetic", label: "Kin", className: "bg-zinc-400/25" },
  { key: "explosive", label: "Exp", className: "bg-amber-500/25" },
] as const;

function ResistGrid({ shield, armor, hull }: { shield: LayerStats; armor: LayerStats; hull: LayerStats }) {
  return (
    <table aria-label="Resists" className="w-full text-[0.6875rem]">
      <thead>
        <tr className="text-muted">
          <th className="text-left font-normal" scope="col"><span className="sr-only">Layer</span></th>
          {DAMAGE_COLUMNS.map((column) => (
            <th className="font-normal" key={column.key} scope="col">{column.label}</th>
          ))}
          <th className="text-right font-normal" scope="col">HP</th>
        </tr>
      </thead>
      <tbody>
        {([
          ["Shield", shield],
          ["Armor", armor],
          ["Hull", hull],
        ] as const).map(([name, layer]) => (
          <tr key={name}>
            <th className="text-left font-normal text-muted" scope="row">{name}</th>
            {DAMAGE_COLUMNS.map((column) => (
              <td className={`px-0.5 text-center font-mono ${column.className}`} key={column.key}>
                {formatPercent(layer.resists[column.key], 0)}
              </td>
            ))}
            <td className="text-right font-mono">{formatStat(layer.hp)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** The calculated stats, in the order the page's rail lists them. While
 * editing, `baseline` is the saved fit's: the main stats show how far the
 * draft moved them. */
export function StatsRail({
  fitId,
  stats: view,
  baseline = null,
  recalculating = false,
  price = null,
}: {
  fitId: string;
  stats: FitStatsView | null;
  baseline?: FitStats | null;
  recalculating?: boolean;
  /** The Price section, first in the rail; prices need no engine. */
  price?: ReactNode;
}) {
  if (!view) {
    return (
      <aside aria-label="Fit stats" className="iw-panel h-fit">
        {price}
        <div className="p-3 text-xs text-muted">
          <p className="font-semibold text-foreground">Stats unavailable</p>
          <p className="mt-1">Without fitting engine data the server can't calculate this fit.</p>
        </div>
      </aside>
    );
  }
  const { resources, capacitor, offense, defense, targeting, navigation, drones } = view.stats;
  const depletes = capacitor.state.kind === "depletes";
  const delta = (pick: (stats: FitStats) => number) => (baseline ? pick(view.stats) - pick(baseline) : null);
  return (
    <aside aria-label="Fit stats" className="iw-panel h-fit xl:sticky xl:top-3">
      {baseline ? (
        <p className="border-b border-border px-3 py-1.5 text-[0.625rem] text-muted" role="status">
          {recalculating ? "Recalculating…" : "Changes from the saved fit"}
        </p>
      ) : null}
      {price}
      <Section icon={<Gauge aria-hidden="true" className="h-3 w-3" />} title="Resources">
        <Meter delta={delta((stats) => stats.resources.cpu.used)} fitId={fitId} label="CPU" unit="tf" usage={resources.cpu} why={WHY.cpu} />
        <Meter delta={delta((stats) => stats.resources.powergrid.used)} fitId={fitId} label="Powergrid" unit="MW" usage={resources.powergrid} why={WHY.powergrid} />
        <Meter delta={delta((stats) => stats.resources.calibration.used)} fitId={fitId} label="Calibration" unit="" usage={resources.calibration} why={WHY.calibration} />
        <Line fitId={fitId} label="Turrets" value={`${resources.turretHardpoints.used} / ${resources.turretHardpoints.total}`} />
        <Line fitId={fitId} label="Launchers" value={`${resources.launcherHardpoints.used} / ${resources.launcherHardpoints.total}`} />
        {resources.droneBay.total > 0 ? (
          <>
            <Line fitId={fitId} label="Drone bandwidth" value={`${formatStat(resources.droneBandwidth.used)} / ${formatStat(resources.droneBandwidth.total)} Mbit/s`} />
            <Line fitId={fitId} label="Drone bay" value={`${formatStat(resources.droneBay.used)} / ${formatStat(resources.droneBay.total)} m³`} />
          </>
        ) : null}
      </Section>
      <Section icon={<Battery aria-hidden="true" className="h-3 w-3" />} title="Capacitor">
        <Line fitId={fitId} label="Capacity" value={`${formatStat(capacitor.capacity)} GJ`} why={WHY.capacitor} />
        <Line fitId={fitId} label="Recharge" value={`${formatStat(capacitor.rechargeSeconds, 1)} s`} why={WHY.recharge} />
        <Line fitId={fitId} label="Peak delta" value={`${formatStat(capacitor.peakDelta, 1)} GJ/s`} />
        <p
          className={`rounded border px-2 py-1 text-xs font-semibold ${
            depletes ? "border-warning/60 bg-warning/10 text-warning" : "border-positive/60 bg-positive/10 text-positive"
          }`}
        >
          {capacitorLabel(capacitor.state)}
        </p>
      </Section>
      <Section icon={<Swords aria-hidden="true" className="h-3 w-3" />} title="Offense">
        <Line fitId={fitId} label="Weapon DPS" value={formatStat(offense.weaponDps, 1)} />
        <Line fitId={fitId} label="With reload" value={formatStat(offense.weaponDpsWithReload, 1)} />
        <Line fitId={fitId} label="Drone DPS" value={formatStat(offense.droneDps, 1)} />
        <Line fitId={fitId} label="Volley" value={formatStat(offense.volley)} />
        <div className="flex items-baseline justify-between pt-1">
          <span className="text-xs text-muted">Total DPS</span>
          <span>
            <span className="font-mono text-lg font-semibold" data-testid="total-dps">{formatStat(offense.totalDps, 1)}</span>
            <Delta digits={1} testId="delta-dps" value={delta((stats) => stats.offense.totalDps)} />
          </span>
        </div>
      </Section>
      <Section icon={<Shield aria-hidden="true" className="h-3 w-3" />} title="Defense">
        <div className="flex items-baseline justify-between">
          <span className="text-xs text-muted">Effective HP</span>
          <span>
            <span className="font-mono text-lg font-semibold" data-testid="total-ehp">{formatStat(defense.ehp)}</span>
            <Delta testId="delta-ehp" value={delta((stats) => stats.defense.ehp)} />
          </span>
        </div>
        <ResistGrid armor={defense.armor} hull={defense.hull} shield={defense.shield} />
        <Line fitId={fitId} label="Shield HP" value={formatStat(defense.shield.hp)} why={WHY.shieldHp} />
        <Line fitId={fitId} label="Armor HP" value={formatStat(defense.armor.hp)} why={WHY.armorHp} />
        <Line fitId={fitId} label="Hull HP" value={formatStat(defense.hull.hp)} why={WHY.hullHp} />
        <Line fitId={fitId} label="Passive shield regen" value={`${formatStat(defense.shieldPassiveRegen, 1)} HP/s`} />
        {defense.shieldBoost ? <Line fitId={fitId} label="Shield boost" value={`${formatStat(defense.shieldBoost, 1)} HP/s`} /> : null}
        {defense.armorRepair ? <Line fitId={fitId} label="Armor repair" value={`${formatStat(defense.armorRepair, 1)} HP/s`} /> : null}
      </Section>
      <Section icon={<Crosshair aria-hidden="true" className="h-3 w-3" />} title="Targeting">
        <Line fitId={fitId} label="Max targets" value={formatStat(targeting.maxTargets)} why={WHY.maxTargets} />
        <Line fitId={fitId} label="Lock range" value={formatDistance(targeting.lockRange)} why={WHY.lockRange} />
        <Line fitId={fitId} label="Scan resolution" value={`${formatStat(targeting.scanResolution)} mm`} why={WHY.scanResolution} />
        <Line fitId={fitId} label="Sensor strength" value={formatStat(targeting.sensorStrength, 1)} />
        <Line fitId={fitId} label="Signature radius" value={`${formatStat(targeting.signatureRadius)} m`} why={WHY.signature} />
      </Section>
      <Section icon={<Navigation aria-hidden="true" className="h-3 w-3" />} title="Navigation">
        <Line
          fitId={fitId}
          label="Max velocity"
          value={
            <>
              {`${formatStat(navigation.maxVelocity, 1)} m/s`}
              <Delta digits={1} testId="delta-velocity" value={delta((stats) => stats.navigation.maxVelocity)} />
            </>
          }
          why={WHY.velocity}
        />
        <Line
          fitId={fitId}
          label="Align time"
          value={
            <>
              {`${formatStat(navigation.alignSeconds, 2)} s`}
              <Delta digits={2} higherIsGood={false} testId="delta-align" value={delta((stats) => stats.navigation.alignSeconds)} />
            </>
          }
        />
        <Line fitId={fitId} label="Inertia" value={formatStat(navigation.inertia, 4)} why={WHY.inertia} />
        <Line fitId={fitId} label="Mass" value={`${formatStat(navigation.mass / 1000)} t`} />
        <Line fitId={fitId} label="Warp speed" value={`${formatStat(navigation.warpSpeed, 2)} AU/s`} why={WHY.warp} />
        {drones.maxActive > 0 && resources.droneBay.total > 0 ? (
          <Line fitId={fitId} label="Drones" value={`${formatStat(drones.active)} / ${formatStat(drones.maxActive)} active`} />
        ) : null}
      </Section>
    </aside>
  );
}

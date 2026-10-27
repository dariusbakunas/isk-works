import type {
  CapacitorState,
  FitListPilot,
  FitSummary,
  Headline,
  ModuleState,
  SlotKind,
} from "../../api/fittings";

const number = (digits: number) =>
  new Intl.NumberFormat("en-US", { maximumFractionDigits: digits, minimumFractionDigits: 0 });

/** A stat with thousands separators, rounded to `digits`. */
export function formatStat(value: number, digits = 0): string {
  return number(digits).format(value);
}

/** Metres, as km from 1 km up. */
export function formatDistance(metres: number): string {
  return metres >= 1000 ? `${formatStat(metres / 1000, 1)} km` : `${formatStat(metres)} m`;
}

/** A share (0.735) as a percent ("73.5%"). */
export function formatPercent(share: number, digits = 1): string {
  return `${formatStat(share * 100, digits)}%`;
}

const oneDecimal = new Intl.NumberFormat("en-US", { maximumFractionDigits: 1, minimumFractionDigits: 1 });

/** A resist (0.6) as a percent with one decimal always ("60.0%"), so a
 * grid of them lines up. */
export function formatResist(share: number): string {
  // The data holds 32-bit floats: 73.75% comes out as 73.7499…%, which
  // should still round up, as pyfa shows it.
  return `${oneDecimal.format(share * 100 + 1e-4)}%`;
}

/** m:ss, or h:mm:ss from an hour up. */
export function formatClock(totalSeconds: number): string {
  const seconds = Math.max(0, Math.round(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = String(seconds % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}

export function capacitorLabel(state: CapacitorState): string {
  return state.kind === "stable"
    ? `Stable ${formatPercent(state.level)}`
    : `Depletes in ${formatClock(state.seconds)}`;
}

/** The module table's Effect cell; "—" for a module without a headline. */
export function formatHeadline(headline: Headline | null): string {
  if (!headline) return "—";
  const { kind, value } = headline;
  switch (kind) {
    case "dps":
      return `${formatStat(value, 1)} DPS`;
    case "shieldRepair":
    case "armorRepair":
      return `${formatStat(value, 1)} HP/s`;
    case "resist":
      return `+${formatStat(value, 1)}% resist`;
    case "velocity":
      return `+${formatStat(value)}% velocity`;
    case "shieldHp":
      return `+${formatStat(value)} shield HP`;
    case "armorHp":
      return `+${formatStat(value)} armor HP`;
    case "capacitor":
      return `+${formatStat(value)} GJ`;
  }
}

const RACK_LABELS: Record<SlotKind, string> = {
  high: "High slots",
  med: "Med slots",
  low: "Low slots",
  rig: "Rig slots",
  subsystem: "Subsystems",
  service: "Service slots",
  fighter_tube: "Fighter tubes",
  fighter_bay: "Fighter bay",
  drone_bay: "Drones",
  cargo: "Cargo",
  implant: "Implants",
  booster: "Boosters",
};

export function rackLabel(kind: SlotKind): string {
  return RACK_LABELS[kind];
}

const STATE_LABELS: Record<ModuleState, string> = {
  offline: "Offline",
  online: "Online",
  active: "Active",
  overload: "Overloaded",
};

export function stateLabel(state: ModuleState): string {
  return STATE_LABELS[state];
}

export interface TreeShip {
  typeId: number;
  name: string;
  fits: FitSummary[];
}

export interface TreeGroup {
  name: string;
  fitCount: number;
  ships: TreeShip[];
}

const byName = <T extends { name: string }>(a: T, b: T) => a.name.localeCompare(b.name);

/** Fits as the "Hulls & Fits" tree: ship group → ship → fits, by name. */
export function fitTree(fits: FitSummary[]): TreeGroup[] {
  const groups = new Map<string, Map<number, TreeShip>>();
  for (const fit of fits) {
    const groupName = fit.ship.groupName || "Unknown group";
    const ships = groups.get(groupName) ?? new Map<number, TreeShip>();
    groups.set(groupName, ships);
    const ship = ships.get(fit.ship.typeId) ?? {
      typeId: fit.ship.typeId,
      name: fit.ship.name || `Type ${fit.ship.typeId}`,
      fits: [],
    };
    ship.fits.push(fit);
    ships.set(fit.ship.typeId, ship);
  }
  return [...groups.entries()]
    .map(([name, ships]) => {
      const sorted = [...ships.values()].sort(byName);
      for (const ship of sorted) ship.fits.sort(byName);
      return { name, fitCount: sorted.reduce((sum, ship) => sum + ship.fits.length, 0), ships: sorted };
    })
    .sort((a, b) => {
      // The fallback group goes last.
      if (a.name === "Unknown group") return 1;
      if (b.name === "Unknown group") return -1;
      return a.name.localeCompare(b.name);
    });
}

/** A fit's characters split for its flyability chips; unknown ones are left out. */
export function pilotsByStatus(pilots: FitListPilot[]) {
  const canUseAll = pilots.filter((pilot) => pilot.status === "canUseAll");
  const hullOnly = pilots.filter((pilot) => pilot.status === "canFlyHull");
  return { canUseAll, hullOnly, anyone: canUseAll.length + hullOnly.length > 0 };
}

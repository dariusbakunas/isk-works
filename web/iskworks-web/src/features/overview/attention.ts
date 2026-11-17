import type { CharacterRosterEntry } from "../../api/characters";
import type { InventoryRunway } from "../../api/inventory-analytics";
import type { ActiveJobs } from "../../api/jobs";
import type { MarketOrdersOverview } from "../../api/market-orders";
import type { PlanetaryOverview } from "../../api/planetary";
import { deriveTrainingState } from "../characters/training-state";

export type AttentionTone = "critical" | "warn" | "info";

export type AttentionChipId =
  | "reconnect"
  | "extractorsExpired"
  | "queueEmpty"
  | "outbid"
  | "epicsBlocked"
  | "storageFull"
  | "starved"
  | "queueEnding"
  | "syncIssues"
  | "jobsReady"
  | "idleSlots"
  | "likelyFilled";

export interface AttentionChip {
  id: AttentionChipId;
  tone: AttentionTone;
  count: number;
  label: string;
  to: string;
}

/** Just what the rules read from a load: a source is ready, failed, or
 * (neither) still loading. */
interface SourceState<T> {
  data: T | null;
  error: Error | null;
}

export interface AttentionSources {
  characters: SourceState<CharacterRosterEntry[]>;
  jobs: SourceState<ActiveJobs>;
  market: SourceState<MarketOrdersOverview>;
  planetary: SourceState<PlanetaryOverview>;
  runway: SourceState<InventoryRunway>;
}

export interface Attention {
  /** Most urgent first: critical, then warn, then info. */
  chips: AttentionChip[];
  /** Sources that failed to load, so their chips can't be shown. */
  unavailable: number;
  /** Some source is still loading: "all clear" can't be claimed yet. */
  pending: boolean;
}

const DAY_MS = 24 * 3_600_000;
const TONE_ORDER: AttentionTone[] = ["critical", "warn", "info"];

interface Rule {
  id: AttentionChipId;
  tone: AttentionTone;
  to: string;
  /** Singular and plural label. */
  label: [string, string];
  count: (sources: AttentionSources, nowMs: number) => number | null;
}

/** `null` while the source isn't loaded: the rule yields no chip. */
function from<K extends keyof AttentionSources>(
  key: K,
  read: (data: NonNullable<AttentionSources[K]["data"]>, nowMs: number) => number,
) {
  return (sources: AttentionSources, nowMs: number) => {
    const data = sources[key].data;
    return data === null ? null : read(data as NonNullable<AttentionSources[K]["data"]>, nowMs);
  };
}

function queueEmpty(character: CharacterRosterEntry, nowMs: number): boolean {
  if (character.trainingQueueScopeMissing) return false;
  const state = deriveTrainingState(character.trainingQueue, nowMs);
  return state.kind === "empty" || state.kind === "cachedQueueExpired";
}

function queueEndsWithinDay(character: CharacterRosterEntry, nowMs: number): boolean {
  if (character.trainingQueueScopeMissing) return false;
  if (deriveTrainingState(character.trainingQueue, nowMs).kind !== "active") return false;
  const ends = Math.max(...character.trainingQueue.map((entry) => (entry.finishDate ? Date.parse(entry.finishDate) : 0)));
  return ends - nowMs < DAY_MS;
}

// In display order within each tone (spec §3.1).
const RULES: Rule[] = [
  {
    id: "reconnect",
    tone: "critical",
    to: "/characters",
    label: ["character needs reconnect", "characters need reconnect"],
    count: from("characters", (characters) => characters.filter((c) => c.health === "reconnectRequired").length),
  },
  {
    id: "extractorsExpired",
    tone: "critical",
    to: "/planetary",
    label: ["extractor expired", "extractors expired"],
    count: from("planetary", (planetary) => planetary.summary.alerts.expired),
  },
  {
    id: "queueEmpty",
    tone: "critical",
    to: "/skills",
    label: ["skill queue empty", "skill queues empty"],
    count: from("characters", (characters, nowMs) => characters.filter((c) => queueEmpty(c, nowMs)).length),
  },
  {
    id: "outbid",
    tone: "warn",
    to: "/market-orders",
    label: ["order outbid", "orders outbid"],
    count: from("market", (market) => market.summary.outbid),
  },
  {
    id: "epicsBlocked",
    tone: "warn",
    to: "/inventory/runway",
    label: ["Epic blocked", "Epics blocked"],
    count: from("runway", (runway) => runway.kpis.epicsBlocked),
  },
  {
    id: "storageFull",
    tone: "warn",
    to: "/planetary",
    label: ["storage full", "storage full"],
    count: from("planetary", (planetary) => planetary.summary.alerts.storageFull),
  },
  {
    id: "starved",
    tone: "warn",
    to: "/planetary",
    label: ["factory starved", "factories starved"],
    count: from("planetary", (planetary) => planetary.summary.alerts.starved),
  },
  {
    id: "queueEnding",
    tone: "warn",
    to: "/skills",
    label: ["skill queue ends < 24h", "skill queues end < 24h"],
    count: from("characters", (characters, nowMs) => characters.filter((c) => queueEndsWithinDay(c, nowMs)).length),
  },
  {
    id: "syncIssues",
    tone: "warn",
    to: "/characters",
    label: ["character with sync issues", "characters with sync issues"],
    count: from("characters", (characters) => characters.filter((c) => c.health === "stale" || c.health === "partialPerms").length),
  },
  {
    id: "jobsReady",
    tone: "info",
    to: "/jobs",
    label: ["job ready to deliver", "jobs ready to deliver"],
    count: from("jobs", (jobs) => jobs.summary.ready),
  },
  {
    id: "idleSlots",
    tone: "info",
    to: "/jobs",
    label: ["idle slot", "idle slots"],
    count: from("jobs", (jobs) =>
      Object.values(jobs.slots).reduce((idle, slot) => idle + (slot.max === null ? 0 : Math.max(0, slot.max - slot.used)), 0),
    ),
  },
  {
    id: "likelyFilled",
    tone: "info",
    to: "/market-orders",
    label: ["order likely filled", "orders likely filled"],
    count: from("market", (market) => market.summary.likelyFilled),
  },
];

/** What on the Overview needs the player now (spec §3.1, §9.9). */
export function attentionChips(sources: AttentionSources, nowMs: number): Attention {
  const states = Object.values(sources) as SourceState<unknown>[];
  const chips = RULES.flatMap((rule): AttentionChip[] => {
    const count = rule.count(sources, nowMs);
    if (!count) return [];
    return [{ id: rule.id, tone: rule.tone, count, to: rule.to, label: rule.label[count === 1 ? 0 : 1] }];
  }).sort((a, b) => TONE_ORDER.indexOf(a.tone) - TONE_ORDER.indexOf(b.tone));
  return {
    chips,
    unavailable: states.filter((state) => state.error !== null).length,
    pending: states.some((state) => state.data === null && state.error === null),
  };
}

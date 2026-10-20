import type { IndustryActivity } from "../../api/characters";
import type { JobCharacter, JobStatus, JobView } from "../../api/jobs";
import type { Tone } from "../../components/primitives";

/** Activities in display (and chart stacking) order. */
export const JOB_ACTIVITIES: IndustryActivity[] = [
  "manufacturing",
  "reaction",
  "materialEfficiencyResearch",
  "timeEfficiencyResearch",
  "copying",
  "invention",
  "reverseEngineering",
  "other",
];

const ACTIVITY_META: Record<IndustryActivity, { label: string; abbr: string; color: string }> = {
  manufacturing: { label: "Manufacturing", abbr: "MFG", color: "var(--color-cat-5)" },
  reaction: { label: "Reaction", abbr: "RXN", color: "var(--color-cat-3)" },
  materialEfficiencyResearch: { label: "ME research", abbr: "ME", color: "var(--color-cat-1)" },
  timeEfficiencyResearch: { label: "TE research", abbr: "TE", color: "var(--color-cat-6)" },
  copying: { label: "Copying", abbr: "CPY", color: "var(--color-cat-4)" },
  invention: { label: "Invention", abbr: "INV", color: "var(--color-cat-7)" },
  reverseEngineering: { label: "Reverse engineering", abbr: "REV", color: "var(--color-cat-2)" },
  other: { label: "Other", abbr: "OTH", color: "var(--color-muted)" },
};

export function activityLabel(activity: IndustryActivity): string {
  return ACTIVITY_META[activity].label;
}

export function activityAbbr(activity: IndustryActivity): string {
  return ACTIVITY_META[activity].abbr;
}

export function activityColor(activity: IndustryActivity): string {
  return ACTIVITY_META[activity].color;
}

const STATUS_META: Record<JobStatus, { label: string; tone: Tone }> = {
  active: { label: "Active", tone: "primary" },
  ready: { label: "Ready", tone: "positive" },
  paused: { label: "Paused", tone: "warning" },
  delivered: { label: "Delivered", tone: "positive" },
  cancelled: { label: "Cancelled", tone: "danger" },
  reverted: { label: "Reverted", tone: "danger" },
};

export function statusLabel(status: JobStatus): string {
  return STATUS_META[status].label;
}

export function statusTone(status: JobStatus): Tone {
  return STATUS_META[status].tone;
}

/** What a research job does to its blueprint: ESI reports levels gained as
 * runs, not the blueprint's current ME/TE. */
export function researchNote(job: Pick<JobView, "activity" | "runs">): string | null {
  if (job.activity === "materialEfficiencyResearch") return `+${job.runs} ME level${job.runs === 1 ? "" : "s"}`;
  if (job.activity === "timeEfficiencyResearch") return `+${job.runs} TE level${job.runs === 1 ? "" : "s"}`;
  return null;
}

/** "Ishtar Blueprint" with "→ Ishtar" underneath, when the job yields a different item. */
export function jobTitle(job: Pick<JobView, "blueprintName" | "productName" | "blueprintTypeId">): {
  blueprint: string;
  product: string | null;
} {
  const blueprint = job.blueprintName ?? `Type ${job.blueprintTypeId}`;
  const product = job.productName && job.productName !== job.blueprintName ? job.productName : null;
  return { blueprint, product };
}

/** "Kira Vantor" → "KV". */
export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]!.toUpperCase())
    .join("");
}

export function characterById(characters: JobCharacter[]): Map<string, JobCharacter> {
  return new Map(characters.map((character) => [character.connectionId, character]));
}

const DATE_TIME = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
  timeZone: "UTC",
});
const SHORT_DATE = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", timeZone: "UTC" });

/** EVE time (UTC), e.g. "Oct 17, 08:00". */
export function eveDateTime(iso: string | null): string {
  return iso ? `${DATE_TIME.format(new Date(iso))}` : "—";
}

export function shortDate(iso: string | null): string {
  return iso ? SHORT_DATE.format(new Date(iso)) : "—";
}

/** "12 min ago", "3 h ago", "2 d ago". */
export function ago(iso: string | null, nowMs: number): string {
  if (!iso) return "never";
  const minutes = Math.max(0, Math.floor((nowMs - Date.parse(iso)) / 60_000));
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours} h ago`;
  return `${Math.floor(hours / 24)} d ago`;
}

export function utcDay(nowMs: number): string {
  return new Date(nowMs).toISOString().slice(0, 10);
}

export function daysBefore(day: string, days: number): string {
  return new Date(Date.parse(`${day}T00:00:00Z`) - days * 86_400_000).toISOString().slice(0, 10);
}

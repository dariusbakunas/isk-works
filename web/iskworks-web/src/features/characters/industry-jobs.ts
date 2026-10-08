import type { IndustryActivity, IndustryJob } from "../../api/characters";
import type { Tone } from "../../components/primitives";

// Row badge vocabulary: three activity families,
// each with its own colour (amber / purple / blue). Research subtypes stay
// in the RESEARCH family and colour -- the subtype (ME / TE / Copy /
// Invent) is surfaced as a separate dim tag next to the job name, not a
// fourth badge colour, keeping the visual family consistent.
export interface ActivityBadge {
  label: "MFG" | "RXN" | "RESEARCH";
  tone: Tone;
}

export function industryActivityBadge(activity: IndustryActivity): ActivityBadge {
  switch (activity) {
    case "manufacturing":
      return { label: "MFG", tone: "warning" };
    case "reaction":
      return { label: "RXN", tone: "reaction" };
    default:
      return { label: "RESEARCH", tone: "primary" };
  }
}

// A concise research subtype tag, or null for manufacturing / reactions
// (and the removed-activity `other` fallback), which have no subtype.
export function industryActivitySubtype(activity: IndustryActivity): string | null {
  switch (activity) {
    case "materialEfficiencyResearch":
      return "ME";
    case "timeEfficiencyResearch":
      return "TE";
    case "copying":
      return "Copy";
    case "invention":
      return "Invent";
    case "reverseEngineering":
      return "Reverse Eng";
    default:
      return null;
  }
}

// The most useful name per activity: the produced item for manufacturing /
// reactions, the blueprint (with a "(copy)" hint) for copying, the
// blueprint otherwise. `runs` is appended as `×N` when above 1, and a
// research subtype (`ME` / `TE` / `Invent` / `Reverse Eng`) as a trailing
// tag -- the subtype is folded into the job name rather than shown as a
// fourth badge.
export function industryJobName(job: IndustryJob): string {
  const base = jobBaseName(job);
  const withRuns = job.runs > 1 ? `${base} ×${job.runs.toLocaleString("en-US")}` : base;
  // Copying already reads as "... (copy)"; don't also append "Copy".
  if (job.activity === "copying") return withRuns;
  const subtype = industryActivitySubtype(job.activity);
  return subtype ? `${withRuns} ${subtype}` : withRuns;
}

function jobBaseName(job: IndustryJob): string {
  switch (job.activity) {
    case "manufacturing":
    case "reaction":
      return job.productName ?? job.blueprintName ?? "Unknown job";
    case "copying":
      return `${job.blueprintName ?? "Unknown blueprint"} (copy)`;
    default:
      return job.blueprintName ?? job.productName ?? "Unknown job";
  }
}

// `system — structure`, degrading to whichever half resolved, and finally
// to a neutral "Unknown structure" (never the raw id -- an inaccessible
// player structure legitimately has no name we can show). EVE structures
// are routinely named "<system> - <owner> <thing>", so a redundant leading
// system token is stripped: "Q-3HS5 — Q-3HS5 Trading Hub" -> "Q-3HS5 —
// Trading Hub", and a structure named exactly after its system collapses
// to just the system.
export function industryJobLocation(job: IndustryJob): string {
  const system = job.solarSystemName?.trim() ?? "";
  const facility = job.facilityName?.trim() ?? "";
  if (system && facility) {
    const rest = facilityWithoutLeadingSystem(facility, system);
    if (rest === null) return `${system} — ${facility}`;
    return rest === "" ? system : `${system} — ${rest}`;
  }
  return facility || system || "Unknown structure";
}

// `""`  -> the facility name *is* the system; `null` -> no redundant
// prefix, keep the full facility name; otherwise the facility name with its
// leading system token and separators removed.
function facilityWithoutLeadingSystem(facility: string, system: string): string | null {
  const f = facility.toLowerCase();
  const s = system.toLowerCase();
  if (f === s) return "";
  if (!f.startsWith(s)) return null;
  // Require a real word boundary so "Perim" can't strip from "Perimeter".
  if (!/[\s\-–—:_·|/]/.test(facility.charAt(system.length))) return null;
  return facility
    .slice(system.length)
    .replace(/^[\s\-–—:_·|/]+/, "")
    .trim();
}

export type JobProgressState = "running" | "ready" | "paused";

export interface JobProgress {
  /** 0-100, rounded, for the percentage label. */
  percent: number;
  /** 0-1, clamped, for the progress bar width. */
  fraction: number;
  remainingMs: number;
  /** Right-aligned label: a duration, or "Ready" / "Paused". */
  remainingLabel: string;
  state: JobProgressState;
}

// Progress purely from the job's fixed timestamps and a caller-supplied
// `nowMs`, so the page can tick locally without refetching. A job whose
// timer has elapsed (or whose ESI status is already `ready`) reads as 100 %
// / "Ready" rather than a negative countdown. Zero / invalid / unknown
// durations don't divide -- they collapse to 0 % (or 100 % if we at least
// know the job is done).
export function industryJobProgress(job: IndustryJob, nowMs: number): JobProgress {
  const start = job.startDate ? Date.parse(job.startDate) : Number.NaN;
  const end = job.endDate ? Date.parse(job.endDate) : Number.NaN;
  const paused = job.status === "paused";
  const timerDone = Number.isFinite(end) && nowMs >= end;
  const ready = job.status === "ready" || (!paused && timerDone);

  let fraction: number;
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) {
    fraction = ready ? 1 : 0;
  } else {
    fraction = (nowMs - start) / (end - start);
  }
  fraction = Math.min(1, Math.max(0, ready ? 1 : fraction));

  const remainingMs = Number.isFinite(end) ? Math.max(0, end - nowMs) : 0;
  const state: JobProgressState = paused ? "paused" : ready ? "ready" : "running";
  const remainingLabel = paused ? "Paused" : ready ? "Ready" : formatJobRemaining(remainingMs);

  return { percent: Math.round(fraction * 100), fraction, remainingMs, remainingLabel, state };
}

// Compact remaining-time label: `48m`, `4h 12m`,
// `1d 2h`, `3d 1h`. Sub-minute remainders read as `<1m` rather than `0m`.
export function formatJobRemaining(ms: number): string {
  const totalMinutes = Math.floor(ms / 60_000);
  if (totalMinutes <= 0) return "<1m";
  const days = Math.floor(totalMinutes / 1_440);
  const hours = Math.floor((totalMinutes % 1_440) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

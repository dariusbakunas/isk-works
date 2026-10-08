import type { RecordingState, TicketKind } from "../../../api/industry";
import type { Tone } from "../../../components/primitives";

// Presentation of the backend's *derived* recording state. Deliberately
// NOT the workflow-status vocabulary (Blocked/Ready/In Progress/Complete):
// recording state answers "what does ISK Works know actually happened",
// workflow status answers "how am I organizing this work", and the two are
// independent -- a Complete ticket can be Not recorded, a Ready ticket can
// be Recorded. The glyph gives a non-colour cue for the same three states.
export const recordingStateMeta: Record<
  RecordingState,
  { label: string; tone: Tone; glyph: string }
> = {
  notRecorded: { label: "Not recorded", tone: "muted", glyph: "○" },
  partiallyRecorded: { label: "Partially recorded", tone: "warning", glyph: "◐" },
  recorded: { label: "Recorded", tone: "positive", glyph: "●" },
};

export interface RecordingTerms {
  requested: string;
  recorded: string;
  remaining: string;
  surplus: string;
  action: string;
}

// Acquisition recordings are measured in item quantities; production /
// reaction recordings in runs. The frozen execution snapshot's `runs` is
// the canonical unit for the latter, so the summary rows read "Planned
// runs" / "Recorded runs" rather than a bare count.
export function recordingTerms(kind: TicketKind): RecordingTerms {
  if (kind === "acquisition") {
    return {
      requested: "Requested",
      recorded: "Recorded",
      remaining: "Remaining",
      surplus: "Surplus",
      action: "Record acquisition",
    };
  }
  return {
    requested: "Planned runs",
    recorded: "Recorded runs",
    remaining: "Remaining runs",
    surplus: "Surplus runs",
    action: kind === "reaction" ? "Record reaction" : "Record production",
  };
}

// One idempotency key per submission *intent* (see the recording forms):
// generated once, reused across timeout/network retries, replaced only
// after a confirmed success. `crypto.randomUUID` is available in every
// browser this app targets and in the jsdom test env; the fallback only
// exists so a stray non-secure context can't throw.
export function newIdempotencyKey(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(16)}-${Math.random().toString(16).slice(2)}-${Math.random()
    .toString(16)
    .slice(2)}`;
}

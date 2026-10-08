import type { OrderStatus, TicketKind, TicketStatus, RequirementKind } from "../../api/industry";
import type { Tone } from "../../components/primitives";

// `OrderStatus` is the Epic-lifecycle enum -- a SEPARATE axis from a
// Ticket's workflow status. Its `blocked`/`ready` mean "some requirement
// unmet" / "all requirements satisfied", i.e. the Epic's own derived
// dependency readiness, and are kept as-is here (this enum is out of scope
// for the ticket workflow/dependency split). The Board has no Blocked or
// Ready *lane*, so `board-page.tsx` folds a blocked/ready Epic into the To
// Do lane for placement -- the badge below still names the requirement
// readiness.
export const orderStatusMeta: Record<OrderStatus, { label: string; tone: Tone }> = {
  blocked: { label: "Blocked", tone: "danger" },
  ready: { label: "Ready", tone: "positive" },
  inProgress: { label: "In Progress", tone: "primary" },
  complete: { label: "Complete", tone: "muted" },
  canceled: { label: "Canceled", tone: "muted" },
};

// Ticket workflow status -- purely organizational, user-controlled. No
// `blocked`/`ready`: dependency state is a separate derived concept
// (`TicketSummary.blockedBy`), never a lane.
export const orderTicketStatusMeta: Record<TicketStatus, { label: string; tone: Tone }> = {
  todo: { label: "To Do", tone: "muted" },
  inProgress: { label: "In Progress", tone: "primary" },
  complete: { label: "Complete", tone: "muted" },
  canceled: { label: "Canceled", tone: "muted" },
};

// Same BUY/BUILD/RXN badge vocabulary as requirementKindMeta, keyed by
// TicketKind's own string values ("acquisition"/"manufacturing"/
// "reaction") rather than RequirementKind's ("buy"/"build"/"react").
export const orderTicketKindMeta: Record<TicketKind, { label: string; tone: Tone }> = {
  acquisition: { label: "BUY", tone: "primary" },
  manufacturing: { label: "BUILD", tone: "warning" },
  reaction: { label: "RXN", tone: "reaction" },
  generic: { label: "GENERIC", tone: "muted" },
};

export const requirementKindMeta: Record<RequirementKind, { label: string; tone: Tone }> = {
  buy: { label: "BUY", tone: "primary" },
  build: { label: "BUILD", tone: "warning" },
  react: { label: "RXN", tone: "reaction" },
};


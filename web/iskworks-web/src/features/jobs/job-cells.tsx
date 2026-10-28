import { Link } from "react-router";

import type { IndustryActivity } from "../../api/characters";
import type { JobCharacter, JobStatus, JobTicketLink, JobView } from "../../api/jobs";
import { formatIskCompact, formatIskSummary } from "../../components/money";
import { Badge } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { activityAbbr, activityColor, activityLabel, initials, jobTitle, researchNote, statusLabel, statusTone } from "./jobs-format";
import { TypeInfoButton } from "../../components/type-info/type-info-button";

export function StatusChip({ status }: { status: JobStatus }) {
  return <Badge square tone={statusTone(status)}>{statusLabel(status)}</Badge>;
}

export function ActivityChip({ activity, compact = false }: { activity: IndustryActivity; compact?: boolean }) {
  const color = activityColor(activity);
  return (
    <span className="inline-flex items-center gap-1.5 text-xs" title={activityLabel(activity)}>
      <span
        className="rounded-[2px] border px-1 font-mono text-[0.625rem] font-semibold"
        style={{ borderColor: color, color }}
      >
        {activityAbbr(activity)}
      </span>
      {compact ? null : <span className="truncate text-muted">{activityLabel(activity)}</span>}
    </span>
  );
}

export function JobTitle({ job }: { job: JobView }) {
  const { blueprint, product } = jobTitle(job);
  const note = researchNote(job);
  return (
    <span className="flex min-w-0 flex-col leading-tight">
      <span className="flex min-w-0 items-center gap-1">
        <span className="truncate text-xs font-semibold text-foreground">{blueprint}</span>
        <TypeInfoButton name={blueprint} typeId={job.blueprintTypeId} />
      </span>
      {product ? (
        <span className="flex min-w-0 items-center gap-1">
          <span className="truncate text-[0.625rem] text-muted">→ {product}</span>
          {job.productTypeId !== null ? <TypeInfoButton name={product} typeId={job.productTypeId} /> : null}
        </span>
      ) : null}
      {note ? <span className="font-mono text-[0.625rem] text-muted">{note}</span> : null}
    </span>
  );
}

export function TicketChip({ ticket }: { ticket: JobTicketLink | null }) {
  if (!ticket) {
    return (
      <span className="text-xs text-muted" title="Not linked to a Board ticket, so its inputs aren't counted in Consumption">
        Untracked
      </span>
    );
  }
  return (
    <Link
      className="rounded-[2px] border border-primary/50 bg-primary/10 px-1.5 font-mono text-[0.625rem] text-primary hover:bg-primary/20"
      onClick={(event) => event.stopPropagation()}
      title="Open the ticket on the Board"
      to={`/board?ticket=${ticket.ticketId}`}
    >
      {ticket.displayId}
    </Link>
  );
}

export function CostCell({ cost }: { cost: string | null }) {
  if (cost === null) return <span className="text-muted">—</span>;
  return <span title={formatIskSummary(cost)}>{formatIskCompact(cost)}</span>;
}

export function CharacterChip({ character }: { character: JobCharacter | undefined }) {
  if (!character) return <span className="text-xs text-muted">Unknown character</span>;
  return (
    <span className="flex min-w-0 items-center gap-1.5 text-xs">
      <span aria-hidden="true" className="rounded-[2px] bg-panel-strong px-1 font-mono text-[0.5625rem] text-muted">
        {initials(character.characterName)}
      </span>
      <CharacterName className="truncate" name={character.characterName} />
    </span>
  );
}

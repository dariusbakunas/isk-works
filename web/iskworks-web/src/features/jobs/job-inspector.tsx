import { ExternalLink } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import type { JobCharacter, JobView } from "../../api/jobs";
import { formatIskAbbreviated, formatIskSummary } from "../../components/money";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { ProgressBar, SectionHead } from "../../components/primitives";
import { industryJobLocation, industryJobProgress } from "../characters/industry-jobs";
import { ActivityChip, CharacterChip, StatusChip } from "./job-cells";
import { activityLabel, eveDateTime, jobTitle, researchNote } from "./jobs-format";

function Detail({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <div className="text-[0.625rem] uppercase tracking-wider text-muted">{label}</div>
      <div className="truncate text-xs">{children}</div>
    </div>
  );
}

export function JobInspector({
  job,
  character,
  nowMs,
  onClose,
}: {
  job: JobView;
  character: JobCharacter | undefined;
  nowMs: number;
  onClose: () => void;
}) {
  const { blueprint, product } = jobTitle(job);
  const progress = industryJobProgress(job, nowMs);
  const finished = job.completedDate !== null;
  const note = researchNote(job);
  return (
    <PlannerInspectorShell
      closeLabel="Close job inspector"
      dismissLabel="Dismiss job inspector"
      eyebrow={activityLabel(job.activity)}
      onClose={onClose}
      open
      returnFocusRowKey={String(job.jobId)}
      title={blueprint}
      width="wide"
    >
      <div className="space-y-4 px-3 pb-4">
        <div className="flex flex-wrap items-center gap-2">
          <ActivityChip activity={job.activity} compact />
          <StatusChip status={job.status} />
          {product ? <span className="text-xs text-muted">→ {product}</span> : null}
        </div>

        <section>
          <SectionHead>Timeline</SectionHead>
          <ol className="space-y-2 text-xs">
            <li>
              <div className="font-semibold">Installed</div>
              <div className="font-mono text-[0.625rem] text-muted">{eveDateTime(job.startDate)} EVE</div>
            </li>
            {finished ? (
              <li>
                <div className="font-semibold">Completed</div>
                <div className="font-mono text-[0.625rem] text-muted">{eveDateTime(job.completedDate)} EVE</div>
              </li>
            ) : (
              <li>
                <div className="font-semibold text-primary">
                  {progress.state === "running" ? `Running · ${progress.percent}%` : progress.remainingLabel}
                </div>
                <div className="mt-1"><ProgressBar percent={progress.percent} tone={progress.state === "paused" ? "warning" : progress.state === "ready" ? "positive" : "primary"} /></div>
                <div className="mt-1 font-mono text-[0.625rem] text-muted">
                  Ends {eveDateTime(job.endDate)} EVE{progress.state === "running" ? ` · ${progress.remainingLabel} left` : ""}
                </div>
              </li>
            )}
          </ol>
        </section>

        <section>
          <SectionHead>Details</SectionHead>
          <div className="grid grid-cols-2 gap-3">
            <Detail label="Character"><CharacterChip character={character} /></Detail>
            <Detail label="Runs">
              {job.successfulRuns !== null && job.activity === "invention"
                ? `${job.successfulRuns} of ${job.runs} succeeded`
                : job.runs.toLocaleString("en-US")}
            </Detail>
            {note ? <Detail label="Research">{note}</Detail> : null}
            <Detail label="Facility">{industryJobLocation(job)}</Detail>
          </div>
        </section>

        <section>
          <SectionHead>Cost</SectionHead>
          <div className="flex items-baseline justify-between text-xs">
            <span className="text-muted">Install cost</span>
            {job.cost === null ? (
              <span className="text-muted">—</span>
            ) : (
              <span className="text-right">
                <span className="block font-mono text-sm font-bold text-warning">{formatIskAbbreviated(job.cost, { currency: true })}</span>
                <span className="block font-mono text-[0.625rem] text-muted">{formatIskSummary(job.cost)}</span>
              </span>
            )}
          </div>
        </section>

        <section>
          <SectionHead>Board ticket</SectionHead>
          {job.ticket ? (
            <Link
              className="flex items-center justify-between rounded-[2px] border border-primary/50 bg-primary/10 px-3 py-2 text-primary hover:bg-primary/20"
              to={`/board?ticket=${job.ticket.ticketId}`}
            >
              <span>
                <span className="block font-mono text-sm">{job.ticket.displayId}</span>
                <span className="text-[0.625rem] text-muted">Linked to this job</span>
              </span>
              <span className="flex items-center gap-1 text-xs"><ExternalLink aria-hidden="true" className="h-3 w-3" />Open</span>
            </Link>
          ) : (
            <p className="text-xs text-muted">
              Untracked: no Board ticket is linked to this job, so its inputs aren't counted in Inventory › Consumption.
              Link it from the ticket on the Board.
            </p>
          )}
        </section>
      </div>
    </PlannerInspectorShell>
  );
}

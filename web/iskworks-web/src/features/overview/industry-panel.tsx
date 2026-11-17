import type { JobView, SlotUsage } from "../../api/jobs";
import { Badge } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { industryJobName, industryJobProgress } from "../characters/industry-jobs";
import { ActivityChip, TicketChip } from "../jobs/job-cells";
import { ago, characterById } from "../jobs/jobs-format";
import { useNow } from "../jobs/use-jobs";
import { useOverviewData } from "./overview-data";
import { panelState, PanelFrame } from "./panel-frame";

const JOB_LIMIT = 5;
/** Above this many slots a meter draws a bar instead of one cell per slot. */
const MAX_CELLS = 50;

export function Stat({ label, children, accent = false }: { label: string; children: React.ReactNode; accent?: boolean }) {
  return (
    <div className="min-w-[6rem] flex-1 border-r border-border px-3 py-2 last:border-r-0">
      <div className="text-[0.625rem] font-semibold uppercase tracking-wider text-muted">{label}</div>
      <div className={`font-mono text-lg font-bold tabular-nums ${accent ? "text-primary" : "text-foreground"}`}>{children}</div>
    </div>
  );
}

function SlotMeter({ label, usage }: { label: string; usage: SlotUsage }) {
  if (usage.max === null) {
    return (
      <div className="grid min-h-8 grid-cols-[7rem_1fr] items-center border-b border-border/60 px-3 text-xs">
        <span>{label}</span>
        <span className="text-muted">— (skills not synced)</span>
      </div>
    );
  }
  const free = Math.max(0, usage.max - usage.used);
  return (
    <div className="grid min-h-8 grid-cols-[7rem_3.5rem_minmax(4rem,1fr)_3.5rem] items-center gap-2 border-b border-border/60 px-3 text-xs">
      <span>{label}</span>
      <span className="font-mono text-muted">{usage.used} / {usage.max}</span>
      <span
        aria-label={`${label} slots`}
        aria-valuemax={usage.max}
        aria-valuemin={0}
        aria-valuenow={usage.used}
        className="flex h-2 gap-0.5"
        role="meter"
      >
        {usage.max <= MAX_CELLS ? (
          Array.from({ length: usage.max }, (_, index) => (
            <i
              key={index}
              className={`min-w-0.5 flex-1 rounded-[1px] border ${index < usage.used ? "border-primary/70 bg-primary/70" : "border-muted/50"}`}
            />
          ))
        ) : (
          <i className="flex-1 overflow-hidden rounded-[1px] border border-muted/50">
            <i className="block h-full bg-primary/70" style={{ width: `${Math.min(100, (usage.used / usage.max) * 100)}%` }} />
          </i>
        )}
      </span>
      <span className={free > 0 ? "text-primary" : "text-muted"}>{free > 0 ? `${free} free` : "full"}</span>
    </div>
  );
}

function JobRow({ job, characterName, nowMs, mobileHidden }: { job: JobView; characterName: string | null; nowMs: number; mobileHidden: boolean }) {
  const progress = industryJobProgress(job, nowMs);
  return (
    <li className={`${mobileHidden ? "hidden sm:grid" : "grid"} min-h-9 grid-cols-[2.75rem_minmax(0,1fr)_auto] items-center gap-2 border-t border-border/60 px-3 py-1 sm:grid-cols-[2.75rem_minmax(0,1.6fr)_minmax(0,1fr)_minmax(0,0.8fr)_4.5rem]`}>
      <ActivityChip activity={job.activity} compact />
      <span className="flex min-w-0 items-center gap-1.5">
        <span className="truncate text-xs text-foreground">{industryJobName(job)}</span>
        {job.ticket ? <TicketChip ticket={job.ticket} /> : null}
      </span>
      <span className="hidden truncate text-[0.6875rem] text-muted sm:block">
        {characterName ? <CharacterName name={characterName} /> : null}
      </span>
      <span className="hidden truncate text-[0.6875rem] text-muted sm:block">{job.solarSystemName ?? ""}</span>
      <span className="justify-self-end">
        {progress.state === "ready" ? (
          <Badge square tone="positive">Ready</Badge>
        ) : (
          <span className={`font-mono text-[0.6875rem] ${progress.state === "paused" ? "text-warning" : ""}`}>{progress.remainingLabel}</span>
        )}
      </span>
    </li>
  );
}

/** Running work and free capacity: counts, a slot meter per activity, and
 * the jobs finishing soonest (spec §3.3, §9.2). */
export function IndustryPanel({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const nowMs = useNow();
  const jobs = sources.jobs;
  const data = jobs.data;
  const characters = data ? characterById(data.characters) : new Map();

  return (
    <PanelFrame
      className={className}
      link={{ to: "/jobs" }}
      stamp={data?.syncedAt ? `synced ${ago(data.syncedAt, nowMs)}` : undefined}
      state={panelState(jobs, () => false, null)}
      title="Industry"
    >
      {data ? (
        <>
          <div aria-label="Job totals" className="flex flex-wrap border-b border-border" role="group">
            <Stat label="Running">{data.summary.running}</Stat>
            <Stat accent label="Ready">{data.summary.ready}</Stat>
            <Stat label="Ending < 24h">{data.summary.endingWithinDay}</Stat>
          </div>
          <div className="border-b border-border">
            <SlotMeter label="Manufacturing" usage={data.slots.manufacturing} />
            <SlotMeter label="Reactions" usage={data.slots.reaction} />
            <SlotMeter label="Research" usage={data.slots.research} />
          </div>
          <div className="flex h-6 items-center bg-panel-strong/60 px-3 text-[0.625rem] font-semibold uppercase tracking-wider text-muted">
            Soonest jobs
          </div>
          {data.jobs.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-muted">No jobs running.</p>
          ) : (
            <ul>
              {data.jobs.slice(0, JOB_LIMIT).map((job, index) => (
                <JobRow
                  mobileHidden={index >= 3}
                  key={`${job.connectionId}:${job.jobId}`}
                  characterName={characters.get(job.connectionId)?.characterName ?? null}
                  job={job}
                  nowMs={nowMs}
                />
              ))}
            </ul>
          )}
        </>
      ) : null}
    </PanelFrame>
  );
}

import type { CharacterDetail, IndustryJob } from "../../api/characters";
import type { Tone } from "../../components/primitives";
import { textToneClasses } from "../../components/primitives";
import { industryCategoryStats, missingScopeFrom, type IndustryCategoryStat } from "./characters-formatters";
import { industryActivityBadge, industryJobLocation, industryJobName, industryJobProgress } from "./industry-jobs";

// The inspector's Industry tab: three skill-derived slot cards over a
// soonest-finishing-first list of the character's running jobs. Job
// progress and remaining time are recomputed from `nowMs` on every render
// so the parent can tick locally without refetching.
export function CharacterIndustryTab({ detail, nowMs }: { detail: CharacterDetail; nowMs: number }) {
  const stats = industryCategoryStats(detail);
  const industrySource = detail.sources.find((source) => source.sourceKind === "industryJobs");
  const missingScope = missingScopeFrom(industrySource?.lastError ?? null);
  const syncFailed = industrySource?.refreshState === "failed" && missingScope === null;
  const neverSynced =
    industrySource === undefined ||
    (industrySource.refreshState === "missing" && industrySource.observedAt === null);
  const jobs = detail.industryJobs;

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-3 gap-2">
        {stats.map((stat) => (
          <SlotCard key={stat.key} stat={stat} />
        ))}
      </div>

      <div>
        <div className="mb-1.5 text-[9px] font-semibold uppercase tracking-wider text-muted">Active jobs</div>
        {missingScope !== null ? (
          <div className="border border-warning/40 bg-warning/10 p-2 text-[10px] text-warning">
            <p className="font-semibold">Industry jobs need re-authorization</p>
            <p className="mt-0.5 text-warning/80">
              This character hasn’t granted <span className="font-mono">{missingScope}</span>. Use “Reconnect” on the Sync
              tab to add it.
            </p>
          </div>
        ) : syncFailed ? (
          <p className="border border-danger/40 bg-danger/10 p-2 text-[10px] text-danger">
            Industry jobs failed to sync
            {industrySource?.lastError ? `: ${industrySource.lastError}` : "."}
          </p>
        ) : neverSynced ? (
          <p className="text-[10px] text-muted">Not synced yet.</p>
        ) : jobs.length === 0 ? (
          <p className="text-[10px] text-muted">No active industry jobs.</p>
        ) : (
          <ul className="divide-y divide-border/70 border-y border-border/70">
            {jobs.map((job) => (
              <IndustryJobRow job={job} key={job.jobId} nowMs={nowMs} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function SlotCard({ stat }: { stat: IndustryCategoryStat }) {
  return (
    <div className="border border-border bg-background px-3 py-2.5 text-center">
      <div className="font-mono text-[17px] font-bold leading-none text-foreground">
        {stat.active ?? "—"}
        <span className="ml-0.5 text-[12px] font-normal text-muted">/{stat.max ?? "—"}</span>
      </div>
      <div className={`mt-1.5 text-[10px] font-medium ${textToneClasses[stat.tone]}`}>{stat.label}</div>
    </div>
  );
}

const miniBarFillClass: Record<Tone, string> = {
  primary: "bg-primary",
  positive: "bg-positive",
  danger: "bg-danger",
  warning: "bg-warning",
  muted: "bg-muted",
  batch: "bg-batch",
  reaction: "bg-reaction",
};

function IndustryJobRow({ job, nowMs }: { job: IndustryJob; nowMs: number }) {
  const badge = industryActivityBadge(job.activity);
  const name = industryJobName(job);
  const location = industryJobLocation(job);
  const progress = industryJobProgress(job, nowMs);
  const ready = progress.state === "ready";
  const barTone: Tone = ready ? "positive" : badge.tone;

  return (
    <li className="py-1.5">
      <div className="flex items-baseline gap-2">
        <span className={`shrink-0 text-[9px] font-semibold uppercase tracking-wide ${textToneClasses[badge.tone]}`}>
          {badge.label}
        </span>
        <span className="min-w-0 flex-1 truncate text-[11px] text-foreground" title={name}>
          {name}
        </span>
        <span className={`shrink-0 text-[10px] tabular-nums ${ready ? "text-positive" : "text-muted"}`}>
          {progress.remainingLabel}
        </span>
      </div>
      <div className="mt-1 flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-[10px] text-muted" title={location}>
          {location}
        </span>
        <div className="h-[2px] w-28 shrink-0 overflow-hidden bg-border">
          <div className={`h-full ${miniBarFillClass[barTone]}`} style={{ width: `${progress.percent}%` }} />
        </div>
        <span className="w-9 shrink-0 text-right text-[10px] tabular-nums text-muted">{progress.percent}%</span>
      </div>
    </li>
  );
}

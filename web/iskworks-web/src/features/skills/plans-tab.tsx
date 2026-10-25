import { BookOpen, ClipboardPaste, Plus } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useNavigate } from "react-router";

import { listSkillPlans, type SkillPlanDetail, type SkillPlanSummary } from "../../api/skills";
import { InlineAlert, ProgressBar } from "../../components/primitives";
import { ImportPlanDialog, NewPlanDialog } from "./plan-dialogs";
import { planProgressLabel } from "./skill-plans";

function usePlans(characterId: string | null) {
  const [plans, setPlans] = useState<SkillPlanSummary[] | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setError(null);
    listSkillPlans(characterId, controller.signal)
      .then((result) => {
        if (!controller.signal.aborted) setPlans(result);
      })
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) setError(reason instanceof Error ? reason : new Error("Skill plans request failed."));
      });
    return () => controller.abort();
  }, [characterId, attempt]);
  return { plans, error, retry: useCallback(() => setAttempt((current) => current + 1), []) };
}

function PlanCard({ plan, search }: { plan: SkillPlanSummary; search: string }) {
  const progress = plan.progress;
  const complete = progress !== null && progress.percentComplete >= 100;
  return (
    <Link
      className="iw-panel flex flex-col gap-2 p-3 hover:border-primary/60"
      to={{ pathname: `/skills/plans/${plan.id}`, search }}
    >
      <span className="text-sm font-semibold">{plan.name}</span>
      {plan.description ? <span className="line-clamp-2 text-xs text-muted">{plan.description}</span> : null}
      {progress ? (
        <>
          <span className="flex items-baseline justify-between gap-2 text-xs">
            <span className="text-muted">{Math.floor(progress.percentComplete)}% complete</span>
            <span className={`font-mono ${complete ? "text-positive" : ""}`}>
              {complete ? "Completed ✓" : planProgressLabel(progress)}
            </span>
          </span>
          <ProgressBar percent={progress.percentComplete} tone={complete ? "positive" : "primary"} />
          {progress.missingSkillbooks > 0 ? (
            <span className="text-xs text-warning">
              {progress.missingSkillbooks} missing skillbook{progress.missingSkillbooks === 1 ? "" : "s"}
            </span>
          ) : null}
          {progress.blocked ? <span className="text-xs text-[#a58ad8]">Blocked: Omega-only skills</span> : null}
        </>
      ) : null}
      <span className="mt-auto border-t border-border pt-1.5 text-[0.625rem] text-muted">
        {plan.entryCount} skill{plan.entryCount === 1 ? "" : "s"}
      </span>
    </Link>
  );
}

export function PlansTab({ characterId }: { characterId: string | null }) {
  const { plans, error, retry } = usePlans(characterId);
  const [dialog, setDialog] = useState<"new" | "import" | null>(null);
  const navigate = useNavigate();
  const search = characterId ? `?${new URLSearchParams({ character: characterId })}` : "";

  const opened = (detail: SkillPlanDetail) => {
    setDialog(null);
    navigate({ pathname: `/skills/plans/${detail.plan.id}`, search });
  };
  const actions = (
    <>
      <button className="iw-button-secondary inline-flex items-center gap-1" onClick={() => setDialog("import")} type="button">
        <ClipboardPaste aria-hidden="true" className="h-3.5 w-3.5" />
        Import from clipboard
      </button>
      <button className="iw-button-primary inline-flex items-center gap-1" onClick={() => setDialog("new")} type="button">
        <Plus aria-hidden="true" className="h-3.5 w-3.5" />
        New plan
      </button>
    </>
  );

  return (
    <div className="space-y-3">
      {error && plans === null ? (
        <InlineAlert title="Skill plans unavailable">
          {error.message} <button className="underline" onClick={retry} type="button">Retry</button>
        </InlineAlert>
      ) : plans === null ? (
        <div aria-busy="true" aria-label="Loading skill plans" className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3" role="status">
          {Array.from({ length: 3 }, (_, index) => <div className="h-32 animate-pulse rounded-[2px] bg-panel-strong" key={index} />)}
        </div>
      ) : plans.length === 0 ? (
        <div className="iw-panel flex flex-col items-center gap-2 px-3 py-10 text-center">
          <BookOpen aria-hidden="true" className="h-8 w-8 text-muted" />
          <h3 className="text-sm font-semibold">No skill plans yet</h3>
          <p className="text-xs text-muted">Create your first plan or paste one from the game.</p>
          <div className="flex gap-2">{actions}</div>
        </div>
      ) : (
        <>
          <div className="flex justify-end gap-2">{actions}</div>
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {plans.map((plan) => <PlanCard key={plan.id} plan={plan} search={search} />)}
          </div>
        </>
      )}
      {dialog === "new" ? <NewPlanDialog onClose={() => setDialog(null)} onCreated={opened} /> : null}
      {dialog === "import" ? <ImportPlanDialog onClose={() => setDialog(null)} onCreated={opened} /> : null}
    </div>
  );
}

import { ArrowLeft, Globe2, Pencil, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import { apiMessage } from "../../../../api/api-error";
import {
  deleteSavedPlan,
  getSavedPlan,
  SAVED_PLAN_SNAPSHOT_VERSION,
  updateSavedPlan,
  type SavedPlan,
} from "../../../../api/pi-plans";
import { ConfirmDialog, InlineAlert } from "../../../../components/primitives";
import { useLoad } from "../../../../hooks/use-load";
import { formatDate } from "../../../../lib/formatting";
import { PlanetarySectionTabs } from "../planetary-section-tabs";
import { PlanPanel } from "../planner/plan-panel";
import { PlanNameDialog } from "./plan-name-dialog";

/** Planetary › Plans › one plan: the snapshot as saved, and its set-up checklist. */
export function SavedPlanPage() {
  const { id = "" } = useParams();
  const loaded = useLoad(() => getSavedPlan(id), id);

  return (
    <>
      <header className="mb-3 flex flex-wrap items-center gap-3">
        <h1 className="iw-title flex items-center gap-2">
          <Globe2 aria-hidden="true" className="h-4 w-4 text-primary" />
          Planetary Interaction
        </h1>
      </header>
      <PlanetarySectionTabs />
      {loaded.error ? (
        <InlineAlert title="The plan could not be loaded">{loaded.errorMessage}</InlineAlert>
      ) : loaded.data === null ? (
        <p className="text-sm text-muted">Loading the plan…</p>
      ) : (
        // Keyed so another plan starts from its own ticks.
        <SavedPlanView key={loaded.data.id} plan={loaded.data} />
      )}
    </>
  );
}

function SavedPlanView({ plan }: { plan: SavedPlan }) {
  const navigate = useNavigate();
  const [name, setName] = useState(plan.name);
  const [setUp, setSetUp] = useState<ReadonlySet<number>>(() => new Set(plan.setUpColonies));
  const [failure, setFailure] = useState<string | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  // Ticks save one after another, so the last one sent is the one kept.
  const saving = useRef<Promise<unknown>>(Promise.resolve());
  const colonies = plan.solution.plan.colonies.length;

  function toggle(index: number, done: boolean) {
    const previous = setUp;
    const next = new Set(previous);
    if (done) next.add(index);
    else next.delete(index);
    setSetUp(next);
    setFailure(null);
    const indexes = [...next].sort((a, b) => a - b);
    saving.current = saving.current
      .then(() => updateSavedPlan(plan.id, { setUpColonies: indexes }))
      .catch((error: unknown) => {
        setSetUp((current) => (current === next ? previous : current));
        setFailure(apiMessage(error));
      });
  }

  async function rename(newName: string) {
    const saved = await updateSavedPlan(plan.id, { name: newName });
    setName(saved.name);
    setRenaming(false);
  }

  async function remove() {
    setDeleting(false);
    try {
      await deleteSavedPlan(plan.id);
      navigate("/planetary/plans");
    } catch (error) {
      setFailure(apiMessage(error));
    }
  }

  const scenario = plan.scenario;
  return (
    <>
      <section className="mb-3 flex flex-wrap items-start gap-3">
        <div className="min-w-0 flex-1">
          <Link className="mb-1 inline-flex items-center gap-1 text-xs text-muted hover:text-foreground" to="/planetary/plans">
            <ArrowLeft aria-hidden="true" className="h-3 w-3" />
            All plans
          </Link>
          <h2 className="truncate text-base font-semibold" data-private="">
            {name}
          </h2>
          <p className="text-xs text-muted">
            Saved {formatDate(plan.createdAt)}
            {plan.solution.priceObservedAt ? ` · prices as of ${formatDate(plan.solution.priceObservedAt)}` : null}
            {" · "}
            <span className={setUp.size === colonies ? "text-success" : "text-foreground"}>
              {setUp.size} of {colonies} set up
            </span>
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button className="iw-button-secondary flex items-center gap-1.5" onClick={() => setRenaming(true)} type="button">
            <Pencil aria-hidden="true" className="h-3.5 w-3.5" />
            Rename
          </button>
          <button className="iw-button-secondary flex items-center gap-1.5" onClick={() => setDeleting(true)} type="button">
            <Trash2 aria-hidden="true" className="h-3.5 w-3.5" />
            Delete
          </button>
        </div>
      </section>
      {failure ? <InlineAlert title="Not saved">{failure}</InlineAlert> : null}
      {plan.snapshotVersion > SAVED_PLAN_SNAPSHOT_VERSION ? (
        <InlineAlert title="Can't show this plan">
          This plan was saved by a newer version of ISK Works. Reload the page to update, then open it again.
        </InlineAlert>
      ) : (
        <section aria-label="Plan" className="iw-panel min-w-0">
          <PlanPanel
            characterNames={scenario.characterNames}
            haulDays={scenario.haulDays}
            resetDays={scenario.resetDays}
            setUp={{ indexes: setUp, onToggle: toggle }}
            solution={plan.solution}
            solving={false}
          />
        </section>
      )}
      {renaming ? (
        <PlanNameDialog
          initialName={name}
          onCancel={() => setRenaming(false)}
          onSubmit={rename}
          submitLabel="Save"
          title="Rename plan"
        />
      ) : null}
      <ConfirmDialog
        confirmLabel="Delete"
        onCancel={() => setDeleting(false)}
        onConfirm={() => void remove()}
        open={deleting}
        title={`Delete ${name}?`}
      >
        Its colonies and set-up ticks are gone for good. The colonies in game stay as they are.
      </ConfirmDialog>
    </>
  );
}

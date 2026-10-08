// The Plan inspector's two actions -- changing one demand edge's sourcing
// (an always-visible Buy / Build / Reaction switch), and creating one
// direct production ticket for one operation. Both delegate to existing server paths; neither computes a
// quantity, run count or cost.

import { useState } from "react";
import { Link } from "react-router";

import type { MaterialActivity, RecipeSelection, Ticket } from "../../../../api/industry";
import { createTicket } from "../../../../api/industry";
import { apiMessage } from "../../shared/api-error";

import type { SourcingChoice } from "./use-plan-sourcing";

function choiceValue(choice: SourcingChoice): string {
  if (!choice) return "buy";
  return choice.mode === "manufacturing"
    ? `manufacturing:${choice.blueprintTypeId}`
    : `reaction:${choice.reactionFormulaTypeId}`;
}

function choiceLabel(choice: SourcingChoice): string {
  if (!choice) return "Buy";
  return choice.mode === "manufacturing" ? "Build" : "Reaction";
}

/** One demand edge's sourcing, as an always-visible
 * segmented switch -- Buy plus every valid production method for the
 * component (from the plan's own `productionMethods`). It changes exactly
 * this edge; a multi-edge action would be a separate, explicit control. */
export function SourcingSwitch({
  consumerName,
  current,
  methods,
  pending,
  error,
  disabled = false,
  onChange,
}: {
  consumerName: string;
  current: SourcingChoice;
  methods: RecipeSelection[];
  pending: boolean;
  error: string | null;
  disabled?: boolean;
  onChange: (choice: SourcingChoice) => void;
}) {
  const options: SourcingChoice[] = [null, ...methods];
  if (current && !options.some((option) => choiceValue(option) === choiceValue(current))) {
    options.push(current);
  }
  return (
    <div className="space-y-1">
      <div
        aria-label={`Sourcing for ${consumerName}`}
        className="inline-flex rounded border border-border p-0.5 text-xs"
        role="radiogroup"
      >
        {options.map((option) => {
          const selected = choiceValue(option) === choiceValue(current);
          return (
            <button
              aria-checked={selected}
              className={
                selected
                  ? "rounded bg-primary/15 px-3 py-1 font-semibold text-primary"
                  : "px-3 py-1 text-muted hover:text-foreground disabled:hover:text-muted"
              }
              disabled={pending || disabled || selected}
              key={choiceValue(option)}
              onClick={() => onChange(option)}
              role="radio"
              type="button"
            >
              {choiceLabel(option)}
            </button>
          );
        })}
      </div>
      {pending ? <p className="text-[11px] text-muted">Updating plan...</p> : null}
      {error ? <p className="text-[11px] text-danger">{error}</p> : null}
    </div>
  );
}

/** Direct Plan ticket: ONE standalone production ticket for ONE operation
 * (`POST /api/tickets`, the existing Build-backed standalone ticket), at
 * the plan's projected runs. The server snapshots the producer Build's
 * current recipe/blueprint/facility and materials at creation, exactly as
 * a ticket created from the Board does -- not an Epic snapshot, and never
 * one ticket per consumer. */
export function CreateOperationTicket({
  activity,
  buildId,
  runs,
  outputName,
}: {
  activity: MaterialActivity;
  buildId: string;
  runs: number;
  outputName: string;
}) {
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<Ticket | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleCreate() {
    setCreating(true);
    setError(null);
    try {
      setCreated(await createTicket({ kind: activity, buildId, runs }));
    } catch (caught) {
      setError(apiMessage(caught));
    } finally {
      setCreating(false);
    }
  }

  return (
    <div className="space-y-1.5">
      <p className="text-[11px] text-muted">
        One standalone {activity === "reaction" ? "reaction" : "manufacturing"} ticket for this
        operation: {runs.toLocaleString("en-US")} run{runs === 1 ? "" : "s"} of {outputName}. It
        captures the producer&rsquo;s current recipe, facility and materials when created. No Epic
        needed.
      </p>
      <button
        className="iw-button-secondary"
        disabled={creating}
        onClick={() => void handleCreate()}
        type="button"
      >
        {creating ? "Creating ticket..." : "Create ticket"}
      </button>
      {created ? (
        <p className="text-[11px] text-muted" role="status">
          Created{" "}
          <Link className="underline" to={`/board?ticket=${created.id}`}>
            {created.displayId}
          </Link>
          .
        </p>
      ) : null}
      {error ? <p className="text-[11px] text-danger">{error}</p> : null}
    </div>
  );
}

import type { ReactNode } from "react";
import type { CreateBuildPlanPreview } from "../../../../api/industry";
import { Field } from "../../shared/field";
import { Panel } from "../../../../components/primitives";
import { CandidateSummary } from "./candidate-summary";

type CandidatePreview = CreateBuildPlanPreview;

/** The server's run limit. A blueprint copy never caps runs: more runs than
 * it licenses plan as one job per copy. */
const MAX_RUNS = 1_000_000;

export function BuildEditorHeader({
  epicSelector,
  onRunsChange,
  onShowLogistics,
  preview,
  runs,
  runsDisabled = false,
}: {
  /** The Plan view's Epic selector, next to Runs (saved root Builds). */
  epicSelector?: ReactNode;
  onRunsChange: (value: string) => void;
  /** Jump from the shortage line to Logistics. */
  onShowLogistics?: () => void;
  preview: CandidatePreview | null;
  runs: string;
  /** An Epic is selected: the Plan shows its frozen runs, not the draft's. */
  runsDisabled?: boolean;
}) {
  return (
    <Panel>
      <div
        className={epicSelector
          ? "grid gap-3 sm:grid-cols-[6rem_14rem_minmax(0,1fr)] sm:gap-0"
          : "grid gap-3 sm:grid-cols-[6rem_minmax(0,1fr)] sm:gap-0"}
      >
        <div className="min-w-0 sm:pr-4">
          <Field
            label="Runs"
            value={runs}
            onChange={(value) => {
              if (
                value === ""
                || (/^\d+$/.test(value) && Number(value) <= MAX_RUNS)
              ) {
                onRunsChange(value);
              }
            }}
            inputMode="numeric"
            type="number"
            min={1}
            max={MAX_RUNS}
            step={1}
            disabled={runsDisabled}
          />
        </div>
        {epicSelector ? <div className="min-w-0 sm:pr-4">{epicSelector}</div> : null}
        <CandidateSummary
          onShowLogistics={onShowLogistics}
          preview={preview}
        />
      </div>
    </Panel>
  );
}

import type { CreateBuildPlanPreview } from "../../../../api/industry";
import { Field } from "../../shared/field";
import { Panel } from "../../../../components/primitives";
import { CandidateSummary } from "./candidate-summary";

type CandidatePreview = CreateBuildPlanPreview;

/** The server's run limit. A blueprint copy never caps runs: more runs than
 * it licenses plan as one job per copy. */
const MAX_RUNS = 1_000_000;

export function BuildEditorHeader({
  onRunsChange,
  onShowLogistics,
  preview,
  runs,
}: {
  onRunsChange: (value: string) => void;
  /** Jump from the shortage line to Logistics. */
  onShowLogistics?: () => void;
  preview: CandidatePreview | null;
  runs: string;
}) {
  return (
    <Panel>
      <div className="grid gap-3 sm:grid-cols-[6rem_minmax(0,1fr)] sm:gap-0">
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
          />
        </div>
        <CandidateSummary
          onShowLogistics={onShowLogistics}
          preview={preview}
        />
      </div>
    </Panel>
  );
}

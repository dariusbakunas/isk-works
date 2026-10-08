import type { CreateBuildPlanPreview } from "../../../../api/industry";
import { BuildPlannerResults } from "../planner/build-planner-results";
import { ProductionWorksheet } from "../planner/production-worksheet";
import { projectCreatePreview } from "../planner/projection";

export function CreateCandidateResults({
  onSelectRow,
  preview,
  selectedRowKey,
}: {
  onSelectRow: (rowKey: string | null) => void;
  preview: CreateBuildPlanPreview;
  selectedRowKey: string | null;
}) {
  return (
    <div className="mt-2">
      <WorksheetResults
        onSelectRow={onSelectRow}
        selectedRowKey={selectedRowKey}
        worksheet={preview.worksheet}
      />
      {preview.validation.blockers.length > 0 ? (
        <div className="mt-4">
          <BuildPlannerResults preview={projectCreatePreview(preview)} updating={false} />
        </div>
      ) : null}
    </div>
  );
}

/**
 * The candidate worksheet table. Row selection is controlled from the Build
 * editor's single `inspectorMode` state -- this component owns no selection
 * state of its own, and does not render the Selected Item inspector
 * (`BuildInspector` is the sole Build-page inspector owner).
 */
export function WorksheetResults({
  onSelectRow,
  selectedRowKey,
  worksheet,
}: {
  onSelectRow: (rowKey: string | null) => void;
  selectedRowKey: string | null;
  worksheet: CreateBuildPlanPreview["worksheet"];
}) {
  return (
    <ProductionWorksheet
      onSelectRow={onSelectRow}
      selectedRowKey={selectedRowKey}
      worksheet={worksheet}
    />
  );
}

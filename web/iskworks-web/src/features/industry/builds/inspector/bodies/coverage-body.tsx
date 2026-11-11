import type {
  CoverageSlice,
} from "../inspector-model";
import { InspectorMetricGrid } from "../inspector-section";

export function CoverageBody({ slice }: { slice: CoverageSlice }) {
  return (
    <>
      <InspectorMetricGrid metrics={slice.metrics} />
      <div className="mt-1.5 flex items-center gap-2">
        <span className="h-1 flex-1 overflow-hidden bg-border">
          <span
            className={`block h-full ${slice.hasShortage ? "bg-warning" : "bg-positive"}`}
            style={{ width: `${slice.percentage}%` }}
          />
        </span>
        <strong className={`font-mono text-xs ${slice.hasShortage ? "text-warning" : "text-positive"}`}>
          {slice.percentage}%
        </strong>
      </div>
    </>
  );
}

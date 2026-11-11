import type {
  QuantitiesSlice,
} from "../inspector-model";
import { InspectorMetricGrid } from "../inspector-section";
import { CoverageBar } from "./coverage-bar";

export function QuantitiesBody({ slice }: { slice: QuantitiesSlice }) {
  return (
    <>
      <InspectorMetricGrid metrics={slice.metrics} />
      {slice.percentage != null ? (
        <CoverageBar hasShortage={Boolean(slice.hasShortage)} percentage={slice.percentage} />
      ) : null}
    </>
  );
}

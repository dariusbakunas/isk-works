import type {
  CostSlice,
} from "../inspector-model";
import { InspectorRow } from "../inspector-section";

export function CostBody({ slice }: { slice: CostSlice }) {
  const unknown = slice.computing
    ? "Computing…"
    : slice.state === "incomplete"
      ? "Incomplete"
      : slice.state === "stale"
        ? "Stale"
        : slice.state === "unresolved"
          ? "Unresolved"
          : "Not computed";
  return (
    <>
      <InspectorRow label="Material / Component Cost" value={slice.material ?? unknown} />
      <InspectorRow
        label="Installation"
        value={
          slice.installation ?? (
            <span className="text-muted">{slice.computing ? "Computing…" : "Not included"}</span>
          )
        }
      />
      <InspectorRow
        label="Total Production Cost"
        value={
          slice.total ?? (
            <span className="text-muted">{slice.computing ? "Computing…" : "Incomplete"}</span>
          )
        }
      />
    </>
  );
}

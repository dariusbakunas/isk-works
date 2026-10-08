// Small presentational pieces shared by the Stages inspector panels.

import { AlertTriangle } from "lucide-react";
import type { ReactNode } from "react";

import type {
  AcquisitionLine,
  ExecutionNode,
} from "../../../../api/industry";
import { InspectorRow } from "../../inspector/inspector-section";

import type { RootRowEditing } from "./root-row-editing";



export const qty = (value: number) => value.toLocaleString("en-US");

/** An item that is bought for some consumers and produced for others --
 * mixed sourcing is a valid plan, so both sides are shown together. */
export function SourcingSplit({
  acquisition,
  produced,
  nodesById,
}: {
  acquisition: AcquisitionLine;
  produced: ExecutionNode[];
  nodesById: Map<string, ExecutionNode>;
}) {
  const producedRequired = produced.reduce((sum, node) => sum + node.requiredQuantity, 0);
  const fromProduction = produced.reduce((sum, node) => sum + node.productionDemand, 0);
  const names = (ids: string[]) =>
    [...new Set(ids.map((id) => nodesById.get(id)?.outputTypeName ?? "Unknown"))].join(", ");
  return (
    <div aria-label="Sourcing split" className="mt-2 rounded border border-border p-2" role="group">
      <p className="mb-1 text-[10px] font-semibold uppercase tracking-wide text-muted">
        Bought and produced
      </p>
      <InspectorRow label="Total required" value={qty(acquisition.requiredQuantity + producedRequired)} />
      <InspectorRow label="From production" value={qty(fromProduction)} />
      <InspectorRow label="To acquire" value={qty(acquisition.shortageQuantity)} />
      <p className="pt-1 text-[11px] text-muted">
        Produced for {names(produced.flatMap((node) => node.consumers.map((consumer) => consumer.nodeId)))};
        bought for {names(acquisition.consumers.map((consumer) => consumer.nodeId))}.
      </p>
    </div>
  );
}

export function ConfigurationWarning({ children }: { children: ReactNode }) {
  return (
    <p className="flex items-start gap-1 text-[11px] text-warning" role="note">
      <AlertTriangle aria-hidden="true" className="mt-0.5 h-3 w-3 shrink-0" />
      <span>{children}</span>
    </p>
  );
}

export function MissingFacilityWarning() {
  return (
    <ConfigurationWarning>
      Facility not selected -- installation cost is incomplete until a facility is chosen.
    </ConfigurationWarning>
  );
}

/** A demand edge's fulfillment scope. Editable only for the root Build's
 * own edges (the Worksheet's per-component scope, same editor overlay);
 * a nested edge's scope is owned by its consumer Build. */
export function ScopeRow({
  consumerName,
  rootEditing,
  scope,
  typeId,
}: {
  consumerName: string;
  rootEditing: RootRowEditing | null;
  scope: "missing" | "full";
  typeId: number;
}) {
  if (!rootEditing) return <InspectorRow label="Scope" value={scopeLabel(scope)} />;
  return (
    <label className="flex items-center justify-between gap-2 py-0.5 text-xs">
      <span className="text-muted">Scope</span>
      <select
        aria-label={`Fulfillment scope for ${consumerName}`}
        className="iw-input h-7 w-auto py-0 text-xs"
        onChange={(event) => rootEditing.onScopeChange(typeId, event.target.value as "missing" | "full")}
        value={scope}
      >
        <option value="missing">{scopeLabel("missing")}</option>
        <option value="full">{scopeLabel("full")}</option>
      </select>
    </label>
  );
}

const scopeLabel = (scope: "missing" | "full") =>
  scope === "full" ? "Full (ignores inventory)" : "Missing (uses inventory)";

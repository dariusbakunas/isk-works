// Stages inspector panel for one acquisition (Buy) line.

import { useState } from "react";

import type {
  AcquisitionLine,
  EpicStockProgress,
  ExecutionPlanProjection,
} from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { MoneyAmount } from "../../../../components/money";
import { Badge } from "../../../../components/primitives";
import { InspectorRow, InspectorSection } from "../../inspector/inspector-section";
import { PlannerInspectorShell } from "../../../../components/planner-inspector-shell";
import { PricingBody } from "../../inspector/unified-item-inspector";

import type { RootRowEditing } from "./root-row-editing";

import { SourcingSwitch } from "./plan-actions";
import { edgeKey, type PlanSourcing } from "./use-plan-sourcing";

import { ConfigurationWarning, qty, ScopeRow, SourcingSplit } from "./stages-inspector-parts";

export function AcquisitionInspector({
  line,
  plan,
  onClose,
  sourcing,
  rootBuildId,
  rootEditing,
  readOnly = false,
  epicStock = null,
}: {
  line: AcquisitionLine;
  plan: ExecutionPlanProjection;
  onClose: () => void;
  sourcing: PlanSourcing;
  rootBuildId: string;
  rootEditing: RootRowEditing | null;
  /** An Epic's frozen plan: no sourcing switches, no price overrides, no
   * links into the draft's producer Builds. */
  readOnly?: boolean;
  /** With `readOnly`: what the Epic holds and has used of this input. */
  epicStock?: EpicStockProgress | null;
}) {
  const nodesById = new Map(plan.nodes.map((node) => [node.id, node]));
  const methods = line.productionMethods ?? [];
  const [producingAll, setProducingAll] = useState(false);
  const produced = plan.nodes.filter((node) => node.outputTypeId === line.typeId);
  // A row-level price exception belongs to the root Build's own material
  // row (the Worksheet's per-row override); a nested consumer's inputs are
  // priced by that consumer Build's own configuration.
  const rootConsumes = line.consumers.some((consumer) => consumer.buildId === rootBuildId);
  const rootPricing = rootConsumes ? rootEditing?.pricing(line.typeId, "material") ?? null : null;
  const nestedConsumers = line.consumers.filter((consumer) => consumer.buildId !== rootBuildId);
  const multiEdge = line.consumers.length > 1;

  return (
    <PlannerInspectorShell
      closeLabel="Close input inspector"
      dismissLabel="Dismiss input inspector"
      eyebrow="Input to Source"
      onClose={onClose}
      open
      returnFocusRowKey={String(line.typeId)}
      title={line.typeName}
    >
      <>
        <div className="flex items-center gap-2 border-b border-border px-3 py-2">
          <EveTypeImage size={32} typeId={line.typeId} typeName={line.typeName} />
          <Badge square tone="warning">BUY</Badge>
          {line.sourceStrategy === "mixed" ? (
            <span
              className="text-[10px] uppercase text-muted"
              title="This type is also produced elsewhere in this Build"
            >
              Also produced elsewhere
            </span>
          ) : null}
        </div>

        <InspectorSection
          id="sourcing"
          label="Sourcing"
          summary={`${line.consumers.length} consumer${line.consumers.length === 1 ? "" : "s"}`}
        >
          {readOnly ? (
            <p className="mb-2 text-[11px] text-muted">
              Bought, as frozen when the Epic was created.
            </p>
          ) : methods.length === 0 ? (
            <p className="mb-2 text-[11px] text-muted">
              Buy only -- no published blueprint or reaction formula produces {line.typeName}.
            </p>
          ) : multiEdge ? (
            <p className="mb-2 text-[11px] text-muted">
              Each consumer buys this separately; switching one consumer to production changes only
              that consumer.
            </p>
          ) : (
            <p className="mb-2 text-[11px] text-muted">
              Switching to production adds the producer to the plan and selects it, so its
              blueprint or formula, ME/TE and facility can be configured next.
            </p>
          )}
          {line.consumers.length === 0 ? (
            <p className="text-xs text-muted">No consumer evidence is available for this shortage.</p>
          ) : (
            <ul className="space-y-2">
              {line.consumers.map((consumer, index) => {
                const consumerName = nodesById.get(consumer.nodeId)?.outputTypeName ?? "Unknown";
                const key = edgeKey(consumer.buildId, line.typeId);
                return (
                  <li
                    className="flex flex-wrap items-center justify-between gap-2 rounded border border-border p-2"
                    key={`${consumer.nodeId}-${consumer.occurrenceId}-${index}`}
                  >
                    <p className="text-xs font-semibold">For {consumerName}</p>
                    {readOnly ? <span className="text-xs">Buy</span> : <SourcingSwitch
                      consumerName={consumerName}
                      current={null}
                      disabled={methods.length === 0}
                      error={sourcing.errorByEdge[key] ?? null}
                      methods={methods}
                      onChange={(choice) => sourcing.change(consumer.buildId, line.typeId, choice)}
                      pending={Boolean(sourcing.pendingByEdge[key])}
                    />}
                  </li>
                );
              })}
            </ul>
          )}
          {!readOnly && multiEdge && methods.length > 0 ? (
            // Explicitly multi-consumer: one visible action that switches
            // every consumer listed above; the per-consumer switches never
            // change another consumer.
            <div className="mt-2 rounded border border-border p-2">
              <InspectorRow label="Total to buy" value={qty(line.requiredQuantity)} />
              <div className="mt-1.5 flex flex-wrap gap-2">
                {methods.map((method) => {
                  const label = method.mode === "reaction" ? "Reaction" : "Build";
                  return (
                    <button
                      className="iw-button-secondary"
                      disabled={producingAll}
                      key={method.mode === "reaction" ? `r${method.reactionFormulaTypeId}` : `m${method.blueprintTypeId}`}
                      onClick={() => {
                        setProducingAll(true);
                        void sourcing
                          .changeAll(
                            line.consumers.map((consumer) => consumer.buildId),
                            line.typeId,
                            method,
                          )
                          .finally(() => setProducingAll(false));
                      }}
                      type="button"
                    >
                      Produce all by {label}
                    </button>
                  );
                })}
              </div>
              <p className="mt-1 text-[11px] text-muted">
                Switches all {line.consumers.length} consumers above to one shared production of{" "}
                {line.typeName}.
              </p>
            </div>
          ) : null}
        </InspectorSection>

        <InspectorSection id="requirement" label="Requirement">
          {line.consumers.map((consumer, index) => {
            const consumerName = nodesById.get(consumer.nodeId)?.outputTypeName ?? "Unknown";
            return (
              <div
                className={multiEdge ? "mb-2 rounded border border-border p-2" : "mb-1"}
                key={`${consumer.nodeId}-${consumer.occurrenceId}-${index}`}
              >
                {multiEdge ? <p className="mb-1 text-xs font-semibold">For {consumerName}</p> : null}
                <InspectorRow label="Required" value={qty(consumer.requiredQuantity)} />
                <ScopeRow
                  consumerName={consumerName}
                  rootEditing={consumer.buildId === rootBuildId ? rootEditing : null}
                  scope={consumer.fulfillmentScope}
                  typeId={line.typeId}
                />
                <InspectorRow label="Planned inventory use" value={qty(consumer.plannedInventoryQuantity)} />
                <InspectorRow label="Shortage" value={qty(consumer.quantity)} />
              </div>
            );
          })}
          {multiEdge ? (
            <>
              <InspectorRow label="Total required" value={qty(line.requiredQuantity)} />
              <InspectorRow label="Total planned inventory use" value={qty(line.plannedInventoryQuantity)} />
            </>
          ) : null}
          <InspectorRow label="Available (whole tree)" value={qty(line.availableQuantity)} />
          <InspectorRow label="External shortage" value={qty(line.shortageQuantity)} />
          {produced.length > 0 ? <SourcingSplit acquisition={line} nodesById={nodesById} produced={produced} /> : null}
        </InspectorSection>

        {epicStock ? (
          <InspectorSection id="epic-stock" label="Epic">
            <InspectorRow label="Reserved" value={qty(epicStock.reserved)} />
            <InspectorRow label="Used" value={qty(epicStock.consumed)} />
            <InspectorRow label="Still needed" value={qty(epicStock.remainingNeed)} />
          </InspectorSection>
        ) : null}

        <InspectorSection id="pricing" label="Pricing">
          <InspectorRow
            label="Unit price"
            value={line.freshUnitPrice != null ? <MoneyAmount value={line.freshUnitPrice} /> : "Unpriced"}
          />
          <InspectorRow
            label="Est. fresh cost"
            value={line.freshCost != null ? <MoneyAmount value={line.freshCost} /> : "Unpriced"}
          />
          {multiEdge
            ? line.consumers.map((consumer, index) => (
                <InspectorRow
                  key={`${consumer.nodeId}-${consumer.occurrenceId}-${index}`}
                  label={`For ${nodesById.get(consumer.nodeId)?.outputTypeName ?? "Unknown"}`}
                  value={consumer.freshCost != null ? <MoneyAmount value={consumer.freshCost} /> : "Unpriced"}
                />
              ))
            : null}
          {line.freshPriceStale ? (
            <p className="pt-1 text-[11px] text-warning">At least one price is stale.</p>
          ) : null}
          {line.freshCost == null && line.shortageQuantity > 0 ? (
            <ConfigurationWarning>
              No price for {line.typeName} -- Build economics stay incomplete until it is priced
              {rootPricing ? " (set a manual price below)" : ""}.
            </ConfigurationWarning>
          ) : null}
          {rootPricing && rootEditing ? (
            <div className="mt-2 border-t border-border pt-2">
              <InspectorRow label="Pricing mode" value={rootPricing.summary} />
              <PricingBody onChange={rootEditing.onPricingChange} slice={rootPricing} />
            </div>
          ) : null}
          {!readOnly && nestedConsumers.length > 0 ? (
            <p className="pt-1 text-[11px] text-muted">
              {rootPricing
                ? "Other consumers are priced by their own Build's settings -- override there: "
                : "Priced by the consuming Build's own settings -- override there: "}
              {nestedConsumers.map((consumer, index) => (
                <span key={`${consumer.buildId}-${index}`}>
                  {index > 0 ? ", " : ""}
                  <a
                    className="font-semibold text-primary hover:underline"
                    href={consumer.buildId === rootBuildId
                      ? `/builds/${rootBuildId}`
                      : `/builds/${rootBuildId}/producers/${consumer.buildId}`}
                  >
                    {nodesById.get(consumer.nodeId)?.outputTypeName ?? "Build"}
                  </a>
                </span>
              ))}
              .
            </p>
          ) : null}
        </InspectorSection>
      </>
    </PlannerInspectorShell>
  );
}

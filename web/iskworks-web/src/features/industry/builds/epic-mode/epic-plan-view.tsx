import { useCallback, useEffect, useState, type ReactNode } from "react";
import {
  getOrder,
  getOrderCoverage,
  type EpicCoverageLine,
  type OrderDetail,
  type OrderRequirement,
  type PlanOperationView,
  type RequirementKind,
  type TicketStatus,
} from "../../../../api/industry/orders";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { InlineAlert, LoadingState } from "../../../../components/primitives";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { apiMessage } from "../../shared/api-error";

const COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(220px,1fr)", sticky: true },
  { key: "source", label: "Source", width: "80px" },
  { key: "required", label: "Required", width: "100px", align: "right", numeric: true },
  { key: "reserved", label: "Reserved", width: "100px", align: "right", numeric: true },
  { key: "used", label: "Used", width: "96px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "needed", label: "Still Needed", width: "110px", align: "right", numeric: true },
  {
    key: "free",
    label: "Free Now",
    title: "Free stock of this item (not reserved by any Epic) that could cover what's still needed",
    width: "100px",
    align: "right",
    numeric: true,
    hideBelow: "tablet",
  },
];

const SOURCE_LABELS: Record<RequirementKind, string> = { buy: "Buy", build: "Build", react: "React" };

const TICKET_STATUS_LABELS: Record<TicketStatus, string> = {
  todo: "To do",
  inProgress: "In progress",
  complete: "Done",
  canceled: "Canceled",
};

function Quantity({ value, className = "" }: { value: number; className?: string }) {
  return <span className={`tabular-nums ${className}`}>{value.toLocaleString()}</span>;
}

/**
 * The Plan tab with an Epic selected: that Epic's frozen operations, with
 * live reservations, usage and ticket progress. Read-only -- the Build draft
 * is edited with No Epic selected. Finished operations stay visible as done.
 */
export function EpicPlanView({
  epicId,
  buildRevision,
  active,
  actions,
  toolbar,
  reloadKey = 0,
}: {
  epicId: string;
  /** The Build's current revision, to flag edits made after the freeze. */
  buildRevision: number;
  active: boolean;
  /** Per-requirement row action (e.g. create a ticket). */
  actions?: (requirement: OrderRequirement, line: EpicCoverageLine | undefined) => ReactNode;
  /** Shown above the table (e.g. a Create ticket button). */
  toolbar?: ReactNode;
  /** Bump to reload after a change made from this view. */
  reloadKey?: number;
}) {
  const [order, setOrder] = useState<OrderDetail | null>(null);
  const [coverage, setCoverage] = useState<Map<string, EpicCoverageLine>>(new Map());
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    setError("");
    try {
      const [detail, epicCoverage] = await Promise.all([getOrder(epicId), getOrderCoverage(epicId)]);
      setOrder(detail);
      setCoverage(new Map(epicCoverage.lines.map((line) => [line.requirementId, line])));
    } catch (requestError) {
      setError(apiMessage(requestError));
    }
  }, [epicId]);

  useEffect(() => {
    setOrder(null);
  }, [epicId]);

  useEffect(() => {
    if (active) void load();
  }, [active, load, reloadKey]);

  if (error) return <InlineAlert title="Epic not loaded">{error}</InlineAlert>;
  if (!order || order.id !== epicId) return <LoadingState>Loading Epic...</LoadingState>;

  // Build order, like the Plan view: earliest stage first, the final
  // product last. Stage numbers come from the frozen operation DAG (`0` =
  // consumes nothing produced in this Epic).
  const rootKey = order.productionPlan?.rootOccurrenceKey ?? null;
  const operations: PlanOperationView[] = [...(order.productionPlan?.operations ?? [])]
    .sort((a, b) => a.stage - b.stage || a.occurrenceKey.localeCompare(b.occurrenceKey));
  const sections = epicPlanSections(operations, rootKey);
  const requirementsByOperation = new Map<string, OrderRequirement[]>();
  for (const requirement of order.requirements) {
    const key = requirement.operationOccurrenceKey ?? "";
    requirementsByOperation.set(key, [...(requirementsByOperation.get(key) ?? []), requirement]);
  }
  const changedSinceFreeze = order.sourceBuildRevision !== buildRevision;

  return (
    <div className="space-y-3">
      <InlineAlert title={`Epic: ${order.displayName}`} tone="info">
        Read-only. This is the Epic&apos;s frozen plan with live reservations and ticket progress. Choose
        No Epic to edit the Build.
      </InlineAlert>
      {toolbar ? <div className="flex justify-end">{toolbar}</div> : null}
      {changedSinceFreeze ? (
        <InlineAlert title="The Build has changed since this Epic was frozen" tone="warning">
          This view shows the plan as it was when the Epic was created, not the Build&apos;s current recipe,
          sourcing or runs.
        </InlineAlert>
      ) : null}
      <section aria-label="Epic plan" className="space-y-5">
        {sections.map((section) => (
          <div
            className={section.final ? "rounded-md border border-primary/30 p-2" : ""}
            key={section.title}
          >
            <h3
              className={section.final
                ? "mb-2 flex items-baseline gap-2 text-sm font-semibold text-foreground"
                : "mb-2 flex items-baseline gap-2 text-xs font-semibold uppercase tracking-wide text-muted"}
            >
              {section.title}
              {section.subtitle ? (
                <span className="text-[11px] font-normal normal-case text-muted">{section.subtitle}</span>
              ) : null}
            </h3>
            <OperationalTable
              ariaLabel={section.title}
              columns={COLUMNS}
              onSelectRow={() => undefined}
              selectedRowKey={null}
            >
            {section.operations.map((operation) => {
              const rows = requirementsByOperation.get(operation.occurrenceKey) ?? [];
              const done = operation.ticketStatus === "complete";
              return (
                <OperationalTableGroup
                  groupKey={operation.occurrenceKey}
                  itemCount={rows.length}
                  key={operation.occurrenceKey}
                  label={operation.productName}
                  labelCase="normal"
                  leading={<EveTypeImage size={24} typeId={operation.productTypeId} typeName={operation.productName} />}
                  showCount={false}
                  status={done ? "positive" : "neutral"}
                  summary={(
                    <span className="text-xs text-muted">
                      ×{operation.producedQuantity.toLocaleString()} · {operation.runs.toLocaleString()} run
                      {operation.runs === 1 ? "" : "s"}
                      {operation.ticketDisplayId
                        ? ` · ${operation.ticketDisplayId}${operation.ticketStatus ? ` ${TICKET_STATUS_LABELS[operation.ticketStatus]}` : ""}`
                        : " · No ticket"}
                    </span>
                  )}
                  emptyMessage="No inputs"
                >
                  {rows.map((requirement) => {
                    const line = coverage.get(requirement.id);
                    const needed = line?.remainingNeed ?? requirement.requiredQuantity;
                    return (
                      <OperationalTableRow
                        cells={{
                          item: (
                            <span className="flex min-w-0 items-center gap-2">
                              <EveTypeImage size={24} typeId={requirement.typeId} typeName={requirement.capturedName} />
                              <span className="min-w-0 truncate">{requirement.capturedName}</span>
                              {actions ? <span className="ml-auto shrink-0">{actions(requirement, line)}</span> : null}
                            </span>
                          ),
                          source: <span className="text-xs text-muted">{SOURCE_LABELS[requirement.kind]}</span>,
                          required: <Quantity value={requirement.requiredQuantity} />,
                          reserved: <Quantity value={line?.reserved ?? 0} />,
                          used: <Quantity value={line?.consumed ?? 0} />,
                          needed: <Quantity className={needed > 0 && !done ? "text-warning" : ""} value={needed} />,
                          free: <Quantity className="text-muted" value={line?.freeCoverable ?? 0} />,
                        }}
                        interactive={false}
                        key={requirement.id}
                        rowKey={requirement.id}
                        status={done || needed === 0 ? "positive" : "neutral"}
                      />
                    );
                  })}
                </OperationalTableGroup>
              );
            })}
            </OperationalTable>
          </div>
        ))}
      </section>
    </div>
  );
}

interface EpicPlanSection {
  title: string;
  subtitle?: string;
  final: boolean;
  operations: PlanOperationView[];
}

/**
 * The Plan view's layout for an Epic's frozen operations: "Stage N" per
 * dependency stage, earliest first, then the root operation as Final
 * Production. `operations` must already be in build order.
 */
export function epicPlanSections(
  operations: PlanOperationView[],
  rootOccurrenceKey: string | null,
): EpicPlanSection[] {
  const byStage = new Map<number, PlanOperationView[]>();
  const finals: PlanOperationView[] = [];
  for (const operation of operations) {
    if (operation.occurrenceKey === rootOccurrenceKey) {
      finals.push(operation);
      continue;
    }
    byStage.set(operation.stage, [...(byStage.get(operation.stage) ?? []), operation]);
  }
  const sections: EpicPlanSection[] = [...byStage.keys()]
    .sort((a, b) => a - b)
    .map((stage, index) => ({
      title: `Stage ${index + 1}`,
      subtitle: index === 0 ? "Earliest production" : undefined,
      final: false,
      operations: byStage.get(stage) ?? [],
    }));
  if (finals.length > 0) {
    sections.push({ title: "Final Production", final: true, operations: finals });
  }
  return sections;
}

import type {
  AcquisitionLine,
  ExecutionNode,
  ExecutionRequirement,
} from "../../../../api/industry";
import { Badge, type Tone } from "../../../../components/primitives";
import { InspectorRow } from "../../inspector/inspector-section";

import type { StagesSelection } from "./stages-inspector";

interface Props {
  acquisitions: AcquisitionLine[];
  requirements: ExecutionRequirement[];
  nodes: ExecutionNode[];
  onSelect: (selection: StagesSelection) => void;
}

const qty = (value: number) => value.toLocaleString("en-US");

const sourcePresentation: Record<ExecutionRequirement["resolution"], { label: string; tone: Tone }> = {
  buy: { label: "Buy", tone: "warning" },
  build: { label: "Build", tone: "primary" },
  reaction: { label: "Reaction", tone: "reaction" },
  unresolved: { label: "Unresolved", tone: "danger" },
};

export function OperationRequirements({ acquisitions, requirements, nodes, onSelect }: Props) {
  if (requirements.length === 0) {
    return <p className="text-[11px] text-muted">No direct material requirements.</p>;
  }

  const nodesById = new Map(nodes.map((node) => [node.id, node]));
  const acquisitionTypeIds = new Set(acquisitions.map((line) => line.typeId));

  return (
    <ul className="space-y-2">
      {requirements.map((requirement) => {
        const producer = requirement.producerNodeId
          ? nodesById.get(requirement.producerNodeId)
          : undefined;
        const target: StagesSelection | null = requirement.producerNodeId
          ? { kind: "production", nodeId: requirement.producerNodeId }
          : requirement.resolution === "buy" && acquisitionTypeIds.has(requirement.typeId)
            ? { kind: "acquisition", typeId: requirement.typeId }
            : null;
        const source = sourcePresentation[requirement.resolution];
        const content = (
          <>
            <div className="flex items-start justify-between gap-2">
              <span className="text-left text-xs font-semibold text-foreground">{requirement.typeName}</span>
              <Badge square tone={source.tone}>{source.label}</Badge>
            </div>
            {producer && producer.productionDemand !== requirement.requiredQuantity ? (
              <p className="mt-1 text-left text-[10px] text-muted">Total planned {qty(producer.productionDemand)}</p>
            ) : null}
            <div className="mt-1 border-t border-border/70 pt-1">
              <InspectorRow label="Required here" value={qty(requirement.requiredQuantity)} />
              <InspectorRow label="Planned inventory use" value={qty(requirement.plannedInventoryQuantity)} />
              <InspectorRow label="Shortage" value={qty(requirement.shortageQuantity)} />
            </div>
          </>
        );

        return (
          <li key={`${requirement.dependencyId}:${requirement.typeId}`}>
            {target ? (
              <button
                aria-label={`Open ${requirement.typeName}`}
                className="w-full rounded border border-border bg-panel-strong p-2 text-left transition hover:border-primary/60 focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
                onClick={() => onSelect(target)}
                type="button"
              >
                {content}
              </button>
            ) : (
              <div className="rounded border border-border bg-panel-strong p-2">{content}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

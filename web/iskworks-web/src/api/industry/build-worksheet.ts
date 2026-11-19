import type { BlueprintKind, PreviewBuildPlanCommand } from "./builds";
import type { CostWarning } from "./execution-plan";
import type { MarketPricingPolicy } from "./market";
import { json, request } from "./request";
import type { Money } from "./shared";

export type WorksheetSourcing = "buy" | "manufacturing" | "reaction" | "unresolved";
export type WorksheetEvidenceState = "complete" | "unpriced" | "incomplete" | "notApplicable";
export type WorksheetPricingClassification = "production" | "default" | "manual" | "marketPolicy" | "mixed" | "unresolved";

export type BlueprintCoverageState = "owned" | "insufficientRuns" | "unknownKind" | "notOwned";
export type ProducerBlueprintSource = "manual" | "observedAsset";

export interface OwnedBlueprintRef {
  observationId: string;
  kind: BlueprintKind;
  materialEfficiency: number;
  timeEfficiency: number;
  licensedRuns: number | null;
  locationName: string | null;
}

export interface BlueprintHoldingSummary {
  originals: number;
  copies: number;
  unknown: number;
  copyRuns: number;
  best: OwnedBlueprintRef | null;
  observedAt: string | null;
}

export interface ProducerBlueprintUse {
  requiredRuns: number;
  source: ProducerBlueprintSource;
  observationId: string | null;
  materialEfficiency: number | null;
  timeEfficiency: number | null;
}

/** Present on Buy/Manufacturing rows whose type has a manufacturing blueprint. */
export interface WorksheetBlueprintCoverage {
  blueprintTypeId: number;
  blueprintName: string;
  holdings: BlueprintHoldingSummary;
  producer: ProducerBlueprintUse | null;
  state: BlueprintCoverageState;
}

/** Counted once per blueprint type over the rows in scope. */
export interface WorksheetBlueprintSummary {
  owned: number;
  missing: number;
  ownedButBought: number;
  producerUsesManual: number;
}

export interface WorksheetPricingEvidence {
  state: WorksheetEvidenceState;
  classification: WorksheetPricingClassification;
  policy: MarketPricingPolicy | null;
  sourceNote: string | null;
}

export interface BuildWorksheetRow {
  id: string;
  typeId: number;
  typeName: string;
  categoryId: number | null;
  categoryName: string | null;
  groupId: number | null;
  groupName: string | null;
  sourcing: WorksheetSourcing;
  requiredQuantity: number | null;
  coveredQuantity: number | null;
  shortageQuantity: number | null;
  coveragePercentage: string | null;
  evidenceState: WorksheetEvidenceState;
  pricing: WorksheetPricingEvidence;
  unitCost: Money | null;
  totalValue: Money | null;
  producerBuildId: string | null;
  retainedSurplusQuantity: number | null;
  retainedSurplusBasis: Money | null;
  warnings: CostWarning[];
  blueprint: WorksheetBlueprintCoverage | null;
}

export interface BuildWorksheetGroup {
  key: string;
  label: string;
  rowCount: number;
  complete: boolean;
  rows: BuildWorksheetRow[];
}

export interface BuildWorksheetOutput {
  typeId: number;
  typeName: string;
  quantity: number | null;
  unitValue: Money | null;
  totalValue: Money | null;
  evidenceState: WorksheetEvidenceState;
}

export interface BuildWorksheetProjection {
  scope: { rootBuildId: string; focusedProducerId: string | null; includeDownstream: boolean; label: string };
  groups: BuildWorksheetGroup[];
  output: BuildWorksheetOutput;
  warnings: CostWarning[];
  blueprints: WorksheetBlueprintSummary;
  economicsAreAdditive: false;
  generatedAt: string;
}

export function postBuildWorksheet(
  rootBuildId: string,
  command: PreviewBuildPlanCommand,
  focusedProducerId: string | null,
  includeDownstream: boolean,
  signal?: AbortSignal,
): Promise<BuildWorksheetProjection> {
  return request(`/api/builds/${rootBuildId}/worksheet`, {
    ...json("POST", { command, focusedProducerId, includeDownstream }),
    signal,
  });
}

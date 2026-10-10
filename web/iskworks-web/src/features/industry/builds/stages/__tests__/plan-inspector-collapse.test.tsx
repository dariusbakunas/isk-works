import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AcquisitionConsumerRef,
  AcquisitionLine,
  CostWarning,
  ExecutionNode,
  ExecutionOccurrence,
  ExecutionPlanProjection,
  ExecutionStage,
} from "../../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildStagesView } from "../build-stages-view";

const postBuildExecutionPlan = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  postBuildExecutionPlan: (...args: unknown[]) => postBuildExecutionPlan(...args),
}));

const OVERLAY = {
  recipe: { mode: "manufacturing", blueprintTypeId: 80_000 },
  runs: 1,
  componentResolutions: [],
};


/** A production consumer edge: `quantity` is its production demand. */

function editorStub(over: Partial<BuildWorksheetEditorModel> = {}): BuildWorksheetEditorModel {
  return {
    initialBuild: { id: "build-1" },
    previewKey: JSON.stringify(OVERLAY),
    linkedBuildsByTypeId: {},
    ...over,
  } as unknown as BuildWorksheetEditorModel;
}

function occurrence(
  over: Partial<ExecutionOccurrence> & { id: string; nodeId: string },
): ExecutionOccurrence {
  return {
    buildId: over.id,
    revision: 1,
    blueprintSelection: null,
    isRoot: false,
    stage: 0,
    activity: "manufacturing",
    outputTypeId: 1,
    outputTypeName: "Item",
    blueprintOrFormulaTypeId: 1,
    blueprintOrFormulaName: "Item Blueprint",
    facilityId: null,
    facilityName: null,
    effectiveMe: null,
    effectiveTe: null,
    projectedRuns: 1,
    projectedOutput: 1,
    requiredQuantity: 0,
    plannedInventoryQuantity: 0,
    productionDemand: 0,
    retainedSurplusQuantity: 0,
    retainedSurplusCost: null,
    materialComponentCost: null,
    ownInstallationCost: null,
    totalProductionCost: null,
    unitProductionCost: null,
    costComplete: true,
    requirements: [],
    ...over,
  };
}

function node(over: Partial<ExecutionNode> & { id: string; occurrenceIds: string[] }): ExecutionNode {
  return {
    outputTypeId: 1,
    outputTypeName: "Item",
    activity: "manufacturing",
    stage: 0,
    facilityId: null,
    facilityName: null,
    effectiveMe: null,
    effectiveTe: null,
    requiredQuantity: 0,
    plannedInventoryQuantity: 0,
    productionDemand: 0,
    projectedOutput: 0,
    projectedRuns: 0,
    retainedSurplusQuantity: 0,
    retainedSurplusCost: null,
    materialComponentCost: null,
    ownInstallationCost: null,
    totalProductionCost: null,
    costComplete: true,
    consumers: [],
    productionMethods: [],
    unitProductionCost: null,
    availableQuantity: 0,
    ...over,
  };
}

function acquisition(over: Partial<AcquisitionLine> & { typeId: number }): AcquisitionLine {
  return {
    typeName: `Type ${over.typeId}`,
    requiredQuantity: 100,
    plannedInventoryQuantity: 0,
    shortageQuantity: 100,
    availableQuantity: 0,
    reservedQuantity: 0,
    sourceStrategy: "buy",
    consumers: [],
    productionMethods: [],
    freshCost: null,
    freshUnitPrice: null,
    freshPriceStale: false,
    ...over,
  };
}

/** One acquisition demand edge: `quantity` is the consumer's own shortage;
 * the consuming Build defaults to the occurrence id. */
function edge(
  nodeId: string,
  occurrenceId: string,
  quantity: number,
  over: Partial<AcquisitionConsumerRef> = {},
): AcquisitionConsumerRef {
  return {
    nodeId,
    occurrenceId,
    quantity,
    buildId: occurrenceId,
    dependencyId: `pd:${occurrenceId}`,
    fulfillmentScope: "missing",
    requiredQuantity: quantity,
    plannedInventoryQuantity: 0,
    freshCost: null,
    freshUnitPrice: null,
    ...over,
  };
}

function stage(index: number, nodeIds: string[]): ExecutionStage {
  return { index, nodeIds };
}

function plan(over: Partial<ExecutionPlanProjection> = {}): ExecutionPlanProjection {
  return {
    rootNodeId: "root",
    stages: [],
    nodes: [],
    edges: [],
    occurrences: [],
    acquisitions: [],
    unresolved: [],
    complete: true,
    warnings: [],
    generatedAt: "2026-01-01T00:00:00Z",
    logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
    ...over,
  };
}

async function flush() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(300);
  });
}

function renderView(over?: Partial<BuildWorksheetEditorModel>) {
  return render(<BuildStagesView active editor={editorStub(over)} />);
}

function rowFor(id: string): HTMLElement {
  const el = document.querySelector<HTMLElement>(`tr[data-row-key="${id}"]`);
  if (!el) throw new Error(`no rendered row for ${id}`);
  return el;
}

function sectionButton(label: RegExp): HTMLElement {
  return screen.getByRole("button", { name: label });
}

function expandedSections(): string[] {
  return [...document.querySelectorAll<HTMLElement>("section > h3 > button[aria-expanded='true']")].map(
    (button) => button.textContent ?? "",
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  postBuildExecutionPlan.mockReset();
  postBuildExecutionPlan.mockResolvedValue(
    plan({
      stages: [stage(0, ["root"])],
      nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Ferrogel", stage: 0 })],
      occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Ferrogel", stage: 0 })],
      acquisitions: [
        acquisition({ typeId: 999, typeName: "Raw Material X", consumers: [edge("root", "root", 100)] }),
      ],
    }),
  );
});
afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

describe("Plan inspector sections", () => {
  it("start collapsed", async () => {
    renderView();
    await flush();

    fireEvent.click(rowFor("root"));
    expect(document.querySelectorAll("section > h3 > button").length).toBeGreaterThan(0);
    expect(expandedSections()).toEqual([]);

    fireEvent.click(rowFor("999"));
    expect(expandedSections()).toEqual([]);
  });

  it("stay expanded while switching rows and after closing the inspector", async () => {
    renderView();
    await flush();

    fireEvent.click(rowFor("root"));
    fireEvent.click(sectionButton(/^Economics/));
    fireEvent.click(rowFor("999"));
    fireEvent.click(sectionButton(/^Pricing/));

    fireEvent.click(rowFor("root"));
    expect(sectionButton(/^Economics/)).toHaveAttribute("aria-expanded", "true");

    fireEvent.click(screen.getByRole("button", { name: "Close production inspector" }));
    fireEvent.click(rowFor("999"));
    expect(sectionButton(/^Pricing/)).toHaveAttribute("aria-expanded", "true");
    expect(sectionButton(/^Sourcing/)).toHaveAttribute("aria-expanded", "false");
  });
});

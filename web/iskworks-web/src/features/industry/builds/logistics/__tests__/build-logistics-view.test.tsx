import { act, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  ExecutionPlanProjection,
  LogisticsDestination,
  LogisticsLine,
} from "../../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildLogisticsView, formatM3 } from "../build-logistics-view";

const postBuildExecutionPlan = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  postBuildExecutionPlan: (...args: unknown[]) => postBuildExecutionPlan(...args),
}));

function line(over: Partial<LogisticsLine> & { typeId: number; typeName: string }): LogisticsLine {
  return {
    quantity: 0,
    plannedInventoryQuantity: 0,
    shortageQuantity: 0,
    acquireQuantity: 0,
    producedQuantity: 0,
    unresolvedQuantity: 0,
    unitVolumeM3: null,
    totalVolumeM3: null,
    consumers: [],
    producers: [],
    ...over,
  };
}

function destination(over: Partial<LogisticsDestination> & { key: string }): LogisticsDestination {
  return {
    facilityId: null,
    facilityName: null,
    solarSystem: null,
    operationIds: [],
    lines: [],
    totalVolumeM3: "0",
    volumeComplete: true,
    ...over,
  };
}

function planWith(destinations: LogisticsDestination[], total: string, complete = true): ExecutionPlanProjection {
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
    logistics: { destinations, totalVolumeM3: total, volumeComplete: complete },
  };
}

function consumer(name: string) {
  return {
    operationId: `build:${name}`,
    buildId: name,
    outputTypeName: name,
    source: "acquire" as const,
    fulfillmentScope: "missing" as const,
    requiredQuantity: 1,
    plannedInventoryQuantity: 0,
    shortageQuantity: 1,
    dependencyId: `pd:${name}`,
  };
}

async function renderLogistics() {
  const editor = {
    initialBuild: { id: "build-1" },
    previewKey: JSON.stringify({ recipe: { mode: "manufacturing", blueprintTypeId: 1 }, runs: 1 }),
    linkedBuildsByTypeId: {},
  } as unknown as BuildWorksheetEditorModel;
  render(<BuildLogisticsView active editor={editor} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(300);
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  postBuildExecutionPlan.mockReset();
});
afterEach(() => vi.useRealTimers());

describe("BuildLogisticsView", () => {
  it("groups lines by destination facility with per-destination and total cargo volume", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      planWith(
        [
          destination({
            key: "facility:a",
            facilityId: "a",
            facilityName: "Home Raitaru",
            solarSystem: "Jita",
            totalVolumeM3: "100.00",
            lines: [
              line({
                typeId: 34,
                typeName: "Tritanium",
                quantity: 10_000,
                plannedInventoryQuantity: 4_000,
                shortageQuantity: 6_000,
                acquireQuantity: 6_000,
                unitVolumeM3: "0.01",
                totalVolumeM3: "100.00",
                consumers: [consumer("Squall"), consumer("Life Support Backup Unit")],
              }),
            ],
          }),
          destination({
            key: "facility:b",
            facilityId: "b",
            facilityName: "Reaction Tatara",
            solarSystem: "Perimeter",
            totalVolumeM3: "50",
            lines: [
              line({
                typeId: 34,
                typeName: "Tritanium",
                quantity: 5_000,
                acquireQuantity: 5_000,
                shortageQuantity: 5_000,
                unitVolumeM3: "0.01",
                totalVolumeM3: "50",
                consumers: [consumer("Reinforced Carbon Fiber")],
              }),
            ],
          }),
        ],
        "150.00",
      ),
    );
    await renderLogistics();

    expect(screen.getByText("150 m³")).toBeInTheDocument();
    expect(screen.getByText(/across 2 destinations/)).toBeInTheDocument();

    const home = within(screen.getByRole("table", { name: "Logistics for Home Raitaru" }));
    expect(home.getByText("Tritanium")).toBeInTheDocument();
    expect(home.getByText("10,000")).toBeInTheDocument();
    expect(home.getByText("4,000")).toBeInTheDocument();
    expect(home.getByText("6,000")).toBeInTheDocument();
    expect(home.getByText("For Squall, Life Support Backup Unit")).toBeInTheDocument();
    expect(home.getByText("100")).toBeInTheDocument();

    // The same item at a different destination stays its own line.
    const tatara = within(screen.getByRole("table", { name: "Logistics for Reaction Tatara" }));
    expect(tatara.getAllByText("5,000").length).toBeGreaterThan(0);
    expect(tatara.getByText("50")).toBeInTheDocument();
    expect(screen.getByText("Perimeter")).toBeInTheDocument();
  });

  it("marks unknown volume and an unassigned destination honestly", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      planWith(
        [
          destination({
            key: "unassigned",
            volumeComplete: false,
            lines: [line({ typeId: 9, typeName: "Mystery Part", quantity: 7, unresolvedQuantity: 7 })],
          }),
        ],
        "0",
        false,
      ),
    );
    await renderLogistics();

    expect(screen.getByText("No facility selected")).toBeInTheDocument();
    expect(screen.getByText(/some items have no known volume/)).toBeInTheDocument();
    const table = within(screen.getByRole("table", { name: "Logistics for No facility selected" }));
    expect(table.getAllByText("—").length).toBe(2);
  });

  it("formats volume for display without changing the decimal string it came from", () => {
    expect(formatM3("1234.5678")).toBe("1,234.57");
    expect(formatM3("0.0125", true)).toBe("0.0125");
    expect(formatM3(null)).toBe("—");
  });
});

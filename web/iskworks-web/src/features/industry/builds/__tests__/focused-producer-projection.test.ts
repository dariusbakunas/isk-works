import { describe, expect, it } from "vitest";

import type { ExecutionPlanProjection } from "../../../../api/industry";
import { focusExecutionPlan } from "../focused-producer-projection";

describe("focusExecutionPlan", () => {
  it("selects the producer and prerequisites while preserving shared producer aggregate runs", () => {
    const plan = {
      rootNodeId: "squall",
      planner: "canonical",
      stages: [
        { index: 0, nodeIds: ["rcf"] },
        { index: 1, nodeIds: ["auto", "life"] },
        { index: 2, nodeIds: ["squall"] },
      ],
      nodes: [
        { id: "rcf", projectedRuns: 8, productionDemand: 161, consumers: [
          { nodeId: "auto", quantity: 107, requiredQuantity: 107, plannedInventoryQuantity: 0 },
          { nodeId: "life", quantity: 54, requiredQuantity: 54, plannedInventoryQuantity: 0 },
        ] },
        { id: "auto", projectedRuns: 12, productionDemand: 34, consumers: [{ nodeId: "squall", quantity: 34, requiredQuantity: 34, plannedInventoryQuantity: 0 }] },
        { id: "life", projectedRuns: 3, productionDemand: 10, consumers: [{ nodeId: "squall", quantity: 10, requiredQuantity: 10, plannedInventoryQuantity: 0 }] },
        { id: "squall", projectedRuns: 1, productionDemand: 0, consumers: [] },
      ],
      edges: [
        { from: "rcf", to: "auto" }, { from: "rcf", to: "life" },
        { from: "auto", to: "squall" }, { from: "life", to: "squall" },
      ],
      occurrences: [
        { id: "build:rcf", nodeId: "rcf", buildId: "rcf-build" },
        {
          id: "build:auto",
          nodeId: "auto",
          buildId: "auto-build",
          projectedRuns: 12,
          requirements: [{
            typeId: 57_457,
            typeName: "Reinforced Carbon Fiber",
            requiredQuantity: 107,
            plannedInventoryQuantity: 0,
            shortageQuantity: 107,
            fulfillmentScope: "missing",
            resolution: "reaction",
            dependencyId: "pd:auto-rcf",
            producerBuildId: "rcf-build",
            producerNodeId: "rcf",
          }],
        },
        { id: "build:life", nodeId: "life", buildId: "life-build" },
        { id: "root:squall", nodeId: "squall", buildId: "squall-build" },
      ],
      acquisitions: [{
        typeId: 2,
        requiredQuantity: 15,
        plannedInventoryQuantity: 0,
        shortageQuantity: 15,
        freshCost: "30.0000",
        freshUnitPrice: "2.0000",
        freshPriceStale: false,
        consumers: [
          { nodeId: "auto", quantity: 10, requiredQuantity: 10, plannedInventoryQuantity: 0, freshCost: "20.0000", freshUnitPrice: "2.0000" },
          { nodeId: "life", quantity: 5, requiredQuantity: 5, plannedInventoryQuantity: 0, freshCost: "10.0000", freshUnitPrice: "2.0000" },
        ],
      }],
      unresolved: [],
      logistics: {
        totalVolumeM3: "161.0",
        volumeComplete: true,
        destinations: [{
          key: "facility:one",
          operationIds: ["build:auto", "build:life"],
          totalVolumeM3: "161.0",
          volumeComplete: true,
          lines: [{
            typeId: 1,
            quantity: 161,
            plannedInventoryQuantity: 0,
            shortageQuantity: 161,
            acquireQuantity: 0,
            producedQuantity: 161,
            unresolvedQuantity: 0,
            unitVolumeM3: "1.0",
            totalVolumeM3: "161.0",
            producers: [{ quantity: 161 }],
            consumers: [
              { operationId: "build:auto", source: "produced", requiredQuantity: 107, plannedInventoryQuantity: 0, shortageQuantity: 107 },
              { operationId: "build:life", source: "produced", requiredQuantity: 54, plannedInventoryQuantity: 0, shortageQuantity: 54 },
            ],
          }],
        }],
      },
    } as unknown as ExecutionPlanProjection;

    const auto = focusExecutionPlan(plan, "auto-build");
    expect(auto?.rootNodeId).toBe("auto");
    expect(auto?.nodes.map((node) => node.id)).toEqual(["rcf", "auto"]);
    expect(auto?.nodes[0].projectedRuns).toBe(8);
    expect(auto?.nodes[0].productionDemand).toBe(161);
    expect(auto?.nodes[0].requiredQuantity).toBe(107);
    expect(auto?.nodes[0].consumers).toEqual([
      { nodeId: "auto", quantity: 107, requiredQuantity: 107, plannedInventoryQuantity: 0 },
    ]);
    expect(auto?.logistics.destinations[0].lines[0]).toMatchObject({
      quantity: 107,
      producedQuantity: 107,
      totalVolumeM3: "107.0",
      producers: [{ quantity: 107 }],
    });
    expect(auto?.logistics.totalVolumeM3).toBe("107.0");
    expect(auto?.acquisitions[0]).toMatchObject({
      requiredQuantity: 10,
      shortageQuantity: 10,
      freshCost: "20.0000",
      freshUnitPrice: "2.0000",
    });
    const rootOccurrence = plan.occurrences.find((item) => item.buildId === "auto-build")!;
    const focusedOccurrence = auto!.occurrences.find((item) => item.buildId === "auto-build")!;
    expect(focusedOccurrence.projectedRuns).toBe(12);
    expect(focusedOccurrence.requirements).toEqual(rootOccurrence.requirements);
    expect(focusedOccurrence.requirements[0]).toMatchObject({
      typeName: "Reinforced Carbon Fiber",
      requiredQuantity: 107,
      plannedInventoryQuantity: 0,
      shortageQuantity: 107,
      resolution: "reaction",
    });

    const rcf = focusExecutionPlan(plan, "rcf-build");
    expect(rcf?.nodes.map((node) => node.id)).toEqual(["rcf"]);
    expect(rcf?.nodes[0].productionDemand).toBe(161);
  });

  it("returns null when the producer is absent from the root projection", () => {
    const plan = { occurrences: [] } as unknown as ExecutionPlanProjection;
    expect(focusExecutionPlan(plan, "missing")).toBeNull();
  });
});

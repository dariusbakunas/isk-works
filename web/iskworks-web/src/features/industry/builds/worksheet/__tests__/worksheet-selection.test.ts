import { expect, test } from "vitest";

import type { AcquisitionLine, BuildWorksheetRow, ExecutionPlanProjection } from "../../../../../api/industry";
import { rowKeyForTarget, scopeAcquisitionToConsumer, selectionForTarget, targetForRow } from "../worksheet-selection";

const row = (over: Partial<BuildWorksheetRow>): BuildWorksheetRow => ({
  id: "r", typeId: 1, typeName: "T", categoryId: null, categoryName: null, groupId: null, groupName: null,
  sourcing: "buy", requiredQuantity: 10, coveredQuantity: 0, shortageQuantity: 10, coveragePercentage: "0.00",
  evidenceState: "complete", pricing: { state: "complete", classification: "default", policy: null, sourceNote: null },
  unitCost: null, totalValue: null, producerBuildId: null, retainedSurplusQuantity: null, retainedSurplusBasis: null, warnings: [],
  ...over,
});

const consumer = (buildId: string, over = {}) => ({
  nodeId: "n", occurrenceId: "o", quantity: 5, buildId, dependencyId: "d", fulfillmentScope: "missing" as const,
  requiredQuantity: 8, plannedInventoryQuantity: 3, freshCost: "100", freshUnitPrice: "20", ...over,
});

const plan = {
  occurrences: [
    { id: "root:r", nodeId: "n-root", buildId: "root", isRoot: true },
    { id: "build:p", nodeId: "n-p", buildId: "p", isRoot: false },
  ],
  acquisitions: [{ typeId: 1 }],
} as unknown as ExecutionPlanProjection;

test("only producer rows and Buy rows with a shortage have an inspector target", () => {
  expect(targetForRow(row({ sourcing: "manufacturing", producerBuildId: "p" }))).toEqual({ kind: "producer", buildId: "p" });
  expect(targetForRow(row({}))).toEqual({ kind: "buy", typeId: 1 });
  expect(targetForRow(row({ shortageQuantity: 0 }))).toBeNull();
  expect(targetForRow(row({ sourcing: "unresolved", shortageQuantity: null }))).toBeNull();
});

test("targets resolve to plan selections and disappear when the plan no longer has them", () => {
  expect(selectionForTarget(plan, { kind: "producer", buildId: "p" })).toEqual({ kind: "production", nodeId: "n-p" });
  expect(selectionForTarget(plan, { kind: "producer", buildId: "root" })).toBeNull();
  expect(selectionForTarget(plan, { kind: "buy", typeId: 1 })).toEqual({ kind: "acquisition", typeId: 1 });
  expect(selectionForTarget(plan, { kind: "buy", typeId: 2 })).toBeNull();
  expect(selectionForTarget(plan, null)).toBeNull();
});

test("the highlighted row follows the target across a sourcing change", () => {
  const buy = row({ id: "1:buy:none" });
  const build = row({ id: "1:manufacturing:p", sourcing: "manufacturing", producerBuildId: "p" });
  expect(rowKeyForTarget([buy], { kind: "buy", typeId: 1 })).toBe("1:buy:none");
  expect(rowKeyForTarget([build], { kind: "buy", typeId: 1 })).toBeNull();
  expect(rowKeyForTarget([build], { kind: "producer", buildId: "p" })).toBe("1:manufacturing:p");
  expect(rowKeyForTarget([buy], null)).toBeNull();
});

test("an acquisition line is narrowed to the operation's own edges", () => {
  const line = { typeId: 1, requiredQuantity: 20, plannedInventoryQuantity: 6, shortageQuantity: 14, freshCost: "300", freshUnitPrice: "20", consumers: [consumer("root"), consumer("p", { quantity: 9 })] } as unknown as AcquisitionLine;
  const scoped = scopeAcquisitionToConsumer(line, "root");
  expect(scoped).toMatchObject({ requiredQuantity: 8, plannedInventoryQuantity: 3, shortageQuantity: 5, freshCost: "100" });
  expect(scoped.consumers).toHaveLength(1);
  expect(scopeAcquisitionToConsumer(line, "nobody")).toBe(line);
  expect(scopeAcquisitionToConsumer({ ...line, consumers: [consumer("root")] }, "root")).toMatchObject({ shortageQuantity: 14 });
});

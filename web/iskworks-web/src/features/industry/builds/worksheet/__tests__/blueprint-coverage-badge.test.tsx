import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";

import type { WorksheetBlueprintCoverage, WorksheetSourcing } from "../../../../../api/industry";
import { BlueprintCoverageBadge, blueprintBadgeLabel, blueprintBadgeTone, usesManualDespiteOwned } from "../blueprint-coverage-badge";

function coverage(over: Partial<WorksheetBlueprintCoverage> = {}): WorksheetBlueprintCoverage {
  return {
    blueprintTypeId: 21_010,
    blueprintName: "Capital Jump Drive Blueprint",
    holdings: { originals: 0, copies: 0, unknown: 0, copyRuns: 0, best: null, observedAt: null },
    producer: null,
    state: "notOwned",
    ...over,
  };
}

const original = coverage({
  state: "owned",
  holdings: {
    originals: 1, copies: 0, unknown: 0, copyRuns: 0, observedAt: null,
    best: { observationId: "o1", kind: "original", materialEfficiency: 10, timeEfficiency: 20, licensedRuns: null, locationName: null },
  },
});
const copies = coverage({
  state: "owned",
  holdings: {
    originals: 0, copies: 3, unknown: 0, copyRuns: 12, observedAt: null,
    best: { observationId: "c1", kind: "copy", materialEfficiency: 10, timeEfficiency: 20, licensedRuns: 4, locationName: "Jita IV - Moon 4" },
  },
});

describe("label", () => {
  test.each([
    ["an original", original, "BPO 10/20"],
    ["copies, with total runs", copies, "BPC 10/20 · 12r"],
    ["unknown kind", coverage({ state: "unknownKind" }), "BP ?"],
    ["nothing", coverage(), "No BP"],
    [
      "too few runs, against the job",
      coverage({ state: "insufficientRuns", holdings: { ...copies.holdings, copyRuns: 2 }, producer: { requiredRuns: 4, source: "manual", observationId: null, materialEfficiency: 0, timeEfficiency: 0 } }),
      "BPC · 2r < 4",
    ],
  ])("%s", (_name, value, label) => {
    expect(blueprintBadgeLabel(value)).toBe(label);
  });
});

describe("tone by sourcing", () => {
  test.each<[string, WorksheetBlueprintCoverage, WorksheetSourcing, boolean, string]>([
    ["Buy with nothing owned is calm", coverage(), "buy", false, "calm"],
    ["Buy with an owned blueprint is an opportunity", copies, "buy", false, "opportunity"],
    ["Buy covered from inventory is not an opportunity", original, "buy", true, "neutral"],
    ["Build with an owned blueprint is fine", original, "manufacturing", false, "neutral"],
    ["Build with nothing owned is a blocker", coverage(), "manufacturing", false, "blocker"],
    ["too few runs is amber", coverage({ state: "insufficientRuns" }), "manufacturing", false, "short"],
    ["unknown kind stays neutral", coverage({ state: "unknownKind" }), "buy", false, "neutral"],
  ])("%s", (_name, value, sourcing, inventory, tone) => {
    expect(blueprintBadgeTone(value, sourcing, inventory)).toBe(tone);
  });
});

test("flags a Build producer on manual ME/TE when a blueprint is owned", () => {
  const manual = { ...copies, producer: { requiredRuns: 4, source: "manual" as const, observationId: null, materialEfficiency: 10, timeEfficiency: 20 } };
  expect(usesManualDespiteOwned(manual, "manufacturing")).toBe(true);
  expect(usesManualDespiteOwned({ ...manual, producer: { ...manual.producer, source: "observedAsset" } }, "manufacturing")).toBe(false);
  expect(usesManualDespiteOwned(manual, "buy")).toBe(false);

  render(<BlueprintCoverageBadge coverage={manual} sourcing="manufacturing" />);
  expect(screen.getByText("manual")).toBeInTheDocument();
});

test("hover shows what's owned, the best blueprint and the planned job", async () => {
  const value = {
    ...copies,
    holdings: { ...copies.holdings, observedAt: new Date(Date.now() - 12 * 60_000).toISOString() },
    producer: { requiredRuns: 4, source: "manual" as const, observationId: null, materialEfficiency: 10, timeEfficiency: 20 },
  };
  render(<BlueprintCoverageBadge coverage={value} sourcing="manufacturing" />);

  await userEvent.hover(screen.getByRole("group", { name: /Capital Jump Drive Blueprint/ }));

  const tooltip = screen.getByRole("tooltip");
  expect(tooltip).toHaveTextContent("Capital Jump Drive Blueprint");
  expect(tooltip).toHaveTextContent("3 copies · 12 runs");
  expect(tooltip).toHaveTextContent("BPC · ME 10 · TE 20 · 4 runs · Jita IV - Moon 4");
  expect(tooltip).toHaveTextContent("Job needs 4 runs · uses manual ME 10 / TE 20");
  expect(tooltip).toHaveTextContent("As of 12 m ago");
});

test("a Buy row's tooltip has no planned job", async () => {
  render(<BlueprintCoverageBadge coverage={copies} sourcing="buy" />);
  await userEvent.hover(screen.getByRole("group"));
  expect(screen.getByRole("tooltip")).not.toHaveTextContent("Planned job");
});

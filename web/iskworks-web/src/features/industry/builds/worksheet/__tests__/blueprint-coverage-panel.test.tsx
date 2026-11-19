import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { expect, test, vi } from "vitest";

import type { BuildWorksheetRow, WorksheetBlueprintCoverage } from "../../../../../api/industry";
import { BlueprintCoveragePanel, blueprintPanelSections } from "../blueprint-coverage-panel";

const holdings = { originals: 0, copies: 0, unknown: 0, copyRuns: 0, best: null, observedAt: null };
const bpo = { observationId: "o", kind: "original" as const, materialEfficiency: 10, timeEfficiency: 20, licensedRuns: null, locationName: null };

function row(id: string, sourcing: BuildWorksheetRow["sourcing"], blueprint: Partial<WorksheetBlueprintCoverage> | null): BuildWorksheetRow {
  return {
    id, typeId: id.length, typeName: id, categoryId: null, categoryName: null, groupId: null, groupName: null,
    sourcing, requiredQuantity: 1, coveredQuantity: 0, shortageQuantity: 1, coveragePercentage: "0.00",
    evidenceState: "complete", pricing: { state: "complete", classification: "default", policy: null, sourceNote: null },
    unitCost: null, totalValue: null, producerBuildId: sourcing === "manufacturing" ? `${id}-build` : null,
    retainedSurplusQuantity: null, retainedSurplusBasis: null, warnings: [],
    blueprint: blueprint && {
      blueprintTypeId: 1_000 + id.length, blueprintName: `${id} Blueprint`, holdings, producer: null, state: "notOwned", ...blueprint,
    },
  };
}

const producer = (requiredRuns: number) => ({ requiredRuns, source: "observedAsset" as const, observationId: null, materialEfficiency: 10, timeEfficiency: 20 });

const rows = [
  row("Drone Bay", "manufacturing", { producer: producer(2) }),
  row("Construction Parts", "manufacturing", { state: "insufficientRuns", holdings: { ...holdings, copies: 1, copyRuns: 2 }, producer: producer(4) }),
  row("Capacitor Battery", "buy", { state: "owned", holdings: { ...holdings, originals: 1, best: bpo } }),
  row("Shield Emitter", "buy", {}),
  row("Armor Plates", "manufacturing", { state: "owned", holdings: { ...holdings, originals: 1, best: bpo, observedAt: new Date(Date.now() - 5 * 60_000).toISOString() }, producer: producer(1) }),
  row("Tritanium", "buy", null),
];

test("splits blueprints into missing for planned builds and owned, one entry per blueprint type", () => {
  const sections = blueprintPanelSections([...rows, row("Drone Bay", "manufacturing", { producer: producer(2) })]);
  expect(sections.missing.map((entry) => entry.typeName)).toEqual(["Drone Bay", "Construction Parts"]);
  expect(sections.owned.map((entry) => entry.typeName)).toEqual(["Capacitor Battery", "Armor Plates"]);
  expect(sections.observedAt).toBe(rows[4].blueprint?.holdings.observedAt);
});

function renderPanel(panelRows = rows, onOpenProducer = vi.fn()) {
  render(
    <MemoryRouter>
      <BlueprintCoveragePanel onClose={vi.fn()} onOpenProducer={onOpenProducer} open rows={panelRows} />
    </MemoryRouter>,
  );
  return onOpenProducer;
}

test("lists the blueprints missing and the blueprints owned", () => {
  renderPanel();

  const missing = screen.getByRole("region", { name: "Missing" });
  expect(missing).toHaveTextContent("Missing (2)");
  expect(within(missing).getByText("needs 2 runs · none")).toHaveClass("text-destructive");
  expect(within(missing).getByText("needs 4 runs · owned: BPC · 2 runs")).toHaveClass("text-warning");

  const owned = screen.getByRole("region", { name: "Owned" });
  expect(owned).toHaveTextContent("Owned (2)");
  expect(within(owned).getByText("Capacitor Battery Blueprint")).toBeInTheDocument();
  expect(within(owned).getByText("Armor Plates Blueprint")).toBeInTheDocument();
  expect(screen.queryByText("Shield Emitter Blueprint")).not.toBeInTheDocument();
  expect(screen.getByText("As of 5 m ago")).toBeInTheDocument();
});

test("Copy names copies the missing blueprint names, one per line", async () => {
  const user = userEvent.setup();
  renderPanel();
  await user.click(screen.getByRole("button", { name: "Copy missing blueprint names" }));
  await expect(navigator.clipboard.readText()).resolves.toBe("Drone Bay Blueprint\nConstruction Parts Blueprint");
});

test("each entry links to the Blueprint Library and, for a Build row, its producer", async () => {
  const onOpenProducer = renderPanel();
  const missing = screen.getByRole("region", { name: "Missing" });
  expect(within(missing).getAllByRole("link", { name: "Library" })[0]).toHaveAttribute("href", "/builds/blueprints?search=Drone%20Bay%20Blueprint");
  await userEvent.click(within(missing).getAllByRole("button", { name: "Producer" })[0]);
  expect(onOpenProducer).toHaveBeenCalledWith(expect.objectContaining({ id: "Drone Bay" }));
  const bought = within(screen.getByRole("region", { name: "Owned" })).getByText("Capacitor Battery Blueprint").parentElement!;
  expect(within(bought).queryByRole("button", { name: "Producer" })).not.toBeInTheDocument();
});

test("leaves out Missing when every planned build has a blueprint", () => {
  renderPanel(rows.filter((entry) => entry.blueprint?.state === "owned"));
  expect(screen.queryByRole("region", { name: "Missing" })).not.toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Owned" })).toHaveTextContent("Owned (2)");
});

test("says so when none of the blueprints is owned", () => {
  renderPanel([row("Shield Emitter", "buy", {})]);
  expect(screen.getByText("You own none of these blueprints.")).toBeInTheDocument();
});

test("says so when no component can be built from a blueprint", () => {
  renderPanel([row("Tritanium", "buy", null)]);
  expect(screen.getByText("No components here can be built from a blueprint.")).toBeInTheDocument();
});

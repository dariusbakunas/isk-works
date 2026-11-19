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

test("sections follow the chip's terms, one entry per blueprint type", () => {
  const sections = blueprintPanelSections([...rows, row("Drone Bay", "manufacturing", { producer: producer(2) })]);
  expect(sections.missing.map((entry) => entry.typeName)).toEqual(["Drone Bay", "Construction Parts"]);
  expect(sections.ownedButBought.map((entry) => entry.typeName)).toEqual(["Capacitor Battery"]);
  expect(sections.covered.map((entry) => entry.typeName)).toEqual(["Armor Plates"]);
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

test("lists what's missing, what's bought, and keeps Covered collapsed", async () => {
  renderPanel();

  const missing = screen.getByRole("region", { name: "Missing for planned builds" });
  expect(missing).toHaveTextContent("Missing for planned builds (2)");
  expect(within(missing).getByText("needs 2 runs · none")).toHaveClass("text-destructive");
  expect(within(missing).getByText("needs 4 runs · owned: BPC · 2 runs")).toHaveClass("text-warning");
  expect(within(screen.getByRole("region", { name: "Owned but bought" })).getByText("BPO 10/20 · could build")).toBeInTheDocument();
  expect(screen.getByText("As of 5 m ago")).toBeInTheDocument();

  const covered = screen.getByRole("button", { name: /Covered \(1\)/ });
  expect(covered).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByText("Armor Plates Blueprint")).not.toBeInTheDocument();
  await userEvent.click(covered);
  expect(screen.getByText("Armor Plates Blueprint")).toBeInTheDocument();
});

test("Copy names copies the missing blueprint names, one per line", async () => {
  const user = userEvent.setup();
  renderPanel();
  await user.click(screen.getByRole("button", { name: "Copy missing blueprint names" }));
  await expect(navigator.clipboard.readText()).resolves.toBe("Drone Bay Blueprint\nConstruction Parts Blueprint");
});

test("each entry links to the Blueprint Library and, for a Build row, its producer", async () => {
  const onOpenProducer = renderPanel();
  const missing = screen.getByRole("region", { name: "Missing for planned builds" });
  expect(within(missing).getAllByRole("link", { name: "Library" })[0]).toHaveAttribute("href", "/builds/blueprints?search=Drone%20Bay%20Blueprint");
  await userEvent.click(within(missing).getAllByRole("button", { name: "Producer" })[0]);
  expect(onOpenProducer).toHaveBeenCalledWith(expect.objectContaining({ id: "Drone Bay" }));
  expect(within(screen.getByRole("region", { name: "Owned but bought" })).queryByRole("button", { name: "Producer" })).not.toBeInTheDocument();
});

test("says so when every planned build has a blueprint", () => {
  renderPanel(rows.filter((entry) => entry.blueprint?.state === "owned"));
  expect(screen.getByText("Every planned build has a blueprint.")).toBeInTheDocument();
});

test("says so when no component can be built from a blueprint", () => {
  renderPanel([row("Tritanium", "buy", null)]);
  expect(screen.getByText("No components here can be built from a blueprint.")).toBeInTheDocument();
});

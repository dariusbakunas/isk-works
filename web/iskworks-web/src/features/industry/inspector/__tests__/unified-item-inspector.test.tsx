import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { InspectorCollapseProvider } from "../inspector-collapse";
import type { InspectorActions, InspectorModel } from "../inspector-model";
import { UnifiedItemInspector } from "../unified-item-inspector";

function baseModel(overrides: Partial<InspectorModel> = {}): InspectorModel {
  return {
    identity: {
      kind: "linkedBuild",
      kindLabel: "LINKED BUILD",
      name: "Widget",
      subtitle: "T2 Component · Manufacturing",
      typeId: 900,
      showImage: true,
      summary: "Need 34 · Making 34",
    },
    warnings: [],
    ...overrides,
  };
}

function renderInspector(model: InspectorModel, actions: InspectorActions = {}) {
  return render(
    <InspectorCollapseProvider>
      <UnifiedItemInspector actions={actions} model={model} />
    </InspectorCollapseProvider>,
  );
}

describe("UnifiedItemInspector", () => {
  it("renders only the sections the model provides -- no empty sections", () => {
    renderInspector(
      baseModel({
        quantities: { metrics: [{ label: "Runs", value: "45" }], summary: "45 runs" },
        cost: { material: "1 ISK", installation: null, total: null, state: "incomplete", summary: "1 ISK materials" },
      }),
    );
    expect(screen.getByRole("region", { name: "Quantities" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Cost" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Coverage" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Pricing" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Blueprint" })).not.toBeInTheDocument();
  });

  it("keeps the header compact -- kind, name, subtitle, one summary line, no duplication", () => {
    const { container } = renderInspector(baseModel());
    const header = container.querySelector(".border-b");
    expect(header).toHaveTextContent("LINKED BUILD");
    expect(header).toHaveTextContent("Widget");
    expect(header).toHaveTextContent("T2 Component · Manufacturing");
    expect(header).toHaveTextContent("Need 34 · Making 34");
    // The name is not repeated below the header.
    expect(screen.getAllByText("Widget")).toHaveLength(1);
  });

  it("surfaces a transient status line under the header", () => {
    renderInspector(baseModel({ statusLine: { text: "Creating linked build…", tone: "neutral" } }));
    expect(screen.getByRole("status")).toHaveTextContent("Creating linked build…");
  });

  it("shows Cost as Material / Installation / Total, with unknown states explicit", () => {
    renderInspector(
      baseModel({
        cost: { material: "2.70M ISK", installation: null, total: null, state: "incomplete", summary: "x" },
      }),
    );
    const cost = screen.getByRole("region", { name: "Cost" });
    expect(within(cost).getByText("Material / Component Cost")).toBeInTheDocument();
    expect(within(cost).getByText("2.70M ISK")).toBeInTheDocument();
    expect(within(cost).getByText("Installation")).toBeInTheDocument();
    expect(within(cost).getByText("Not included")).toBeInTheDocument();
    expect(within(cost).getByText("Total Production Cost")).toBeInTheDocument();
    expect(within(cost).getByText("Incomplete")).toBeInTheDocument();
  });

  it("Cost never appears for a raw material -- Value does instead", () => {
    renderInspector(
      baseModel({
        identity: { ...baseModel().identity, kind: "buyMaterial", kindLabel: "BUY MATERIAL", showImage: false },
        value: { metrics: [{ label: "Unit price", value: "5 ISK" }] },
      }),
    );
    expect(screen.queryByRole("region", { name: "Cost" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Value" })).toBeInTheDocument();
  });

  it("collapse state is keyed by section id and survives a model swap", async () => {
    const user = userEvent.setup();
    const withBlueprint = baseModel({
      blueprint: {
        kind: "blueprint",
        name: "Widget Blueprint",
        blueprintTypeId: 900,
        mode: "manual",
        origin: "BPO",
        me: 10,
        te: 20,
        licensedRuns: null,
        notes: "",
        observations: [],
        selectedObservationId: null,
        requiredRuns: 1,
        computing: false,
        editable: false,
        summary: "BPO · ME 10 · TE 20",
      },
    });
    const { rerender } = renderInspector(withBlueprint);
    await user.click(screen.getByRole("button", { name: /Blueprint/ }));
    expect(screen.getByRole("button", { name: /Blueprint/ })).toHaveAttribute("aria-expanded", "false");

    // A different selection that still has a Blueprint section keeps it collapsed.
    rerender(
      <InspectorCollapseProvider>
        <UnifiedItemInspector
          actions={{}}
          model={baseModel({
            identity: { ...baseModel().identity, name: "Other Widget" },
            blueprint: { ...withBlueprint.blueprint!, name: "Other Blueprint" },
          })}
        />
      </InspectorCollapseProvider>,
    );
    expect(screen.getByRole("button", { name: /Blueprint/ })).toHaveAttribute("aria-expanded", "false");
  });

  it("sourcing copy is product language -- no implementation terms", () => {
    renderInspector(
      baseModel({
        sourcing: {
          mode: "build",
          buildable: true,
          recipeSummary: "Manufacturing",
          fullyCoveredByInventory: false,
          usingInventory: false,
          hasShortfall: false,
          scope: "missing",
          requiredQuantity: 34,
          missingQuantity: 0,
          availableQuantity: 0,
          fulfillmentSentence: null,
          summary: "Build",
        },
      }),
      { sourcing: { onBuy: vi.fn(), onBuild: vi.fn() } },
    );
    const sourcing = screen.getByRole("region", { name: "Sourcing" });
    expect(sourcing.textContent ?? "").not.toMatch(/mutate|owning build|root editor/i);
  });
});

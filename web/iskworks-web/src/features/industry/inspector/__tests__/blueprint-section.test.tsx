import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { BlueprintObservation } from "../../../../api/industry";
import { InspectorCollapseProvider } from "../inspector-collapse";
import type { BlueprintSlice, InspectorActions, InspectorModel } from "../inspector-model";
import { UnifiedItemInspector } from "../unified-item-inspector";

function observation(over: Partial<BlueprintObservation> = {}): BlueprintObservation {
  return {
    id: "obs-1",
    workspaceId: "ws",
    ownerId: "owner-1",
    ownerName: "Valka",
    eveItemId: 1,
    blueprintTypeId: 900,
    blueprintName: "Crystalline Carbonide Armor Plate Blueprint",
    kind: "original",
    materialEfficiency: 10,
    timeEfficiency: 20,
    licensedRuns: null,
    locationId: 60_003_760,
    locationFlag: "Hangar",
    locationName: "C-J6MT",
    observedAt: "2026-09-01T00:00:00Z",
    importedAt: "2026-09-01T00:00:00Z",
    ...over,
  };
}

function blueprintSlice(over: Partial<BlueprintSlice> = {}): BlueprintSlice {
  return {
    kind: "blueprint",
    name: "Crystalline Carbonide Armor Plate Blueprint",
    blueprintTypeId: 900,
    mode: "manual",
    origin: "BPO",
    me: 0,
    te: 0,
    licensedRuns: null,
    notes: "",
    observations: [],
    selectedObservationId: null,
    requiredRuns: 1,
    computing: false,
    editable: true,
    summary: "BPO · ME 0 · TE 0",
    ...over,
  };
}

function renderBlueprint(slice: BlueprintSlice, blueprint: InspectorActions["blueprint"]) {
  const model: InspectorModel = {
    identity: {
      kind: "linkedBuild",
      kindLabel: "LINKED BUILD",
      name: "Crystalline Carbonide Armor Plate",
      subtitle: "T2 Component · Manufacturing",
      typeId: 900,
      showImage: true,
      summary: "Need 34 · Making 34",
    },
    warnings: [],
    blueprint: slice,
  };
  return render(
    <InspectorCollapseProvider>
      <UnifiedItemInspector actions={{ blueprint }} model={model} />
    </InspectorCollapseProvider>,
  );
}

describe("Blueprint section", () => {
  it("uses the same source toggle as the Choose Blueprint dialog", () => {
    renderBlueprint(blueprintSlice(), { onSelectObservation: vi.fn(), onModelManually: vi.fn() });
    expect(screen.getByRole("button", { name: "Enter manually" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use available blueprint" })).toBeInTheDocument();
  });

  it("'use available blueprint' offers real observations in a compact picker", async () => {
    const user = userEvent.setup();
    renderBlueprint(
      blueprintSlice({
        observations: [
          observation({ id: "bpo-1" }),
          observation({
            id: "bpc-1",
            kind: "copy",
            materialEfficiency: 8,
            timeEfficiency: 16,
            licensedRuns: 12,
            locationName: "Jita",
          }),
        ],
      }),
      { onSelectObservation: vi.fn(), onModelManually: vi.fn() },
    );
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    const picker = screen.getByRole("combobox", { name: "Available blueprint" });
    expect(picker).toHaveTextContent("Choose a blueprint (2 available)");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();

    await user.click(picker);
    const options = within(screen.getByRole("listbox")).getAllByRole("option");
    expect(options).toHaveLength(2);
    // Each option is identifiable by name + kind + ME/TE + (runs) + owner + location.
    expect(within(options[0]).getByText("ME 10 · TE 20")).toBeInTheDocument();
    expect(within(options[1]).getByText("ME 8 · TE 16")).toBeInTheDocument();
    expect(within(options[1]).getByText("12 licensed runs · 1 required · 1 job")).toBeInTheDocument();
    expect(within(options[1]).getByText(/Valka · Jita · synced/)).toBeInTheDocument();
    // No generic manual ME/TE/kind controls while picking a real blueprint.
    expect(screen.queryByLabelText("Blueprint kind")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Material Efficiency (0-10)")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Time Efficiency (0-20)")).not.toBeInTheDocument();
  });

  it("choosing an option persists { mode: observedAsset, observationId } and closes the picker", async () => {
    const user = userEvent.setup();
    const onSelectObservation = vi.fn();
    renderBlueprint(blueprintSlice({ observations: [observation({ id: "obs-42" })] }), {
      onSelectObservation,
      onModelManually: vi.fn(),
    });
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    await user.click(screen.getByRole("combobox", { name: "Available blueprint" }));
    await user.click(screen.getByRole("option", { name: /Crystalline Carbonide Armor Plate Blueprint/ }));
    expect(onSelectObservation).toHaveBeenCalledWith("obs-42");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("the picker is keyboard operable", async () => {
    const user = userEvent.setup();
    const onSelectObservation = vi.fn();
    renderBlueprint(
      blueprintSlice({
        observations: [observation({ id: "bpo-1" }), observation({ id: "bpo-2", materialEfficiency: 9 })],
      }),
      { onSelectObservation, onModelManually: vi.fn() },
    );
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    const picker = screen.getByRole("combobox", { name: "Available blueprint" });
    picker.focus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    await user.keyboard("{ArrowDown}{Enter}");
    expect(onSelectObservation).toHaveBeenCalledWith("bpo-2");

    await user.click(picker);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("a copy with fewer runs than required is selectable and shows its job split", async () => {
    const user = userEvent.setup();
    const onSelectObservation = vi.fn();
    const copy = (id: string, eveItemId: number) =>
      observation({ id, eveItemId, kind: "copy", materialEfficiency: 2, timeEfficiency: 4, licensedRuns: 1 });
    renderBlueprint(
      blueprintSlice({
        requiredRuns: 4,
        observations: [copy("bpc-1", 1), copy("bpc-2", 2), copy("bpc-3", 3), copy("bpc-4", 4)],
      }),
      { onSelectObservation, onModelManually: vi.fn() },
    );
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    await user.click(screen.getByRole("combobox", { name: "Available blueprint" }));
    const [first] = screen.getAllByRole("option");
    expect(first).not.toHaveAttribute("aria-disabled");
    expect(within(first).getByText("1 run per copy · 4 jobs · 4 identical copies")).toHaveClass("text-muted");

    await user.click(first);
    expect(onSelectObservation).toHaveBeenCalledWith("bpc-1");
  });

  it("warns when fewer identical copies are observed than jobs", async () => {
    const user = userEvent.setup();
    renderBlueprint(
      blueprintSlice({
        requiredRuns: 4,
        observations: [
          observation({ id: "bpc-1", kind: "copy", materialEfficiency: 2, timeEfficiency: 4, licensedRuns: 1 }),
          observation({ id: "bpc-2", eveItemId: 2, kind: "copy", materialEfficiency: 2, timeEfficiency: 4, licensedRuns: 1 }),
          // Different ME: not identical, so it covers none of these jobs.
          observation({ id: "bpc-3", eveItemId: 3, kind: "copy", materialEfficiency: 0, timeEfficiency: 4, licensedRuns: 1 }),
        ],
      }),
      { onSelectObservation: vi.fn(), onModelManually: vi.fn() },
    );
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    await user.click(screen.getByRole("combobox", { name: "Available blueprint" }));
    const [first] = screen.getAllByRole("option");
    expect(within(first).getByText("1 run per copy · 4 jobs · 2 identical copies")).toHaveClass("text-warning");
  });

  it("a manual copy with fewer licensed runs than required shows the job split", () => {
    renderBlueprint(
      blueprintSlice({ mode: "manual", origin: "BPC", licensedRuns: 2, requiredRuns: 5 }),
      { onSelectObservation: vi.fn(), onModelManually: vi.fn() },
    );
    const region = screen.getByRole("region", { name: "Blueprint" });
    expect(within(region).getByText("5 runs plan as 3 jobs of up to 2 runs.")).toBeInTheDocument();
  });

  it("a selected observed blueprint shows its ME/TE in the picker, no editable fields", () => {
    renderBlueprint(
      blueprintSlice({
        mode: "existing",
        origin: "BPO",
        me: 10,
        te: 20,
        selectedObservationId: "obs-1",
        observations: [observation({ id: "obs-1", materialEfficiency: 10, timeEfficiency: 20 })],
      }),
      { onSelectObservation: vi.fn(), onModelManually: vi.fn() },
    );
    const region = screen.getByRole("region", { name: "Blueprint" });
    const picker = within(region).getByRole("combobox", { name: "Available blueprint" });
    expect(picker).toHaveTextContent(/Crystalline Carbonide Armor Plate Blueprint Original/);
    expect(within(picker).getByText("ME 10 · TE 20")).toBeInTheDocument();
    expect(within(region).queryByLabelText("Material Efficiency (0-10)")).not.toBeInTheDocument();
    expect(within(region).queryByLabelText("Blueprint kind")).not.toBeInTheDocument();
  });

  it("a selected BPC observation shows its licensed runs and job count", () => {
    renderBlueprint(
      blueprintSlice({
        mode: "existing",
        origin: "BPC",
        me: 8,
        te: 16,
        licensedRuns: 12,
        requiredRuns: 3,
        selectedObservationId: "bpc-1",
        observations: [
          observation({ id: "bpc-1", kind: "copy", materialEfficiency: 8, timeEfficiency: 16, licensedRuns: 12 }),
        ],
      }),
      { onSelectObservation: vi.fn(), onModelManually: vi.fn() },
    );
    const region = screen.getByRole("region", { name: "Blueprint" });
    expect(within(region).getByText("12 licensed runs · 3 required · 1 job")).toBeInTheDocument();
  });

  it("no observed blueprints shows the empty state and the manual toggle still switches", async () => {
    const user = userEvent.setup();
    renderBlueprint(blueprintSlice({ mode: "manual", observations: [] }), {
      onSelectObservation: vi.fn(),
      onModelManually: vi.fn(),
    });
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    expect(screen.getByText("No available blueprints")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Enter manually" }));
    expect(screen.getByLabelText("Blueprint kind")).toBeInTheDocument();
  });

  it("manual mode exposes editable ME/TE and persists a manual selection on blur", async () => {
    const user = userEvent.setup();
    const onModelManually = vi.fn();
    renderBlueprint(blueprintSlice({ mode: "manual", me: 0, te: 0 }), {
      onSelectObservation: vi.fn(),
      onModelManually,
    });
    expect(screen.getByRole("button", { name: "Enter manually" })).toHaveAttribute("aria-pressed", "true");
    const me = screen.getByLabelText("Material Efficiency (0-10)");
    await user.clear(me);
    await user.type(me, "10");
    await user.tab();
    expect(onModelManually).toHaveBeenCalledWith({
      kind: "original",
      materialEfficiency: 10,
      timeEfficiency: 0,
      licensedRuns: null,
      notes: "",
    });
  });

  it("manual BPC mode exposes licensed runs", async () => {
    const user = userEvent.setup();
    renderBlueprint(blueprintSlice({ mode: "manual" }), {
      onSelectObservation: vi.fn(),
      onModelManually: vi.fn(),
    });
    await user.selectOptions(screen.getByLabelText("Blueprint kind"), "copy");
    expect(await screen.findByLabelText("Licensed runs")).toBeInTheDocument();
  });

  it("a persisted observedAsset selection initialises to 'use available blueprint'", () => {
    renderBlueprint(
      blueprintSlice({
        mode: "existing",
        selectedObservationId: "obs-1",
        me: 10,
        te: 20,
        observations: [observation({ id: "obs-1" })],
      }),
      { onSelectObservation: vi.fn(), onModelManually: vi.fn() },
    );
    expect(screen.getByRole("button", { name: "Use available blueprint" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("combobox", { name: "Available blueprint" })).toHaveTextContent(/Crystalline/);
  });

  it("a persisted manual selection initialises to manual mode with its fields", () => {
    renderBlueprint(
      blueprintSlice({ mode: "manual", origin: "BPC", me: 7, te: 14, licensedRuns: 5 }),
      { onSelectObservation: vi.fn(), onModelManually: vi.fn() },
    );
    expect(screen.getByRole("button", { name: "Enter manually" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByLabelText("Blueprint kind")).toHaveValue("copy");
    expect(screen.getByLabelText("Material Efficiency (0-10)")).toHaveValue("7");
    expect(screen.getByLabelText("Time Efficiency (0-20)")).toHaveValue("14");
    expect(screen.getByLabelText("Licensed runs")).toHaveValue("5");
  });
});

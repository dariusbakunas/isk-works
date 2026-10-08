import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ReactFlowProvider, type NodeProps } from "@xyflow/react";
import { describe, expect, it, vi } from "vitest";

import type { GraphWarning } from "../../../../../api/industry";
import {
  AcquisitionGraphNode,
  ProductionGraphNode,
  RootGraphNode,
  UnresolvedBuildGraphNode,
} from "../graph-node";
import type { BuildGraphNode, BuildGraphNodeData } from "../to-react-flow";

import { ROOT_BUILD_ID, acquisitionChild, productionNode, unresolvedBuildChild } from "./fixtures";

const COMPONENT = {
  root: RootGraphNode,
  production: ProductionGraphNode,
  acquisition: AcquisitionGraphNode,
  unresolvedBuild: UnresolvedBuildGraphNode,
};

function renderNode(data: BuildGraphNodeData, extra: Record<string, unknown> = {}) {
  const Component = COMPONENT[data.nodeType];
  const props = {
    id: "n",
    data: { ...data, ...extra },
    selected: Boolean(extra.selected),
    type: data.nodeType,
    dragging: false,
    zIndex: 0,
    isConnectable: false,
    positionAbsoluteX: 0,
    positionAbsoluteY: 0,
  } as unknown as NodeProps<BuildGraphNode>;
  return render(
    <ReactFlowProvider>
      <Component {...props} />
    </ReactFlowProvider>,
  );
}

function rootData(overrides = {}): BuildGraphNodeData {
  return {
    nodeType: "root",
    depth: 0,
    node: productionNode({
      graphNodeId: `root:${ROOT_BUILD_ID}`,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      typeName: "Rifter",
      kind: "rootManufacturing",
      runs: 12,
      ...overrides,
    }),
  };
}
function productionData(overrides = {}): BuildGraphNodeData {
  return {
    nodeType: "production",
    depth: 1,
    node: productionNode({
      graphNodeId: "build:x",
      buildId: "x",
      typeId: 900,
      typeName: "Composite",
      parentBuildId: ROOT_BUILD_ID,
      parentComponentTypeId: 900,
      requiredQuantity: 100,
      netRequiredQuantity: 100,
      producingQuantity: 40,
      surplus: -60,
      costState: "incomplete",
      ...overrides,
    }),
  };
}

describe("graph node cards", () => {
  it("root: image, MANUFACTURE eyebrow, runs, cost, no collapse control", () => {
    renderNode(
      rootData({ costState: "known", estimatedCost: "1234567.0000", children: [] }),
    );
    expect(screen.getByRole("img", { name: "Rifter" })).toBeInTheDocument();
    expect(screen.getByText(/manufacture/i)).toBeInTheDocument();
    expect(screen.getByText(/12 runs/)).toBeInTheDocument();
    expect(screen.getByText(/ISK/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /dependencies/i })).not.toBeInTheDocument();
  });

  it("root reaction uses the REACT eyebrow", () => {
    renderNode(
      rootData({
        kind: "rootReaction",
        recipe: { mode: "reaction", reactionFormulaTypeId: 999 },
      }),
    );
    expect(screen.getByText(/react/i)).toBeInTheDocument();
  });

  it("root never renders inline material rows -- dependencies are child nodes", () => {
    renderNode(
      rootData({
        children: [
          acquisitionChild({ graphNodeId: "buy:root:34", parentBuildId: ROOT_BUILD_ID, typeId: 34, typeName: "Tritanium" }),
          { nodeKind: "production", ...productionNode({ graphNodeId: "c", buildId: "c", typeId: 9 }) },
        ],
      }),
    );
    // The child nodes are laid out separately by React Flow, never as rows
    // inside the root card.
    expect(screen.queryByText("Tritanium")).not.toBeInTheDocument();
    expect(screen.queryByText(/buy materials/i)).not.toBeInTheDocument();
  });

  it("production: BUILD eyebrow, Need/Making/signed shortfall, incomplete cost", () => {
    renderNode(productionData());
    expect(screen.getByText("Build")).toBeInTheDocument();
    expect(screen.getByText("Need")).toBeInTheDocument();
    expect(screen.getByText("100")).toBeInTheDocument();
    expect(screen.getByText("Making")).toBeInTheDocument();
    expect(screen.getByText("40")).toBeInTheDocument();
    expect(screen.getByText("-60")).toBeInTheDocument();
    expect(screen.getByText("Cost incomplete")).toBeInTheDocument();
  });

  it("reaction production uses the REACTION eyebrow", () => {
    renderNode(
      productionData({ kind: "reaction", recipe: { mode: "reaction", reactionFormulaTypeId: 30 } }),
    );
    expect(screen.getByText("Reaction")).toBeInTheDocument();
  });

  it("production shows a collapse control only when it has graph children", async () => {
    const user = userEvent.setup();
    const onToggleCollapse = vi.fn();

    const { unmount } = renderNode(productionData());
    expect(screen.queryByRole("button", { name: /dependencies for/i })).not.toBeInTheDocument();
    unmount();

    renderNode(productionData(), { collapsible: true, collapsed: false, onToggleCollapse });
    const btn = screen.getByRole("button", { name: "Collapse dependencies for Composite" });
    expect(btn).toHaveAttribute("aria-expanded", "true");
    await user.click(btn);
    expect(onToggleCollapse).toHaveBeenCalledTimes(1);
  });

  it("collapsed production shows the hidden-count control and aria-expanded=false", () => {
    renderNode(productionData(), { collapsible: true, collapsed: true, hiddenCount: 7 });
    const btn = screen.getByRole("button", { name: "Expand dependencies for Composite" });
    expect(btn).toHaveAttribute("aria-expanded", "false");
    expect(btn).toHaveTextContent("7 dependencies hidden");
  });

  it("shows a warning chip mapped from the projection warnings", () => {
    const warning: GraphWarning = { graphNodeId: "build:x", code: "runsDiverged", message: "m" };
    renderNode(productionData(), { warnings: [warning] });
    expect(screen.getByText("Runs diverged")).toBeInTheDocument();
  });

  it("renders a distinct recipeCurrency chip per state, and none for quiet states", () => {
    const cases: Array<[string, string | null]> = [
      ["current", null],
      ["olderSdeVersion", null],
      ["unableToCompare", null],
      ["recipeChanged", "Recipe data changed"],
      ["blueprintNoLongerAvailable", "Blueprint unavailable"],
      ["reactionFormulaNoLongerAvailable", "Formula unavailable"],
    ];
    for (const [state, label] of cases) {
      const { unmount } = renderNode(
        productionData({ recipeCurrency: state as never }),
      );
      if (label) {
        expect(screen.getByText(label)).toBeInTheDocument();
      } else {
        expect(screen.queryByText(/recipe|blueprint|formula/i)).not.toBeInTheDocument();
      }
      // No catch-all "Recipe changed" wording in any state.
      expect(screen.queryByText("Recipe changed")).not.toBeInTheDocument();
      unmount();
    }
  });

  it("actionable Buy with no inventory: quiet card, BUY tag, 'Buy N', Switch to BUILD, no image", async () => {
    const user = userEvent.setup();
    const onBuyBuild = vi.fn();
    renderNode(
      {
        nodeType: "acquisition",
        depth: 1,
        node: acquisitionChild({
          graphNodeId: "buy:root:34",
          parentBuildId: ROOT_BUILD_ID,
          typeId: 34,
          typeName: "Tritanium",
          requiredQuantity: 500,
          missingQuantity: 500,
        }) as never,
      },
      { onBuyBuild },
    );
    expect(screen.getByText("BUY")).toBeInTheDocument();
    // No inventory -> "Buy 500", never "Required" / "Inventory".
    expect(screen.getByText("500")).toBeInTheDocument();
    expect(
      screen.getByText((_, el) => el?.textContent === "Buy 500"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Required/)).not.toBeInTheDocument();
    expect(screen.queryByText("Inventory")).not.toBeInTheDocument();
    expect(screen.queryByText("Recipe available")).not.toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Build Tritanium" }));
    expect(onBuyBuild).toHaveBeenCalledTimes(1);
  });

  it("raw acquisition node: BUY tag, no Switch to BUILD button", () => {
    const onBuyBuild = vi.fn();
    renderNode(
      {
        nodeType: "acquisition",
        depth: 1,
        node: acquisitionChild({
          graphNodeId: "buy:root:34",
          parentBuildId: ROOT_BUILD_ID,
          typeId: 34,
          typeName: "Isogen",
          buildableRecipe: null,
        }) as never,
      },
      { onBuyBuild },
    );
    expect(screen.getByText("BUY")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Build Isogen/ })).not.toBeInTheDocument();
  });

  it("unresolved Build: BUILD tag, status variants, required, error message", () => {
    const base: BuildGraphNodeData = {
      nodeType: "unresolvedBuild",
      depth: 1,
      node: unresolvedBuildChild({
        graphNodeId: "buy:root:900",
        parentBuildId: ROOT_BUILD_ID,
        typeId: 900,
        typeName: "Composite",
        requiredQuantity: 40,
      }) as never,
    };

    const { unmount } = renderNode(base);
    expect(screen.getByText("Resolving linked build…")).toBeInTheDocument();
    expect(screen.getByText("40")).toBeInTheDocument();
    unmount();

    const p = renderNode(base, { pending: true });
    expect(screen.getByText("Creating linked build…")).toBeInTheDocument();
    p.unmount();

    renderNode(base, { error: "revision conflict" });
    expect(screen.getByText("Linked build creation failed")).toBeInTheDocument();
    expect(screen.getByText("revision conflict")).toBeInTheDocument();
  });

  it("each card is a keyboard-operable button: role, aria-pressed, Enter selects", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    renderNode(productionData(), { onSelect, selected: true });
    const card = screen.getByRole("button", { name: /Manufacturing Composite/ });
    expect(card).toHaveAttribute("aria-pressed", "true");
    card.focus();
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(onSelect).toHaveBeenCalledTimes(2);
  });
});

function acquisitionData(overrides: Record<string, unknown>): BuildGraphNodeData {
  return {
    nodeType: "acquisition",
    depth: 1,
    node: acquisitionChild({
      graphNodeId: "buy:root:34",
      parentBuildId: ROOT_BUILD_ID,
      typeId: 34,
      typeName: "Mexallon",
      buildableRecipe: null,
      ...overrides,
    }) as never,
  } as BuildGraphNodeData;
}

describe("inventory-aware Graph presentation", () => {
  // ── Acquisition card ──────────────────────────────────────────────────
  it("acquisition, full inventory coverage: INVENTORY, 100% covered, no BUY", () => {
    renderNode(acquisitionData({ requiredQuantity: 2250, missingQuantity: 0 }));
    expect(screen.getByText("INVENTORY")).toBeInTheDocument();
    expect(screen.queryByText("BUY")).not.toBeInTheDocument();
    expect(screen.getByText("Required")).toBeInTheDocument();
    expect(screen.getByText("2,250")).toBeInTheDocument();
    expect(screen.getByText("100% covered")).toBeInTheDocument();
  });

  it("acquisition, partial coverage: BUY, Need / Inventory / Buy split", () => {
    renderNode(acquisitionData({ requiredQuantity: 2250, missingQuantity: 1250 }));
    expect(screen.getByText("BUY")).toBeInTheDocument();
    expect(screen.queryByText("INVENTORY")).not.toBeInTheDocument();
    expect(screen.getByText("Need")).toBeInTheDocument();
    expect(screen.getByText("Inventory")).toBeInTheDocument();
    expect(screen.getByText("2,250")).toBeInTheDocument(); // Need
    expect(screen.getByText("1,000")).toBeInTheDocument(); // Inventory (2250 - 1250)
    expect(screen.getByText("1,250")).toBeInTheDocument(); // Buy
  });

  it("acquisition, no coverage: BUY, 'Buy N' only, no 'Inventory 0'", () => {
    renderNode(acquisitionData({ requiredQuantity: 2250, missingQuantity: 2250 }));
    expect(screen.getByText("BUY")).toBeInTheDocument();
    expect(screen.queryByText("Inventory")).not.toBeInTheDocument();
    expect(screen.queryByText(/Required/)).not.toBeInTheDocument();
    expect(
      screen.getByText((_, el) => el?.textContent === "Buy 2,250"),
    ).toBeInTheDocument();
  });

  it("acquisition, explicit Full scope (DTO: missing == required): stays BUY despite the number", () => {
    // A `Full`-scoped node arrives from the backend with missing === required,
    // so it can never classify as INVENTORY even though physical stock exists.
    renderNode(acquisitionData({ requiredQuantity: 2250, missingQuantity: 2250 }));
    expect(screen.getByText("BUY")).toBeInTheDocument();
    expect(screen.queryByText("INVENTORY")).not.toBeInTheDocument();
    expect(screen.queryByText("100% covered")).not.toBeInTheDocument();
  });

  it("acquisition, defensive: missing > required clamps, no negative inventory", () => {
    renderNode(acquisitionData({ requiredQuantity: 100, missingQuantity: 250 }));
    expect(screen.getByText("BUY")).toBeInTheDocument();
    expect(screen.queryByText("Inventory")).not.toBeInTheDocument();
    expect(
      screen.getByText((_, el) => el?.textContent === "Buy 100"),
    ).toBeInTheDocument();
  });

  // ── Production card ───────────────────────────────────────────────────
  it("production, full inventory coverage: INVENTORY, 100% covered, no MANUFACTURE work", () => {
    renderNode(productionData({ requiredQuantity: 100, netRequiredQuantity: 0 }));
    expect(screen.getByText("INVENTORY")).toBeInTheDocument();
    expect(screen.getByText("Required")).toBeInTheDocument();
    expect(screen.getByText("100% covered")).toBeInTheDocument();
    expect(screen.queryByText("MANUFACTURE")).not.toBeInTheDocument();
    expect(screen.queryByText("Making")).not.toBeInTheDocument();
  });

  it("production, partial coverage: MANUFACTURE, Need / Inventory / Manufacture split", () => {
    // producingQuantity 40 in the fixture; pick net = 55 so "Inventory 45" and
    // "Manufacture 55" don't collide with "Making 40".
    renderNode(productionData({ requiredQuantity: 100, netRequiredQuantity: 55 }));
    expect(screen.getByText("MANUFACTURE")).toBeInTheDocument();
    expect(screen.getByText("Need")).toBeInTheDocument();
    expect(screen.getByText("Inventory")).toBeInTheDocument();
    expect(screen.getByText("Manufacture")).toBeInTheDocument();
    expect(screen.getByText("45")).toBeInTheDocument(); // Inventory (100 - 55)
    expect(screen.getByText("55")).toBeInTheDocument(); // Manufacture
    // The existing production detail (Making / surplus) is still shown below.
    expect(screen.getByText("Making")).toBeInTheDocument();
  });

  it("reaction, partial coverage: REACT badge and 'React' verb", () => {
    renderNode(
      productionData({
        kind: "reaction",
        recipe: { mode: "reaction", reactionFormulaTypeId: 30 },
        requiredQuantity: 100,
        netRequiredQuantity: 55,
      }),
    );
    expect(screen.getByText("REACT")).toBeInTheDocument();
    expect(screen.queryByText("MANUFACTURE")).not.toBeInTheDocument();
    expect(screen.getByText("React")).toBeInTheDocument();
  });

  it("production, no coverage: unchanged normal presentation, no state badge", () => {
    renderNode(productionData({ requiredQuantity: 100, netRequiredQuantity: 100 }));
    expect(screen.queryByText("INVENTORY")).not.toBeInTheDocument();
    expect(screen.queryByText("MANUFACTURE")).not.toBeInTheDocument();
    expect(screen.getByText("Build")).toBeInTheDocument();
    expect(screen.getByText("Need")).toBeInTheDocument();
    expect(screen.getByText("Making")).toBeInTheDocument();
  });

  it("production, explicit Full scope (DTO: netRequired == required): stays a production card", () => {
    renderNode(productionData({ requiredQuantity: 100, netRequiredQuantity: 100 }));
    expect(screen.queryByText("INVENTORY")).not.toBeInTheDocument();
    expect(screen.queryByText("100% covered")).not.toBeInTheDocument();
  });
});

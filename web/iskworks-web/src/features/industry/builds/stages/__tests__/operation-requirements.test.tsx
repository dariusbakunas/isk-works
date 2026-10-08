import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { AcquisitionLine, ExecutionNode, ExecutionRequirement } from "../../../../../api/industry";
import { OperationRequirements } from "../operation-requirements";

function requirement(over: Partial<ExecutionRequirement> = {}): ExecutionRequirement {
  return {
    typeId: 34,
    typeName: "Tritanium",
    requiredQuantity: 10,
    plannedInventoryQuantity: 0,
    shortageQuantity: 10,
    fulfillmentScope: "missing",
    resolution: "buy",
    dependencyId: "dep-34",
    producerBuildId: null,
    producerNodeId: null,
    ...over,
  };
}

const rcfNode = {
  id: "rcf",
  productionDemand: 161,
} as ExecutionNode;

const tritaniumAcquisition = {
  typeId: 34,
} as AcquisitionLine;

describe("OperationRequirements", () => {
  it("navigates from a shared reaction requirement while keeping local and shared quantities distinct", () => {
    const onSelect = vi.fn();
    const { container } = render(
      <OperationRequirements
        acquisitions={[]}
        nodes={[rcfNode]}
        onSelect={onSelect}
        requirements={[
          requirement({
            typeId: 57_457,
            typeName: "Reinforced Carbon Fiber",
            requiredQuantity: 107,
            shortageQuantity: 107,
            resolution: "reaction",
            producerNodeId: "rcf",
          }),
        ]}
      />,
    );

    expect(screen.getByText("Reinforced Carbon Fiber")).toBeInTheDocument();
    expect(screen.getAllByText("107")).toHaveLength(2);
    expect(screen.getByText(/Total planned 161/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Reinforced Carbon Fiber/ }));
    expect(onSelect).toHaveBeenCalledWith({ kind: "production", nodeId: "rcf" });
    expect(container.querySelector('[class*="min-w-["]')).not.toBeInTheDocument();
  });

  it("navigates a bought shortage to its acquisition row", () => {
    const onSelect = vi.fn();
    render(
      <OperationRequirements
        acquisitions={[tritaniumAcquisition]}
        nodes={[]}
        onSelect={onSelect}
        requirements={[requirement()]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Tritanium/ }));
    expect(onSelect).toHaveBeenCalledWith({ kind: "acquisition", typeId: 34 });
    expect(screen.getByText("Buy")).toBeInTheDocument();
  });

  it("does not make inventory-covered or unresolved requirements actionable", () => {
    render(
      <OperationRequirements
        acquisitions={[]}
        nodes={[]}
        onSelect={vi.fn()}
        requirements={[
          requirement({ plannedInventoryQuantity: 10, shortageQuantity: 0 }),
          requirement({ typeId: 35, typeName: "Pyerite", resolution: "unresolved" }),
        ]}
      />,
    );

    expect(screen.queryByRole("button", { name: /Tritanium/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Pyerite/ })).not.toBeInTheDocument();
    expect(screen.getByText("Unresolved")).toBeInTheDocument();
  });

  it("shows an explicit empty state", () => {
    render(<OperationRequirements acquisitions={[]} nodes={[]} onSelect={vi.fn()} requirements={[]} />);
    expect(screen.getByText("No direct material requirements.")).toBeInTheDocument();
  });
});

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import { InspectorCollapseProvider } from "../inspector-collapse";
import { InspectorSection } from "../inspector-section";

describe("InspectorSection", () => {
  it("follows its per-section default until toggled", async () => {
    const user = userEvent.setup();
    render(
      <InspectorCollapseProvider>
        <InspectorSection defaultExpanded={false} id="cost" label="Cost" summary="29.56M ISK total">
          <p>cost body</p>
        </InspectorSection>
        <InspectorSection defaultExpanded id="coverage" label="Coverage">
          <p>coverage body</p>
        </InspectorSection>
      </InspectorCollapseProvider>,
    );

    // Collapsed by default -> body hidden, summary shown on the header.
    expect(screen.queryByText("cost body")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Cost/ })).toHaveTextContent("29.56M ISK total");
    // Expanded by default -> body shown.
    expect(screen.getByText("coverage body")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Cost/ }));
    expect(screen.getByText("cost body")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Cost/ })).toHaveAttribute("aria-expanded", "true");
  });

  it("keeps a warning visible whether the section is open or closed", async () => {
    const user = userEvent.setup();
    render(
      <InspectorCollapseProvider>
        <InspectorSection
          defaultExpanded={false}
          id="pricing"
          label="Pricing"
          warning={<span data-testid="warn">Pricing incomplete</span>}
        >
          <p>pricing body</p>
        </InspectorSection>
      </InspectorCollapseProvider>,
    );

    expect(screen.getByTestId("warn")).toBeInTheDocument();
    expect(screen.queryByText("pricing body")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Pricing/ }));
    expect(screen.getByTestId("warn")).toBeInTheDocument();
    expect(screen.getByText("pricing body")).toBeInTheDocument();
  });

  it("persists expansion state across a selection change that keeps the section", async () => {
    const user = userEvent.setup();

    function Harness() {
      const [selection, setSelection] = useState("a");
      return (
        <InspectorCollapseProvider>
          <button onClick={() => setSelection((s) => (s === "a" ? "b" : "a"))} type="button">
            switch
          </button>
          <p>selection {selection}</p>
          {/* This section applies to both selections and stays mounted. */}
          <InspectorSection defaultExpanded={false} id="facility" label="Facility">
            <p>facility body</p>
          </InspectorSection>
          {/* Only present for selection "a" -- it unmounts on switch. */}
          {selection === "a" ? (
            <InspectorSection defaultExpanded={false} id="inputs" label="Inputs">
              <p>inputs body</p>
            </InspectorSection>
          ) : null}
        </InspectorCollapseProvider>
      );
    }

    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Facility" }));
    expect(screen.getByText("facility body")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "switch" }));
    expect(screen.getByText("selection b")).toBeInTheDocument();
    // Still expanded after the selection changed.
    expect(screen.getByText("facility body")).toBeInTheDocument();
  });
});

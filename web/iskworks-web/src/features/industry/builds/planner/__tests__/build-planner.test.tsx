import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { BuildPlanner } from "../build-planner";

function renderPlanner(mode: "create" | "draft") {
  render(
    <BuildPlanner
      assumptions={<div>Blueprint controls</div>}
      commitDisabledReason={mode === "draft" ? "No assumptions have changed." : ""}
      commitLabel="Commit"
      committing={false}
      identity={<div>{mode === "create" ? "Product search" : "Rifter build"}</div>}
      mode={mode}
      notes={<div>Planning notes</div>}
      onCancel={vi.fn()}
      onCommit={vi.fn()}
      preview={<div>Candidate results</div>}
      secondaryAction={mode === "create" ? <button type="button">Save</button> : null}
    />,
  );
}

describe("BuildPlanner", () => {
  test.each([
    ["create", "Product search"],
    ["draft", "Rifter build"],
  ] as const)("renders the shared planner structure in %s mode", (mode, identity) => {
    renderPlanner(mode);

    expect(screen.getByRole("region", { name: "Build planner" })).toBeInTheDocument();
    expect(screen.getByText(identity)).toBeInTheDocument();
    expect(screen.getByText("Blueprint controls")).toBeInTheDocument();
    expect(screen.getByText("Candidate results")).toBeInTheDocument();
    expect(screen.getByText("Planning notes")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Commit" })).toBeInTheDocument();
    if (mode === "create") expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
    else expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    expect(screen.queryByText("Recipe details")).not.toBeInTheDocument();
  });

  test("exposes the disabled commit reason accessibly", () => {
    renderPlanner("draft");

    const action = screen.getByRole("button", { name: "Commit" });
    expect(action).toBeDisabled();
    expect(action).toHaveAccessibleDescription("No assumptions have changed.");
  });

  test("renders with no action bar when onCommit is omitted", () => {
    render(
      <BuildPlanner
        assumptions={<div>Blueprint controls</div>}
        identity={<div>Rifter build</div>}
        mode="draft"
        preview={<div>Candidate results</div>}
      />,
    );

    expect(screen.getByRole("region", { name: "Build planner" })).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});

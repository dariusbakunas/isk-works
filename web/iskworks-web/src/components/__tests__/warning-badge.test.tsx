import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";

import { WarningBadge } from "../warning-badge";

test("renders nothing with no warnings", () => {
  const { container } = render(<WarningBadge warnings={[]} />);
  expect(container).toBeEmptyDOMElement();
});

test("shows the count and reveals messages on hover, hides on leave", async () => {
  const user = userEvent.setup();
  render(
    <WarningBadge
      warnings={[
        "These fitted rigs don't apply to this product and contribute no bonus: Standup XL-Set.",
        "Duration includes blueprint and facility assumptions but no character skills.",
      ]}
    />,
  );

  const trigger = screen.getByRole("button", { name: "2 planning notes" });
  expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();

  await user.hover(trigger);
  const tip = screen.getByRole("tooltip");
  expect(tip).toHaveTextContent("Standup XL-Set");
  expect(tip).toHaveTextContent("no character skills");

  await user.unhover(trigger);
  expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
});

test("click toggles the popover for keyboard/touch users", async () => {
  const user = userEvent.setup();
  render(<WarningBadge warnings={["Market observations are stale."]} label="Notes" />);

  const trigger = screen.getByRole("button", { name: "1 notes" });
  await user.click(trigger);
  expect(screen.getByRole("tooltip")).toHaveTextContent("Market observations are stale.");
});

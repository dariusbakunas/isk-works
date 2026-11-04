import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import type { TemplatePin } from "../../../../../api/pi-planner";
import { ColonyTemplateDialog } from "../colony-template-dialog";
import { TEMPLATE } from "./plan-fixture";

describe("ColonyTemplateDialog", () => {
  test("draws each structure and copies the template", async () => {
    const user = userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    render(<ColonyTemplateDialog onClose={() => {}} planetName="Q-3HS5 IV" template={TEMPLATE} />);

    const dialog = screen.getByRole("dialog", { name: "Q-3HS5 IV template" });
    const pins = within(dialog).getAllByTestId("template-pin");
    expect(pins.map((pin) => pin.getAttribute("data-kind"))).toEqual(["launchpad", "factory", "extractor"]);
    expect(pins[2]).toHaveTextContent("Barren Extractor Control Unit · extracts Base Metals · 10 heads");
    expect(within(dialog).getByRole("img", { name: "Template layout" }).querySelectorAll("line")).toHaveLength(2);
    expect(dialog).toHaveTextContent("1 factory");
    expect(dialog).toHaveTextContent("95%");
    expect(within(dialog).queryByText("Over the command center's limit")).toBeNull();

    await user.click(within(dialog).getByRole("button", { name: "Copy template" }));
    expect(writeText).toHaveBeenCalledWith(TEMPLATE.json);
    expect(await within(dialog).findByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  test("warns when the colony needs more than the command center gives", () => {
    render(
      <ColonyTemplateDialog
        onClose={() => {}}
        planetName="Q-3HS5 IV"
        template={{ ...TEMPLATE, powerUsed: "1.04", fits: false }}
      />,
    );
    expect(screen.getByText("Over the command center's limit")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy template" })).toBeEnabled();
  });

  test("a factory colony with storage says to split its imports", () => {
    const pin = (kind: TemplatePin["kind"]): TemplatePin => ({ ...TEMPLATE.pins[0], kind });
    const { rerender } = render(
      <ColonyTemplateDialog
        onClose={() => {}}
        planetName="Q-3HS5 IV"
        template={{ ...TEMPLATE, pins: [pin("launchpad"), pin("factory"), pin("storage")] }}
      />,
    );
    expect(screen.getByText(/split your imports evenly/)).toBeInTheDocument();
    expect(screen.queryByText(/Drag the extractor heads/)).toBeNull();

    rerender(<ColonyTemplateDialog onClose={() => {}} planetName="Q-3HS5 IV" template={TEMPLATE} />);
    expect(screen.queryByText(/split your imports evenly/)).toBeNull();
    expect(screen.getByText(/Drag the extractor heads/)).toBeInTheDocument();
  });

  test("escape closes it", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<ColonyTemplateDialog onClose={onClose} planetName="Q-3HS5 IV" template={TEMPLATE} />);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });
});

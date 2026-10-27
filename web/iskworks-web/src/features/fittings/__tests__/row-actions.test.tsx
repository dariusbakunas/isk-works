import { fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { FIT_ID, stubFetch } from "./fixtures";
import { renderPage } from "./render";

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.restoreAllMocks());

async function openEditor() {
  stubFetch();
  renderPage(`/fittings/${FIT_ID}`);
  fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
  await screen.findByRole("complementary", { name: "Module browser" });
}

const table = () => screen.getByRole("table", { name: "Modules" });
const row = (name: RegExp) => within(table()).getByRole("row", { name });

function choose(rowName: RegExp, action: string) {
  fireEvent.click(within(row(rowName)).getByRole("button", { name: /^Actions for/ }));
  fireEvent.click(screen.getByRole("menuitem", { name: action }));
}

describe("Fittings › row actions", () => {
  test("a module can be removed", async () => {
    await openEditor();
    choose(/Damage Control II/, "Remove");
    expect(within(table()).queryByRole("row", { name: /Damage Control II/ })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Unsaved changes" })).toHaveTextContent("1 unsaved change");
  });

  test("a module can fill the rest of its rack", async () => {
    await openEditor();
    choose(/200mm AutoCannon II/, "Fill rack with this module");
    expect(within(table()).getAllByRole("row", { name: /200mm AutoCannon II/ })).toHaveLength(2);
    expect(within(table()).queryByText("Empty high slot")).not.toBeInTheDocument();
  });

  test("a charge can be unloaded", async () => {
    await openEditor();
    expect(row(/200mm AutoCannon II/)).toHaveTextContent("EMP S");
    choose(/200mm AutoCannon II/, "Unload charge");
    expect(row(/200mm AutoCannon II/)).not.toHaveTextContent("EMP S");
  });

  test("a module can be swapped for one of its variations", async () => {
    await openEditor();
    choose(/200mm AutoCannon II/, "Variations…");
    const dialog = await screen.findByRole("dialog", { name: "Variations of 200mm AutoCannon II" });
    fireEvent.click(await within(dialog).findByRole("button", { name: "Use 200mm AutoCannon I" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    const swapped = row(/200mm AutoCannon I\b/);
    expect(swapped).toHaveTextContent("EMP S");
    expect(within(table()).queryByRole("row", { name: /200mm AutoCannon II/ })).not.toBeInTheDocument();
  });

  test("a stack's quantity can be set", async () => {
    await openEditor();
    fireEvent.change(within(row(/Hobgoblin II/)).getByRole("spinbutton", { name: "Quantity of Hobgoblin II" }), {
      target: { value: "3" },
    });
    expect(row(/Hobgoblin II/)).toHaveTextContent("Hobgoblin II");
    expect(within(row(/Hobgoblin II/)).getByRole("spinbutton")).toHaveValue(3);
  });
});

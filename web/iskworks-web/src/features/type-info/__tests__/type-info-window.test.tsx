import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { TypeInfo, UsedInPage } from "../../../api/types";
import { ApiError } from "../../../api/workspace";
import { ToolWindowLayer } from "../../../components/tool-windows/tool-window-layer";
import { ToolWindowsProvider, useToolWindows } from "../../../components/tool-windows/tool-windows-context";
import { WindowDock } from "../../../components/tool-windows/window-dock";
import { TypeInfoButton } from "../../../components/type-info/type-info-button";
import { formatTrainingTime } from "../type-info-sections";
import { typeInfoKind } from "../type-info-kind";
import { visibleTabs } from "../type-info-window";

const api = vi.hoisted(() => ({
  getTypeInfo: vi.fn(),
  getTypeVariations: vi.fn(),
  getTypeUsedWith: vi.fn(),
  getTypeUsedIn: vi.fn(),
  listCharacters: vi.fn(),
}));

vi.mock("../../../api/types", async () => {
  const actual = await vi.importActual<typeof import("../../../api/types")>("../../../api/types");
  return { ...actual, ...api, listCharacters: undefined };
});
vi.mock("../../../api/characters", async () => {
  const actual = await vi.importActual<typeof import("../../../api/characters")>("../../../api/characters");
  return { ...actual, listCharacters: api.listCharacters };
});

const HARPY = 11381;
const SCRIPT = 40334;
const TRITANIUM = 34;

function info(overrides: Partial<TypeInfo>): TypeInfo {
  return {
    typeId: 1,
    name: "Thing",
    published: true,
    group: null,
    category: null,
    metaGroup: null,
    marketGroupPath: [],
    price: null,
    description: [],
    basics: { volumeM3: null, packagedVolumeM3: null, massKg: null, capacityM3: null, portionSize: null },
    attributes: [],
    traits: [],
    requirements: [],
    missing: null,
    industry: { producedBy: [], reprocessesInto: [], usedInCount: 0 },
    variationCount: 1,
    usedWith: null,
    ...overrides,
  };
}

const script = info({
  typeId: SCRIPT,
  name: "Missile Range Disruption Script",
  group: { id: 1569, name: "Guidance Disruption Script" },
  category: { id: 8, name: "Charge" },
  price: { bestSell: "20893.72", scope: { regionId: 10000002, locationId: 60003760 } as never, observedAt: null },
  description: [
    { kind: "text", text: "Load into a ", bold: false },
    { kind: "link", typeId: TRITANIUM, text: "Tritanium" },
    { kind: "text", text: " module.", bold: false },
  ],
  basics: { volumeM3: "1", packagedVolumeM3: "1", massKg: "1", capacityM3: "0", portionSize: 1 },
  attributes: [
    {
      category: "Miscellaneous",
      rows: [
        { attributeId: 128, label: "Charge size", display: "Small", raw: "1", unitId: 117, highIsGood: true, link: null },
        {
          attributeId: 137,
          label: "Used with (Launcher Group)",
          display: "Missile Guidance Computer",
          raw: "1396",
          unitId: 115,
          highIsGood: true,
          link: { kind: "group", id: 1396, name: "Missile Guidance Computer" },
        },
        { attributeId: 2023, label: "Modification of Explosion Radius Bonus", display: "-100 %", raw: "-100", unitId: 105, highIsGood: false, link: null },
      ],
    },
  ],
  usedWith: { kind: "modules", count: 7 },
  industry: {
    producedBy: [{ kind: "manufacturing", typeId: 40336, name: "Missile Range Disruption Script Blueprint" }],
    reprocessesInto: [{ typeId: 35, name: "Pyerite", quantity: 4 }],
    usedInCount: 0,
  },
});

const harpy = (missing: TypeInfo["missing"] = null) =>
  info({
    typeId: HARPY,
    name: "Harpy",
    metaGroup: { id: 2, name: "Tech II" },
    category: { id: 6, name: "Ship" },
    traits: [
      {
        heading: "Caldari Frigate bonuses (per skill level)",
        skillTypeId: 3330,
        bonuses: [{ amount: "4%", text: [{ kind: "text", text: "bonus to all shield resistances", bold: false }] }],
      },
    ],
    requirements: [
      {
        skillTypeId: 3330,
        name: "Caldari Frigate",
        level: 5,
        trainedLevel: missing ? 4 : 5,
        children: [{ skillTypeId: 3327, name: "Spaceship Command", level: 1, trainedLevel: 5, children: [] }],
      },
    ],
    missing,
    variationCount: 4,
  });

const tritanium = info({
  typeId: TRITANIUM,
  name: "Tritanium",
  group: { id: 18, name: "Mineral" },
  description: [{ kind: "text", text: "The main building block.", bold: false }],
  industry: { producedBy: [], reprocessesInto: [], usedInCount: 10 },
});

function usedInPage(offset: number, count: number, total = 10): UsedInPage {
  return {
    items: Array.from({ length: count }, (_, index) => ({
      kind: "manufacturing" as const,
      recipeTypeId: 1000 + offset + index,
      recipeName: `Thing ${offset + index} Blueprint`,
      productTypeId: 2000 + offset + index,
      productName: `Thing ${offset + index}`,
      quantity: 314,
    })),
    total,
    offset,
    limit: count,
    nextOffset: offset + count < total ? offset + count : null,
  };
}

beforeEach(() => {
  window.localStorage.clear();
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1440 });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 900 });
  for (const mock of Object.values(api)) mock.mockReset();
  api.getTypeInfo.mockImplementation(async (typeId: number, pilot?: string) => {
    if (typeId === SCRIPT) return script;
    if (typeId === TRITANIUM) return tritanium;
    if (typeId === HARPY) return pilot && pilot !== "allV" ? harpy({ missingSkills: 2, remainingSeconds: 274_000 }) : harpy();
    throw new ApiError(404, { code: "type_not_found", message: "That item isn't in the current static data." });
  });
  api.getTypeVariations.mockResolvedValue([
    { metaGroupId: 1, metaGroupName: "Tech I", items: [{ typeId: 603, name: "Merlin", metaLevel: 0 }] },
    { metaGroupId: 2, metaGroupName: "Tech II", items: [{ typeId: HARPY, name: "Harpy", metaLevel: 5 }] },
  ]);
  api.getTypeUsedWith.mockResolvedValue({
    kind: "modules",
    groups: [{ metaGroupId: 2, metaGroupName: "Tech II", items: [{ typeId: 37546, name: "Guidance Disruptor II", metaLevel: 5 }] }],
  });
  api.getTypeUsedIn.mockImplementation(async (_typeId: number, options: { limit?: number; offset?: number }) =>
    usedInPage(options.offset ?? 0, Math.min(options.limit ?? 50, 10 - (options.offset ?? 0))),
  );
  api.listCharacters.mockResolvedValue([{ connectionId: "c1", characterName: "Aura Starfall" }]);
});

function Narrow() {
  const windows = useToolWindows();
  return (
    <button onClick={() => windows?.resize(windows.windows[0].id, 360, 500)} type="button">
      Make narrow
    </button>
  );
}

function renderApp() {
  return render(
    <MemoryRouter>
      <ToolWindowsProvider kinds={[typeInfoKind]}>
        <TypeInfoButton alwaysVisible name="Missile Range Disruption Script" typeId={SCRIPT} />
        <TypeInfoButton alwaysVisible name="Harpy" typeId={HARPY} />
        <TypeInfoButton alwaysVisible name="Tritanium" typeId={TRITANIUM} />
        <TypeInfoButton alwaysVisible name="Gone" typeId={77777} />
        <Narrow />
        <WindowDock expanded />
        <ToolWindowLayer />
      </ToolWindowsProvider>
    </MemoryRouter>,
  );
}

async function openInfo(name: string) {
  renderApp();
  fireEvent.click(screen.getByRole("button", { name: `Show info: ${name}` }));
  const dialog = await screen.findByRole("dialog");
  await within(dialog).findByRole("tablist");
  return dialog;
}

describe("Type Info window", () => {
  test("the header shows the item, its classification and price; only tabs with content show", async () => {
    const dialog = await openInfo("Missile Range Disruption Script");

    expect(within(dialog).getByRole("heading", { level: 3, name: "Missile Range Disruption Script" })).toBeInTheDocument();
    expect(within(dialog).getByText("Guidance Disruption Script · Charge")).toBeInTheDocument();
    expect(within(dialog).getByText("20.9K ISK")).toHaveAttribute("title", expect.stringContaining("20,893.72"));
    expect(within(dialog).getByRole("link", { name: "Open in Market" })).toHaveAttribute(
      "href",
      "/market?typeId=40334&name=Missile+Range+Disruption+Script",
    );
    const tabs = within(dialog).getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toEqual(["Description", "Attributes", "Used with", "Industry"]);
    expect(within(dialog).getByRole("tab", { name: "Description" })).toHaveAttribute("aria-selected", "true");
  });

  test("attributes read like the client, basics first", async () => {
    const dialog = await openInfo("Missile Range Disruption Script");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Attributes" }));

    const panel = within(dialog).getByRole("tabpanel");
    expect(within(panel).getByText("Basics")).toBeInTheDocument();
    expect(within(panel).getByText("1 m³")).toBeInTheDocument();
    expect(within(panel).queryByText("Packaged volume")).not.toBeInTheDocument();
    expect(within(panel).getByText("Miscellaneous")).toBeInTheDocument();
    expect(within(panel).getByText("Small")).toBeInTheDocument();
    expect(within(panel).getByText("Missile Guidance Computer")).toHaveClass("text-primary");
    expect(within(panel).getByText("-100 %")).toBeInTheDocument();
  });

  test("a link in the text opens that item in the same window; back and forward walk the history", async () => {
    const dialog = await openInfo("Missile Range Disruption Script");
    expect(within(dialog).getByRole("button", { name: "Back" })).toBeDisabled();

    fireEvent.click(within(dialog).getByRole("button", { name: "Tritanium" }));
    expect(await screen.findByRole("dialog", { name: "Tritanium" })).toBeInTheDocument();
    expect(screen.getAllByRole("dialog")).toHaveLength(1);

    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("dialog", { name: "Missile Range Disruption Script" })).toBeInTheDocument();
    expect(within(screen.getByRole("dialog")).getByRole("button", { name: "Forward" })).toBeEnabled();
  });

  test("a shift-clicked link opens another window", async () => {
    const dialog = await openInfo("Missile Range Disruption Script");
    fireEvent.click(within(dialog).getByRole("button", { name: "Tritanium" }), { shiftKey: true });
    await waitFor(() => expect(screen.getAllByRole("dialog")).toHaveLength(2));
  });

  test("traits group bonuses under their headings", async () => {
    const dialog = await openInfo("Harpy");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Traits" }));
    expect(within(dialog).getByText("Caldari Frigate bonuses (per skill level)")).toBeInTheDocument();
    expect(within(dialog).getByText("4%")).toBeInTheDocument();
  });

  test("requirements check against the picked pilot and say what's missing", async () => {
    const dialog = await openInfo("Harpy");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Requirements" }));
    expect(within(dialog).getByText("All requirements met")).toBeInTheDocument();
    expect(within(dialog).getByText("Spaceship Command").closest("span[style]")).toHaveStyle({ paddingLeft: "14px" });

    fireEvent.click(within(dialog).getByRole("button", { name: "Pilot: All V" }));
    fireEvent.click(await within(dialog).findByRole("option", { name: "Aura Starfall" }));

    expect(await within(dialog).findByText("Missing 2 skills · 3d 4h to train")).toBeInTheDocument();
    expect(api.getTypeInfo).toHaveBeenLastCalledWith(HARPY, "c1", expect.anything());
    expect(within(dialog).getAllByLabelText("Missing")).toHaveLength(1);
  });

  test("variations highlight the item itself", async () => {
    const dialog = await openInfo("Harpy");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Variations" }));
    const current = await within(dialog).findByText("Harpy", { selector: "[aria-current]" });
    expect(current.closest("li")).toHaveClass("bg-[#0e1e2e]");
    expect(within(dialog).getByRole("button", { name: "Merlin" })).toBeInTheDocument();
  });

  test("used with lists the modules by meta group", async () => {
    const dialog = await openInfo("Missile Range Disruption Script");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Used with" }));
    expect(await within(dialog).findByRole("button", { name: "Guidance Disruptor II" })).toBeInTheDocument();
    expect(within(dialog).getByText("Tech II")).toBeInTheDocument();
  });

  test("industry pages through what the item is used in", async () => {
    const dialog = await openInfo("Tritanium");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Industry" }));

    expect(await within(dialog).findByText("Used in · 10 items")).toBeInTheDocument();
    expect(await within(dialog).findAllByText("314 / run")).toHaveLength(8);
    fireEvent.click(within(dialog).getByRole("button", { name: "Show more · 2 items" }));
    await waitFor(() => expect(within(dialog).getAllByText("314 / run")).toHaveLength(10));
    expect(within(dialog).queryByRole("button", { name: /Show more/ })).not.toBeInTheDocument();
    expect(api.getTypeUsedIn).toHaveBeenLastCalledWith(TRITANIUM, { limit: 50, offset: 8 });
  });

  test("industry shows producers and reprocessing", async () => {
    const dialog = await openInfo("Missile Range Disruption Script");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Industry" }));
    expect(within(dialog).getByRole("button", { name: "Missile Range Disruption Script Blueprint" })).toBeInTheDocument();
    expect(within(dialog).getByText("Not used as a material")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Pyerite" })).toBeInTheDocument();
  });

  test("an item the static data lacks says so", async () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Show info: Gone" }));
    expect(await screen.findByText("This item isn't in the current static data.")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Gone" })).toBeInTheDocument();
  });

  test("a narrow window keeps three tabs and puts the rest under More", async () => {
    const dialog = await openInfo("Missile Range Disruption Script");
    fireEvent.click(screen.getByRole("button", { name: "Make narrow" }));
    expect(within(dialog).getAllByRole("tab")).toHaveLength(3);
    fireEvent.click(within(dialog).getByRole("button", { name: "More" }));
    fireEvent.click(within(dialog).getByRole("menuitem", { name: "Industry" }));
    // The hidden tab, once picked, names the More button.
    expect(within(dialog).getByRole("button", { name: "Industry" })).toHaveAttribute("aria-haspopup", "menu");
    expect(within(dialog).getByText("Not used as a material")).toBeInTheDocument();
  });

  test("minimised, the dock names the item", async () => {
    await openInfo("Harpy");
    fireEvent.click(screen.getByRole("button", { name: "Minimise Harpy" }));
    expect(screen.getByRole("button", { name: "Restore Harpy" })).toBeInTheDocument();
  });

  test("the window is titled once loaded even when opened without a name", async () => {
    render(
      <MemoryRouter>
        <ToolWindowsProvider kinds={[typeInfoKind]}>
          <OpenUnnamed />
          <ToolWindowLayer />
        </ToolWindowsProvider>
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open unnamed" }));
    expect(screen.getByRole("dialog", { name: "Type 34" })).toBeInTheDocument();
    expect(await screen.findByRole("dialog", { name: "Tritanium" })).toBeInTheDocument();
  });
});

function OpenUnnamed() {
  const windows = useToolWindows();
  return (
    <button onClick={() => windows?.open("type-info", { history: [{ typeId: TRITANIUM }], index: 0 })} type="button">
      Open unnamed
    </button>
  );
}

test("visible tabs follow the content", () => {
  expect(visibleTabs(info({}))).toEqual([]);
  expect(visibleTabs(harpy())).toEqual(["Traits", "Requirements", "Variations"]);
});

test("training time reads in days and hours, or hours and minutes", () => {
  expect(formatTrainingTime(274_000)).toBe("3d 4h");
  expect(formatTrainingTime(18_720)).toBe("5h 12m");
  expect(formatTrainingTime(2_400)).toBe("40m");
  expect(formatTrainingTime(5)).toBe("1m");
  act(() => undefined);
});

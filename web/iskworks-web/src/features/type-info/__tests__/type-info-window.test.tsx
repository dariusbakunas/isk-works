import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { SkillNode, TypeInfo, UsedInPage } from "../../../api/types";
import { ApiError } from "../../../api/workspace";
import { ToolWindowLayer } from "../../../components/tool-windows/tool-window-layer";
import { ToolWindowsProvider, useToolWindows } from "../../../components/tool-windows/tool-windows-context";
import { WindowDock } from "../../../components/tool-windows/window-dock";
import { TypeInfoButton } from "../../../components/type-info/type-info-button";
import { formatActivityTime } from "../blueprint-section";
import { formatTrainingTime, hasBasics } from "../type-info-sections";
import { typeInfoKind } from "../type-info-kind";
import { planBuildPath, visibleTabs } from "../type-info-window";

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
const TRISTAN_BLUEPRINT = 954;
const NEMESIS_BLUEPRINT = 11173;

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
    blueprint: null,
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

const skill = (skillTypeId: number, name: string, level: number, trainedLevel: number, children: SkillNode[] = []): SkillNode => ({
  skillTypeId,
  name,
  level,
  trainedLevel,
  children,
});

/** The Tristan Blueprint: no time research or reaction here, so those stay unavailable. */
const tristanBlueprint = (trained: boolean, missing: TypeInfo["missing"] = null) =>
  info({
    typeId: TRISTAN_BLUEPRINT,
    name: "Tristan Blueprint",
    group: { id: 105, name: "Frigate Blueprint" },
    category: { id: 9, name: "Blueprint" },
    description: [{ kind: "text", text: "Tristan Blueprint", bold: false }],
    blueprint: {
      maxRuns: 30,
      activities: [
        {
          kind: "manufacturing",
          timeSeconds: 6000,
          outcome: [{ typeId: 593, name: "Tristan", quantity: 1, probability: null }],
          skills: [skill(3380, "Industry", 1, trained ? 5 : 0)],
          missing,
          materials: [
            {
              name: "Mineral",
              items: [
                { typeId: TRITANIUM, name: "Tritanium", quantity: 35200 },
                { typeId: 35, name: "Pyerite", quantity: 6600 },
              ],
            },
          ],
        },
        { kind: "researchMaterial", timeSeconds: 2100, outcome: [], skills: [], missing, materials: [] },
        { kind: "copying", timeSeconds: 4800, outcome: [], skills: [], missing, materials: [] },
        {
          kind: "invention",
          timeSeconds: 63900,
          outcome: [{ typeId: NEMESIS_BLUEPRINT, name: "Nemesis Blueprint", quantity: 1, probability: "0.3" }],
          skills: [
            skill(11450, "Gallente Starship Engineering", 1, trained ? 1 : 0, [skill(3403, "Mechanics", 5, 5)]),
            skill(23121, "Gallente Encryption Methods", 1, 1),
          ],
          missing,
          materials: [{ name: "Datacores", items: [{ typeId: 20410, name: "Datacore - Gallentean Starship Engineering", quantity: 2 }] }],
        },
      ],
    },
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
    // Aura (c1) lacks one skill for the Tristan Blueprint; Mira (c2) has them all.
    if (typeId === TRISTAN_BLUEPRINT) {
      if (pilot === "c1") return tristanBlueprint(false, { missingSkills: 1, remainingSeconds: 3600 });
      if (pilot === "c2") return tristanBlueprint(true, { missingSkills: 0, remainingSeconds: 0 });
      return tristanBlueprint(true);
    }
    if (typeId === NEMESIS_BLUEPRINT) return info({ typeId: NEMESIS_BLUEPRINT, name: "Nemesis Blueprint", description: [{ kind: "text", text: "T2", bold: false }] });
    // Aura (c1) lacks two skills; Mira (c2) has them all.
    if (typeId === HARPY) {
      if (pilot === "c1") return harpy({ missingSkills: 2, remainingSeconds: 274_000 });
      if (pilot === "c2") return harpy({ missingSkills: 0, remainingSeconds: 0 });
      return harpy();
    }
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
  api.listCharacters.mockResolvedValue([
    { connectionId: "c3", characterName: "Scout", totalSp: null },
    { connectionId: "c1", characterName: "Aura Starfall", totalSp: 1_000_000 },
    { connectionId: "c2", characterName: "Mira", totalSp: 50_000_000 },
  ]);
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
        <TypeInfoButton alwaysVisible name="Tristan Blueprint" typeId={TRISTAN_BLUEPRINT} />
        <span data-testid="owned-copy">
          <TypeInfoButton
            alwaysVisible
            blueprint={{ kind: "copy", runs: 26, materialEfficiency: 10, timeEfficiency: 20 }}
            name="Tristan Blueprint"
            typeId={TRISTAN_BLUEPRINT}
          />
        </span>
        <Narrow />
        <WindowDock expanded />
        <ToolWindowLayer />
      </ToolWindowsProvider>
    </MemoryRouter>,
  );
}

async function openInfo(name: string) {
  renderApp();
  fireEvent.click(screen.getAllByRole("button", { name: `Show info: ${name}` })[0]);
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
    // Made from a blueprint: Plan a build starts one.
    expect(within(dialog).getByRole("link", { name: "Plan a build" })).toHaveAttribute(
      "href",
      "/builds/new?blueprintTypeId=40336",
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

  test("requirements check against the first character with synced skills, and say what's missing", async () => {
    const dialog = await openInfo("Harpy");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Requirements" }));

    expect(await within(dialog).findByText("Missing 2 skills · 3d 4h to train")).toBeInTheDocument();
    expect(api.getTypeInfo).toHaveBeenLastCalledWith(HARPY, "c1", expect.anything());
    expect(within(dialog).getByText("Spaceship Command").closest("span[style]")).toHaveStyle({ paddingLeft: "14px" });
    expect(within(dialog).getAllByLabelText("Missing")).toHaveLength(1);

    // Only characters: no All V / All 0, and none without synced skills.
    fireEvent.click(within(dialog).getByRole("button", { name: "Pilot: Aura Starfall" }));
    const options = within(dialog).getAllByRole("option").map((option) => option.textContent);
    expect(options).toEqual(["Aura Starfall", "Mira"]);

    fireEvent.click(within(dialog).getByRole("option", { name: "Mira" }));
    expect(await within(dialog).findByText("All requirements met")).toBeInTheDocument();
    expect(api.getTypeInfo).toHaveBeenLastCalledWith(HARPY, "c2", expect.anything());
  });

  test("without a character to check against, requirements list the levels unchecked", async () => {
    api.listCharacters.mockResolvedValue([{ connectionId: "c3", characterName: "Scout", totalSp: null }]);
    const dialog = await openInfo("Harpy");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Requirements" }));

    expect(await within(dialog).findByText("No character with synced skills to check against")).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /^Pilot:/ })).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Met")).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Missing")).not.toBeInTheDocument();
    expect(within(dialog).getByText("Caldari Frigate")).toBeInTheDocument();
    expect(within(dialog).getByRole("img", { name: "Requires level 5" })).toBeInTheDocument();
    expect(api.getTypeInfo).toHaveBeenLastCalledWith(HARPY, undefined, expect.anything());
  });

  test("a window saved with All V or All 0 picks a character instead", async () => {
    window.localStorage.setItem(
      "iskworks:tool-windows",
      JSON.stringify({
        windows: [{ id: "w1", kind: "type-info", state: { history: [{ typeId: HARPY, name: "Harpy" }], index: 0, tab: "Requirements", pilot: "allZero" }, x: 100, y: 100, w: 480, h: 560, minimized: false }],
        nextId: 2,
      }),
    );
    renderApp();
    const dialog = await screen.findByRole("dialog", { name: "Harpy" });
    expect(await within(dialog).findByText("Missing 2 skills · 3d 4h to train")).toBeInTheDocument();
    expect(api.getTypeInfo).not.toHaveBeenCalledWith(HARPY, "allZero", expect.anything());
  });

  test("variations highlight the item itself", async () => {
    const dialog = await openInfo("Harpy");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Variations" }));
    const current = await within(dialog).findByText("Harpy", { selector: "[aria-current]" });
    expect(current.closest("li")).toHaveClass("bg-[#0e1e2e]");
    expect(within(dialog).getByRole("button", { name: "Merlin" })).toBeInTheDocument();
  });

  test("a tab whose fetch fails says why and can retry", async () => {
    api.getTypeVariations.mockRejectedValueOnce(
      new ApiError(503, { code: "persistence_unavailable", message: "Storage is unavailable." }),
    );
    const dialog = await openInfo("Harpy");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Variations" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Storage is unavailable.");
    fireEvent.click(within(dialog).getByRole("button", { name: "Retry" }));
    expect(await within(dialog).findByRole("button", { name: "Merlin" })).toBeInTheDocument();
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(api.getTypeVariations).toHaveBeenCalledTimes(2);
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

  test("another item's used-in list never shows while the next item's loads", async () => {
    const usedInScript = { ...script, industry: { ...script.industry, usedInCount: 3 } };
    const reprocessesIntoScript = info({
      ...tritanium,
      industry: { ...tritanium.industry, reprocessesInto: [{ typeId: SCRIPT, name: script.name, quantity: 1 }] },
    });
    api.getTypeInfo.mockImplementation(async (typeId: number) => (typeId === SCRIPT ? usedInScript : reprocessesIntoScript));
    const tritaniumUsedIn = api.getTypeUsedIn.getMockImplementation()!;
    // The script's own used-in page never arrives.
    api.getTypeUsedIn.mockImplementation((typeId: number, options: { limit?: number; offset?: number }) =>
      typeId === SCRIPT ? new Promise(() => {}) : tritaniumUsedIn(typeId, options),
    );
    const dialog = await openInfo("Tritanium");
    fireEvent.click(within(dialog).getByRole("tab", { name: "Industry" }));
    expect(await within(dialog).findAllByText("314 / run")).toHaveLength(8);

    fireEvent.click(within(dialog).getByRole("button", { name: script.name }));
    // A new item opens on its first tab; go back to Industry.
    fireEvent.click(await within(dialog).findByRole("tab", { name: "Industry" }));
    await waitFor(() => expect(api.getTypeUsedIn).toHaveBeenLastCalledWith(SCRIPT, { limit: 8 }, expect.anything()));
    expect(within(dialog).queryByText("314 / run")).not.toBeInTheDocument();
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

  test("the header and tabs stay put; only the tab content scrolls", async () => {
    const dialog = await openInfo("Harpy");
    const panel = within(dialog).getByRole("tabpanel");

    expect(panel).toHaveClass("overflow-y-auto");
    expect(panel).not.toContainElement(within(dialog).getByRole("tablist"));
    expect(panel).not.toContainElement(within(dialog).getByRole("heading", { level: 3, name: "Harpy" }));
    expect(dialog.querySelector(".overflow-y-auto")).toBe(panel);
  });

  test("the item's image shows in the header, not the title bar; minimised, the dock shows it", async () => {
    const dialog = await openInfo("Harpy");
    expect(within(dialog).getByTestId("tool-window-title-bar").querySelector("img")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Minimise Harpy" }));
    expect(screen.getByRole("button", { name: "Restore Harpy" }).querySelector("img")).not.toBeNull();
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

test("windows stay open across page navigation", async () => {
  const { Link, Route, Routes } = await import("react-router");
  window.localStorage.clear();
  render(
    <MemoryRouter initialEntries={["/one"]}>
      <ToolWindowsProvider kinds={[typeInfoKind]}>
        <Routes>
          <Route element={<><TypeInfoButton alwaysVisible name="Tritanium" typeId={TRITANIUM} /><Link to="/two">Page two</Link></>} path="/one" />
          <Route element={<p>Second page</p>} path="/two" />
        </Routes>
        <ToolWindowLayer />
      </ToolWindowsProvider>
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Show info: Tritanium" }));
  await screen.findByRole("dialog", { name: "Tritanium" });
  fireEvent.click(screen.getByRole("link", { name: "Page two" }));
  expect(screen.getByText("Second page")).toBeInTheDocument();
  expect(screen.getByRole("dialog", { name: "Tritanium" })).toBeInTheDocument();
});

test("a reaction product plans a reaction build; an item nothing makes has no Plan a build", async () => {
  api.getTypeInfo.mockImplementation(async (typeId: number) =>
    typeId === TRITANIUM
      ? tritanium
      : info({
          typeId: 30306,
          name: "Methanofullerene",
          description: [{ kind: "text", text: "A composite.", bold: false }],
          industry: {
            producedBy: [{ kind: "reaction", typeId: 46157, name: "Methanofullerene Reaction Formula" }],
            reprocessesInto: [],
            usedInCount: 0,
          },
        }),
  );
  render(
    <MemoryRouter>
      <ToolWindowsProvider kinds={[typeInfoKind]}>
        <TypeInfoButton alwaysVisible name="Methanofullerene" typeId={30306} />
        <TypeInfoButton alwaysVisible name="Tritanium" typeId={TRITANIUM} />
        <ToolWindowLayer />
      </ToolWindowsProvider>
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Show info: Methanofullerene" }));
  expect(await screen.findByRole("link", { name: "Plan a build" })).toHaveAttribute(
    "href",
    "/builds/new?reactionFormulaTypeId=46157",
  );
  fireEvent.click(screen.getByRole("button", { name: "Show info: Tritanium" }));
  expect(await screen.findByRole("dialog", { name: "Tritanium" })).toBeInTheDocument();
  await within(screen.getByRole("dialog")).findByRole("tablist");
  expect(screen.queryByRole("link", { name: "Plan a build" })).not.toBeInTheDocument();
});

test("basics keep tiny volumes readable and show the reprocessing batch", async () => {
  api.getTypeInfo.mockResolvedValue(
    info({
      typeId: 230,
      name: "Antimatter Charge S",
      basics: { volumeM3: "0.0025", packagedVolumeM3: "0.0025", massKg: "0.01", capacityM3: null, portionSize: 100 },
    }),
  );
  render(
    <MemoryRouter>
      <ToolWindowsProvider kinds={[typeInfoKind]}>
        <TypeInfoButton alwaysVisible name="Antimatter Charge S" typeId={230} />
        <ToolWindowLayer />
      </ToolWindowsProvider>
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Show info: Antimatter Charge S" }));
  const dialog = await screen.findByRole("dialog", { name: "Antimatter Charge S" });
  const panel = await within(dialog).findByRole("tabpanel");
  // The client: "Volume 0.0025 m3", "Units to Reprocess 100".
  expect(within(panel).getByText("0.0025 m³")).toBeInTheDocument();
  expect(within(panel).getByText("Units to reprocess")).toBeInTheDocument();
  expect(within(panel).getByText("100")).toBeInTheDocument();
});

test("a portion of one isn't worth a row", () => {
  expect(hasBasics(info({ basics: { volumeM3: null, packagedVolumeM3: null, massKg: null, capacityM3: null, portionSize: 1 } }))).toBe(false);
  expect(hasBasics(info({ basics: { volumeM3: null, packagedVolumeM3: null, massKg: null, capacityM3: null, portionSize: 100 } }))).toBe(true);
});

describe("a blueprint's Type Info", () => {
  test("it opens on Industry with the game's activities; the ones it lacks are unavailable", async () => {
    const dialog = await openInfo("Tristan Blueprint");

    expect(within(dialog).getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Industry", "Description"]);
    expect(within(dialog).getByRole("tab", { name: "Industry" })).toHaveAttribute("aria-selected", "true");
    const switcher = within(dialog).getByRole("group", { name: "Activity" });
    expect(within(switcher).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
      "Manufacturing",
      "Material efficiency research",
      "Time efficiency research",
      "Copying",
      "Invention",
      "Reactions",
    ]);
    expect(within(switcher).getByRole("button", { name: "Manufacturing" })).toHaveAttribute("aria-pressed", "true");
    expect(within(switcher).getByRole("button", { name: "Time efficiency research" })).toBeDisabled();
    expect(within(switcher).getByRole("button", { name: "Reactions" })).toBeDisabled();
    expect(within(switcher).getByRole("button", { name: "Invention" })).toBeEnabled();
    // Opened by type alone: nothing is known about a particular blueprint.
    expect(within(dialog).queryByText(/runs remaining|infinite runs/)).not.toBeInTheDocument();
  });

  test("manufacturing lists time, outcome, skills and materials by group", async () => {
    const dialog = await openInfo("Tristan Blueprint");
    const panel = within(dialog).getByRole("tabpanel");

    expect(within(panel).getByRole("heading", { level: 4, name: "Manufacturing" })).toBeInTheDocument();
    expect(within(panel).getByText("Time per run").nextSibling).toHaveTextContent("01:40:00");
    expect(within(panel).getByRole("button", { name: "Tristan" })).toBeInTheDocument();
    expect(within(panel).getByText("1 / run")).toBeInTheDocument();
    expect(within(panel).getByText("Required skills [1]")).toBeInTheDocument();
    expect(within(panel).getByText("Required input materials [2]")).toBeInTheDocument();
    const minerals = within(panel).getByRole("region", { name: "Mineral" });
    expect(within(minerals).getByText("Mineral [2]")).toBeInTheDocument();
    expect(within(minerals).getAllByRole("listitem").map((item) => item.textContent)).toEqual(["35,200 xTritanium", "6,600 xPyerite"]);
    // The blueprint makes the Tristan: Plan a build starts from the blueprint itself.
    expect(within(dialog).getByRole("link", { name: "Plan a build" })).toHaveAttribute("href", "/builds/new?blueprintTypeId=954");
  });

  test("research and copying say what they yield, and nothing where nothing is needed", async () => {
    const dialog = await openInfo("Tristan Blueprint");
    const panel = within(dialog).getByRole("tabpanel");

    fireEvent.click(within(panel).getByRole("button", { name: "Material efficiency research" }));
    expect(within(panel).getByText("Time per run").nextSibling).toHaveTextContent("00:35:00");
    expect(within(panel).getByText("Blueprint Material Efficiency", { exact: false })).toHaveTextContent("+1% Blueprint Material Efficiency");
    expect(within(panel).getByText("Required skills [0]")).toBeInTheDocument();
    expect(within(panel).getAllByText("None")).toHaveLength(2);

    fireEvent.click(within(panel).getByRole("button", { name: "Copying" }));
    expect(within(panel).getByText("Time per run").nextSibling).toHaveTextContent("01:20:00");
    expect(within(panel).getByText("Max runs per copy").nextSibling).toHaveTextContent("30");
    // The outcome is the blueprint itself: named, not a link to where you are.
    expect(within(panel).getByText("Tristan Blueprint")).toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: "Tristan Blueprint" })).not.toBeInTheDocument();
  });

  test("invention shows the chance and what it invents; the outcome opens in the window", async () => {
    const dialog = await openInfo("Tristan Blueprint");
    const panel = within(dialog).getByRole("tabpanel");
    fireEvent.click(within(panel).getByRole("button", { name: "Invention" }));

    expect(within(panel).getByText("Time per run").nextSibling).toHaveTextContent("17:45:00");
    expect(within(panel).getByText("Success probability").nextSibling).toHaveTextContent("30.0%");
    expect(within(panel).getByText("Required skills [2]")).toBeInTheDocument();
    expect(within(panel).getByText("Mechanics").closest("span[style]")).toHaveStyle({ paddingLeft: "14px" });
    expect(within(panel).getByRole("region", { name: "Datacores" })).toHaveTextContent("2 xDatacore - Gallentean Starship Engineering");

    fireEvent.click(within(panel).getByRole("button", { name: "Nemesis Blueprint" }));
    expect(await screen.findByRole("heading", { level: 3, name: "Nemesis Blueprint" })).toBeInTheDocument();
    // Back on the Tristan Blueprint, the activity starts over.
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("heading", { level: 4, name: "Manufacturing" })).toBeInTheDocument();
  });

  test("activity skills check against the picked character", async () => {
    const dialog = await openInfo("Tristan Blueprint");

    expect(await within(dialog).findByText("Missing 1 skill · 1h 0m to train")).toBeInTheDocument();
    expect(api.getTypeInfo).toHaveBeenLastCalledWith(TRISTAN_BLUEPRINT, "c1", expect.anything());
    expect(within(dialog).getAllByLabelText("Missing")).toHaveLength(1);

    fireEvent.click(within(dialog).getByRole("button", { name: "Pilot: Aura Starfall" }));
    fireEvent.click(within(dialog).getByRole("option", { name: "Mira" }));
    expect(await within(dialog).findByText("All requirements met")).toBeInTheDocument();
  });

  test("opened from an owned copy, the header says so with its runs and research", async () => {
    renderApp();
    fireEvent.click(within(screen.getByTestId("owned-copy")).getByRole("button"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByText("Copy · 26 runs remaining")).toBeInTheDocument();
    expect(within(dialog).getByTitle("Material efficiency")).toHaveTextContent("ME 10%");
    expect(within(dialog).getByTitle("Time efficiency")).toHaveTextContent("TE 20%");
  });
});

test("a blueprint without activities (an older import) reads like any other item", () => {
  const plain = info({ description: [{ kind: "text", text: "x", bold: false }], industry: { producedBy: [], reprocessesInto: [], usedInCount: 2 } });
  expect(visibleTabs(plain)).toEqual(["Description", "Industry"]);
  expect(visibleTabs({ ...plain, blueprint: { maxRuns: null, activities: [] } })).toEqual(["Description", "Industry"]);
});

test("a reaction formula plans a reaction build of what it makes", () => {
  const formula = info({
    typeId: 46207,
    blueprint: { maxRuns: null, activities: [{ kind: "reaction", timeSeconds: 10800, outcome: [], skills: [], missing: null, materials: [] }] },
  });
  expect(planBuildPath(formula)).toBe("/builds/new?reactionFormulaTypeId=46207");
});

test("activity time reads like the game's clock, with days past one", () => {
  expect(formatActivityTime(6000)).toBe("01:40:00");
  expect(formatActivityTime(63_900)).toBe("17:45:00");
  expect(formatActivityTime(90_061)).toBe("1d 01:01:01");
});

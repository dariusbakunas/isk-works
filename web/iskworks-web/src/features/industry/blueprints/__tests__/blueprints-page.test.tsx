import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  getBlueprintLibraryFacets,
  listBlueprintLibrary,
  type BlueprintLibraryPage,
  type BlueprintLibraryRow,
} from "../../../../api/industry";
import { BlueprintsPage } from "../blueprints-page";
import { WithToolWindows } from "../../../../test/tool-windows";
import { BlueprintInspector } from "../blueprint-inspector";

vi.mock("../../../../api/industry", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../../api/industry")>(),
  getBlueprintLibraryFacets: vi.fn(),
  listBlueprintLibrary: vi.fn(),
}));

function row(overrides: Partial<BlueprintLibraryRow>): BlueprintLibraryRow {
  return {
    observationId: "obs-rifter-bpo",
    eveItemId: 1,
    blueprintTypeId: 691,
    blueprintName: "Rifter Blueprint",
    activity: "manufacturing",
    productTypeId: 587,
    productName: "Rifter",
    productGroupId: 25,
    productGroupName: "Frigate",
    productCategoryId: 6,
    productCategoryName: "Ship",
    kind: "original",
    materialEfficiency: 10,
    timeEfficiency: 20,
    licensedRuns: null,
    connectionId: "0b8f8d6e-2f43-4c1a-9a55-5d0f3c6f9a11",
    characterName: "Valka",
    locationId: 60003760,
    locationName: "Jita IV - Moon 4",
    locationFlag: "Hangar",
    observedAt: "2026-10-18T12:00:00Z",
    usedByBuilds: [],
    ...overrides,
  };
}

const rows = [
  row({}),
  row({ observationId: "obs-rifter-bpc", eveItemId: 2, kind: "copy", materialEfficiency: 8, licensedRuns: 5, usedByBuilds: [{ buildId: "build-1", name: "Rifter run" }] }),
  row({
    observationId: "obs-carbonide",
    eveItemId: 3,
    blueprintTypeId: 46166,
    blueprintName: "Crystalline Carbonide Reaction Formula",
    activity: "reaction",
    productTypeId: 16670,
    productName: "Crystalline Carbonide",
    productGroupId: 429,
    productGroupName: "Composite",
    productCategoryId: 4,
    productCategoryName: "Material",
    materialEfficiency: 0,
    timeEfficiency: 0,
  }),
  row({ observationId: "obs-mystery", eveItemId: 4, blueprintName: "Mystery Blueprint", productTypeId: null, productName: null, productGroupId: null, productGroupName: null, productCategoryId: null, productCategoryName: null }),
];

function page(pageRows: BlueprintLibraryRow[], total = pageRows.length): BlueprintLibraryPage {
  return { rows: pageRows, total, summary: { originals: 3, copies: 1, characters: 1, latestObservedAt: "2026-10-18T12:00:00Z" } };
}

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{`${location.pathname}${location.search}`}</output>;
}

function renderPage(entry = "/builds/blueprints", wrapper?: typeof WithToolWindows) {
  return render(
    <MemoryRouter initialEntries={[entry]}>
      <Routes>
        <Route path="/builds/blueprints" element={<BlueprintsPage />} />
        <Route path="*" element={null} />
      </Routes>
      <LocationProbe />
    </MemoryRouter>,
    { wrapper },
  );
}

beforeEach(() => {
  vi.mocked(listBlueprintLibrary).mockReset().mockResolvedValue(page(rows));
  vi.mocked(getBlueprintLibraryFacets).mockReset().mockResolvedValue({
    categories: [{ value: "4", label: "Material", count: 1 }, { value: "6", label: "Ship", count: 3 }],
    groups: [
      { value: "25", label: "Frigate", count: 3, categoryId: 6 },
      { value: "429", label: "Composite", count: 1, categoryId: 4 },
    ],
    characters: [{ value: "0b8f8d6e-2f43-4c1a-9a55-5d0f3c6f9a11", label: "Valka", count: 4 }],
    locations: [{ value: "60003760", label: "Jita IV - Moon 4", count: 4 }],
  });
});

describe("Blueprints page", () => {
  test("groups blueprints by product category with Unknown last", async () => {
    renderPage();

    const table = await screen.findByRole("table", { name: "Owned blueprints" });
    const groups = within(table).getAllByRole("button", { name: /^Collapse / }).map((button) => button.getAttribute("aria-label"));
    expect(groups).toEqual(["Collapse Material", "Collapse Ship", "Collapse Unknown"]);
    const copy = within(table).getAllByRole("row").find((element) => element.textContent?.includes("Used by 1 build"));
    expect(copy).toHaveTextContent("BPC");
    expect(copy).toHaveTextContent("5");
    expect(screen.getByText("4 of 4 blueprints")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Blueprint summary" })).toHaveTextContent("Originals3");
    expect(listBlueprintLibrary).toHaveBeenCalledWith(expect.objectContaining({ limit: 500, offset: 0, sort: "blueprint", order: "asc" }));
  });

  test("originals show unlimited runs", async () => {
    vi.mocked(listBlueprintLibrary).mockResolvedValue(page([rows[0]]));
    renderPage();

    expect(await screen.findByTitle("Unlimited runs")).toHaveTextContent("∞");
    expect(screen.getByText("BPO")).toBeInTheDocument();
  });

  test("reaction formulas show Reaction, no ME/TE and unlimited runs even when ESI gave no kind", async () => {
    vi.mocked(listBlueprintLibrary).mockResolvedValue(page([{ ...rows[2], kind: "unknown" }]));
    renderPage();

    const table = await screen.findByRole("table", { name: "Owned blueprints" });
    const formula = within(table).getAllByRole("row").find((element) => element.textContent?.includes("Reaction Formula"))!;
    const cells = within(formula).getAllByRole("cell").map((cell) => cell.textContent);
    expect(cells.slice(2, 6)).toEqual(["Reaction", "—", "—", "∞"]);
    expect(within(formula).queryByText("?")).not.toBeInTheDocument();
  });

  test("an unknown-kind blueprint is a never-used original: BPO with unlimited runs", async () => {
    vi.mocked(listBlueprintLibrary).mockResolvedValue(page([{ ...rows[0], kind: "unknown", materialEfficiency: 0, timeEfficiency: 0 }]));
    renderPage();

    const table = await screen.findByRole("table", { name: "Owned blueprints" });
    const fresh = within(table).getAllByRole("row").find((element) => element.textContent?.includes("Rifter Blueprint"))!;
    expect(within(fresh).getAllByRole("cell").map((cell) => cell.textContent).slice(2, 6)).toEqual(["BPO", "0", "0", "∞"]);
  });

  test("filters write the URL and reach the API", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Owned blueprints" });

    await user.click(screen.getByRole("checkbox", { name: "Copies" }));
    await user.click(screen.getByRole("checkbox", { name: /Ship/ }));
    await user.click(screen.getByRole("checkbox", { name: /Needs research/ }));

    await waitFor(() => expect(listBlueprintLibrary).toHaveBeenLastCalledWith(expect.objectContaining({
      kinds: ["copy"],
      categoryIds: [6],
      needsResearch: true,
    })));
    expect(screen.getByTestId("location")).toHaveTextContent("kinds=copy");
    expect(screen.getByTestId("location")).toHaveTextContent("categoryIds=6");
  });

  test("choosing a category narrows the group list to it", async () => {
    const user = userEvent.setup();
    renderPage("/builds/blueprints?categoryIds=6");
    await screen.findByRole("table", { name: "Owned blueprints" });

    await user.click(screen.getByRole("button", { name: "Product group filters" }));

    expect(screen.getByRole("checkbox", { name: /Frigate/ })).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /Composite/ })).not.toBeInTheDocument();
  });

  test("sorting a column toggles direction through the URL", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Owned blueprints" });

    await user.click(screen.getByRole("button", { name: /Runs/ }));
    await waitFor(() => expect(listBlueprintLibrary).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "runs", order: "desc" })));
  });

  test("the inspector links Plan build to this exact blueprint and lists its builds", async () => {
    const user = userEvent.setup();
    renderPage();
    const table = await screen.findByRole("table", { name: "Owned blueprints" });

    const copy = within(table).getAllByRole("row").find((element) => element.textContent?.includes("Used by 1 build"));
    await user.click(copy!);

    const inspector = screen.getByRole("complementary", { name: "Selected blueprint" });
    expect(within(inspector).getByRole("link", { name: "Plan build" })).toHaveAttribute(
      "href",
      "/builds/new?blueprintTypeId=691&observationId=obs-rifter-bpc",
    );
    expect(within(inspector).getByRole("link", { name: "Rifter run" })).toHaveAttribute("href", "/builds/build-1");
  });

  test("reaction formulas get no Plan build link", async () => {
    const user = userEvent.setup();
    renderPage("/builds/blueprints?groupBy=none");
    await user.click(await screen.findByText("Crystalline Carbonide Reaction Formula"));

    const inspector = screen.getByRole("complementary", { name: "Selected blueprint" });
    expect(within(inspector).queryByRole("link", { name: "Plan build" })).not.toBeInTheDocument();
  });

  test("an empty workspace points at Characters", async () => {
    vi.mocked(listBlueprintLibrary).mockResolvedValue({ rows: [], total: 0, summary: { originals: 0, copies: 0, characters: 0, latestObservedAt: null } });
    renderPage();

    expect(await screen.findByText(/No synced blueprints yet/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Characters" })).toHaveAttribute("href", "/characters");
  });

  test("grouped views load every page, so no group is partial", async () => {
    vi.mocked(listBlueprintLibrary)
      .mockResolvedValueOnce(page(rows.slice(0, 2), 4))
      .mockResolvedValueOnce(page(rows.slice(2), 4));
    renderPage();

    expect(await screen.findByText("Mystery Blueprint")).toBeInTheDocument();
    expect(listBlueprintLibrary).toHaveBeenLastCalledWith(expect.objectContaining({ offset: 2 }));
    expect(screen.getByText("4 of 4 blueprints")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
  });

  test("Load more fetches the next page when ungrouped", async () => {
    const user = userEvent.setup();
    vi.mocked(listBlueprintLibrary)
      .mockResolvedValueOnce(page(rows.slice(0, 2), 4))
      .mockResolvedValueOnce(page(rows.slice(2), 4));
    renderPage("/builds/blueprints?groupBy=none");

    await user.click(await screen.findByRole("button", { name: "Load more" }));

    expect(listBlueprintLibrary).toHaveBeenLastCalledWith(expect.objectContaining({ offset: 2 }));
    expect(await screen.findByText("Mystery Blueprint")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
  });
});

test("items open in Type Info from the table", async () => {
  window.localStorage.clear();
  renderPage("/builds/blueprints", WithToolWindows);
  const table = await screen.findByRole("table", { name: "Owned blueprints" });
  const buttons = await within(table).findAllByRole("button", { name: /^Show info: / });
  const name = buttons[0].getAttribute("aria-label")!.replace("Show info: ", "");
  fireEvent.click(buttons[0]);
  expect(screen.getByRole("dialog", { name })).toBeInTheDocument();
});

test("the inspector opens the blueprint and its product in Type Info", () => {
  window.localStorage.clear();
  render(<BlueprintInspector blueprint={row({})} onClose={() => {}} />, {
    wrapper: ({ children }) => (
      <MemoryRouter>
        <WithToolWindows>{children}</WithToolWindows>
      </MemoryRouter>
    ),
  });
  const inspector = screen.getByRole("complementary");
  fireEvent.click(within(inspector).getByRole("button", { name: "Show info: Rifter Blueprint" }));
  expect(screen.getByRole("dialog", { name: "Rifter Blueprint" })).toHaveTextContent("Type 691");
  fireEvent.click(within(inspector).getByRole("button", { name: "Show info: Rifter" }));
  expect(screen.getByRole("dialog", { name: "Rifter" })).toHaveTextContent("Type 587");
});

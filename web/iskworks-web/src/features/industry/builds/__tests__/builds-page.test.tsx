import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { Build } from "../../../../api/industry";
import { BuildsPage } from "../builds-page";

const industryApi = vi.hoisted(() => ({
  listBuilds: vi.fn(),
  deleteBuild: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

type ManufacturingRecipe = Extract<Build["recipe"], { kind: "manufacturing" }>;

function mfgRecipe(overrides: Partial<ManufacturingRecipe> = {}): ManufacturingRecipe {
  return {
    kind: "manufacturing",
    sourceSdeDatasetId: "dataset-1",
    sourceSdeVersion: "3389399",
    blueprintTypeId: 17_739,
    blueprintName: "Machariel Blueprint",
    durationSecondsPerRun: 18_000,
    materials: [],
    products: [{ typeId: 17_738, typeName: "Machariel", quantityPerRun: 1, sortOrder: 0 }],
    fingerprint: "recipe",
    ...overrides,
  };
}

function build(overrides: Partial<Build> = {}): Build {
  return {
    id: "build-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    name: "Machariel build",
    recipe: mfgRecipe(),
    runs: 1,
    notes: "",
    revision: 1,
    createdAt: "2026-08-01T00:00:00Z",
    updatedAt: "2026-08-01T00:00:00Z",
    draftPlanning: null,
    recipeCurrency: "current",
    activeSdeVersion: "3389399",
    productCategoryName: "Ship",
    productGroupName: "Battleship",
    selectedBlueprintOrigin: "original",
    hasOwnedBlueprint: false,
    ...overrides,
  };
}

function LocationProbe() {
  return <div data-testid="location">{useLocation().pathname}</div>;
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/builds"]}>
      <Routes>
        <Route
          element={
            <>
              <BuildsPage />
              <LocationProbe />
            </>
          }
          path="/builds"
        />
        <Route element={<LocationProbe />} path="/builds/new" />
        <Route element={<LocationProbe />} path="/builds/:buildId" />
      </Routes>
    </MemoryRouter>,
  );
}

async function openCardMenu(user: ReturnType<typeof userEvent.setup>, buildName: string) {
  await user.click(screen.getByRole("button", { name: `${buildName} actions` }));
}

describe("BuildsPage library grid", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("renders each Build as a card with a blueprint visual and overflow menu, not list rows", async () => {
    industryApi.listBuilds.mockResolvedValue([
      build(),
      build({ id: "build-2", name: "Rifter build", recipe: mfgRecipe({ blueprintTypeId: 587, blueprintName: "Rifter Blueprint", products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, sortOrder: 0 }] }) }),
    ]);
    renderPage();

    await screen.findByText("Machariel build");
    expect(screen.getByRole("link", { name: "Open Machariel build" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Rifter build" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Machariel Blueprint" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /actions$/ })).toHaveLength(2);
  });

  test("clicking a card opens the correct Build", async () => {
    industryApi.listBuilds.mockResolvedValue([build({ id: "abc-123" })]);
    renderPage();

    const link = await screen.findByRole("link", { name: "Open Machariel build" });
    expect(link).toHaveAttribute?.("href", "/builds/abc-123");
    expect(link).toHaveAttribute("href", "/builds/abc-123");
  });

  test("blueprint visual uses the selected blueprint art (bp) for an original", async () => {
    industryApi.listBuilds.mockResolvedValue([build({ selectedBlueprintOrigin: "original" })]);
    renderPage();

    const image = await screen.findByRole("img", { name: "Machariel Blueprint" });
    expect(image).toHaveAttribute("src", expect.stringContaining("/types/17739/bp"));
    expect(image).toHaveAttribute("src", expect.not.stringContaining("/bpc"));
  });

  test("a BPC build renders the copy art regardless of output category", async () => {
    industryApi.listBuilds.mockResolvedValue([
      build({ selectedBlueprintOrigin: "copy", productCategoryName: "Ship" }),
    ]);
    renderPage();

    const image = await screen.findByRole("img", { name: "Machariel Blueprint" });
    expect(image).toHaveAttribute("src", expect.stringContaining("/types/17739/bpc"));
  });

  test("a reaction Build renders the formula's blueprint-style art (bp), not an icon variation", async () => {
    industryApi.listBuilds.mockResolvedValue([
      build({
        name: "Fullerides build",
        selectedBlueprintOrigin: null,
        recipe: {
          kind: "reaction",
          sourceSdeDatasetId: "dataset-1",
          sourceSdeVersion: "3389399",
          reactionFormulaTypeId: 46_209,
          reactionFormulaName: "Fullerides Reaction Formula",
          durationSecondsPerRun: 10_800,
          materials: [],
          products: [{ typeId: 16_679, typeName: "Fullerides", quantityPerRun: 200, sortOrder: 0 }],
          fingerprint: "reaction",
        },
      }),
    ]);
    renderPage();

    const image = await screen.findByRole("img", { name: "Fullerides Reaction Formula" });
    expect(image).toHaveAttribute("src", expect.stringContaining("/types/46209/bp"));
  });

  test("no SDE / recipe-currency warning is rendered, even for a non-current build", async () => {
    industryApi.listBuilds.mockResolvedValue([
      build({
        recipeCurrency: "olderSdeVersion",
        activeSdeVersion: "20241105",
        recipe: mfgRecipe({ sourceSdeVersion: "20240812" }),
      }),
    ]);
    renderPage();

    await screen.findByText("Machariel build");
    expect(screen.queryByText(/Recipe outdated/)).not.toBeInTheDocument();
    expect(screen.queryByText(/built with SDE/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Recipe status/)).not.toBeInTheDocument();
  });

  test("flags only a manufacturing Build whose blueprint is not on hand", async () => {
    industryApi.listBuilds.mockResolvedValue([
      build({ id: "owned", name: "Owned build", hasOwnedBlueprint: true }),
      build({ id: "missing", name: "Missing build", hasOwnedBlueprint: false }),
      build({
        id: "reaction",
        name: "Reaction build",
        hasOwnedBlueprint: false,
        recipe: {
          kind: "reaction",
          sourceSdeDatasetId: "d",
          sourceSdeVersion: "1",
          reactionFormulaTypeId: 46_209,
          reactionFormulaName: "Fullerides Reaction Formula",
          durationSecondsPerRun: 1,
          materials: [],
          products: [{ typeId: 16_679, typeName: "Fullerides", quantityPerRun: 1, sortOrder: 0 }],
          fingerprint: "r",
        },
      }),
    ]);
    renderPage();

    await screen.findByText("Missing build");
    const flags = screen.getAllByText("Blueprint not available");
    expect(flags).toHaveLength(1);
    expect(flags[0].closest("[title]")).toHaveAttribute("title", "Blueprint not available");
  });
});

describe("BuildsPage filtering, sorting and search", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function threeBuilds() {
    return [
      build({ id: "a", name: "Cerberus build", updatedAt: "2026-08-03T00:00:00Z", productCategoryName: "Ship", recipe: mfgRecipe({ products: [{ typeId: 1, typeName: "Cerberus", quantityPerRun: 1, sortOrder: 0 }] }) }),
      build({ id: "b", name: "Ammo batch", updatedAt: "2026-08-05T00:00:00Z", productCategoryName: "Charge", hasOwnedBlueprint: true, recipe: mfgRecipe({ products: [{ typeId: 2, typeName: "Scourge Fury", quantityPerRun: 1, sortOrder: 0 }] }) }),
      build({ id: "c", name: "Bomber build", updatedAt: "2026-08-01T00:00:00Z", productCategoryName: "Ship", recipe: mfgRecipe({ products: [{ typeId: 3, typeName: "Purifier", quantityPerRun: 1, sortOrder: 0 }] }) }),
    ];
  }

  test("search matches Build name", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue(threeBuilds());
    renderPage();

    await screen.findByText("Cerberus build");
    await user.type(screen.getByRole("searchbox", { name: "Search builds" }), "bomber");

    await waitFor(() => expect(screen.queryByText("Cerberus build")).not.toBeInTheDocument());
    expect(screen.getByText("Bomber build")).toBeInTheDocument();
    expect(screen.getByTestId("build-library-count")).toHaveTextContent("1 of 3");
  });

  test("search matches output/item name", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue(threeBuilds());
    renderPage();

    await screen.findByText("Ammo batch");
    await user.type(screen.getByRole("searchbox", { name: "Search builds" }), "scourge");

    await waitFor(() => expect(screen.queryByText("Cerberus build")).not.toBeInTheDocument());
    expect(screen.getByText("Ammo batch")).toBeInTheDocument();
  });

  test("category filter narrows to the chosen output category", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue(threeBuilds());
    renderPage();

    await screen.findByText("Cerberus build");
    await user.click(screen.getByRole("button", { name: "Category" }));
    await user.click(await screen.findByRole("menuitem", { name: "Charge" }));

    await waitFor(() => expect(screen.queryByText("Cerberus build")).not.toBeInTheDocument());
    expect(screen.getByText("Ammo batch")).toBeInTheDocument();
    expect(screen.queryByText("Bomber build")).not.toBeInTheDocument();
  });

  test("blueprint filter shows only builds whose blueprint is on hand", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue(threeBuilds());
    renderPage();

    await screen.findByText("Cerberus build");
    await user.click(screen.getByRole("button", { name: "Blueprint" }));
    await user.click(await screen.findByRole("menuitem", { name: "On hand" }));

    await waitFor(() => expect(screen.queryByText("Cerberus build")).not.toBeInTheDocument());
    expect(screen.getByText("Ammo batch")).toBeInTheDocument();
    expect(screen.getByTestId("build-library-count")).toHaveTextContent("1 of 3");
  });

  test("sorts by most recently updated by default and alphabetically by name on request", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue(threeBuilds());
    renderPage();

    await screen.findByText("Cerberus build");
    // Card links are labelled by product (not the user's free-form build
    // name, which is masked from replay), so order is checked via product.
    const orderByUpdated = screen.getAllByRole("link", { name: /^Open / }).map((node) => node.getAttribute("aria-label"));
    expect(orderByUpdated).toEqual(["Open Scourge Fury build", "Open Cerberus build", "Open Purifier build"]);

    await user.click(screen.getByRole("button", { name: "Sort: Recently updated" }));
    await user.click(await screen.findByRole("menuitem", { name: "Name" }));

    await waitFor(() => {
      const orderByName = screen.getAllByRole("link", { name: /^Open / }).map((node) => node.getAttribute("aria-label"));
      // Build names sort Ammo batch < Bomber build < Cerberus build.
      expect(orderByName).toEqual(["Open Scourge Fury build", "Open Purifier build", "Open Cerberus build"]);
    });
  });

  test("clear filters restores the whole library and the count", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue(threeBuilds());
    renderPage();

    await screen.findByText("Cerberus build");
    await user.type(screen.getByRole("searchbox", { name: "Search builds" }), "bomber");
    await waitFor(() => expect(screen.getByTestId("build-library-count")).toHaveTextContent("1 of 3"));

    await user.click(screen.getByRole("button", { name: "Clear filters" }));

    await waitFor(() => expect(screen.getByText("Cerberus build")).toBeInTheDocument());
    expect(screen.getByTestId("build-library-count")).toHaveTextContent("3 builds");
  });
});

describe("BuildsPage empty states", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("shows first-run guidance when no Builds exist", async () => {
    industryApi.listBuilds.mockResolvedValue([]);
    renderPage();

    expect(await screen.findByText("No builds yet")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Create your first build" })).toBeInTheDocument();
    expect(screen.queryByText("No builds match these filters")).not.toBeInTheDocument();
  });

  test("shows a distinct filtered-empty state with active-filter context", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue([build()]);
    renderPage();

    await screen.findByText("Machariel build");
    await user.type(screen.getByRole("searchbox", { name: "Search builds" }), "no-such-build");

    expect(await screen.findByText("No builds match these filters")).toBeInTheDocument();
    expect(screen.getByText('Search: "no-such-build"')).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear all filters" })).toBeInTheDocument();
    expect(screen.queryByText("No builds yet")).not.toBeInTheDocument();
  });
});

describe("BuildsPage overflow menu and delete", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("opening the overflow menu and choosing Delete does not navigate", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue([build()]);
    renderPage();

    await screen.findByText("Machariel build");
    expect(screen.getByTestId("location")).toHaveTextContent("/builds");

    await openCardMenu(user, "Machariel build");
    await user.click(await screen.findByRole("menuitem", { name: "Delete build" }));

    expect(screen.getByTestId("location")).toHaveTextContent("/builds");
    expect(await screen.findByRole("dialog", { name: "Delete this Build?" })).toBeInTheDocument();
  });

  test("deletes a Build after confirming and removes it from the grid", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValueOnce([build()]).mockResolvedValueOnce([]);
    industryApi.deleteBuild.mockResolvedValue(undefined);
    renderPage();

    await screen.findByText("Machariel build");
    await openCardMenu(user, "Machariel build");
    await user.click(await screen.findByRole("menuitem", { name: "Delete build" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this Build?" });
    expect(dialog).toHaveTextContent("This removes the Build and its production plan");
    expect(dialog).toHaveTextContent("Tickets and Epics created from it will remain");
    await user.click(within(dialog).getByRole("button", { name: "Delete Build" }));

    await waitFor(() => expect(industryApi.deleteBuild).toHaveBeenCalledWith("build-1", 1));
    await waitFor(() => expect(screen.queryByText("Machariel build")).not.toBeInTheDocument());
  });

  test("cancelling the confirm dialog keeps the Build", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue([build()]);
    renderPage();

    await screen.findByText("Machariel build");
    await openCardMenu(user, "Machariel build");
    await user.click(await screen.findByRole("menuitem", { name: "Delete build" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this Build?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(industryApi.deleteBuild).not.toHaveBeenCalled();
    expect(screen.getByText("Machariel build")).toBeInTheDocument();
  });

  test("shows an error and keeps the Build listed when deletion fails", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValue([build()]);
    industryApi.deleteBuild.mockRejectedValue(new Error("Build revision has changed."));
    renderPage();

    await screen.findByText("Machariel build");
    await openCardMenu(user, "Machariel build");
    await user.click(await screen.findByRole("menuitem", { name: "Delete build" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this Build?" });
    await user.click(within(dialog).getByRole("button", { name: "Delete Build" }));

    expect(await screen.findByText("Delete failed")).toBeInTheDocument();
    expect(screen.getByText("Machariel build")).toBeInTheDocument();
  });

  test("deletion sends one non-destructive request and never offers a dependent teardown", async () => {
    const user = userEvent.setup();
    industryApi.listBuilds.mockResolvedValueOnce([build()]).mockResolvedValueOnce([]);
    industryApi.deleteBuild.mockResolvedValue(undefined);
    renderPage();

    await screen.findByText("Machariel build");
    await openCardMenu(user, "Machariel build");
    await user.click(await screen.findByRole("menuitem", { name: "Delete build" }));
    await user.click(
      within(await screen.findByRole("dialog", { name: "Delete this Build?" })).getByRole("button", {
        name: "Delete Build",
      }),
    );

    await waitFor(() => expect(industryApi.deleteBuild).toHaveBeenCalledTimes(1));
    expect(industryApi.deleteBuild).toHaveBeenCalledWith("build-1", 1);
    expect(screen.queryByRole("button", { name: "Delete Build and dependents" })).not.toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText("Machariel build")).not.toBeInTheDocument());
  });
});

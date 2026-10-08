import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { MarketCategoryNode, MarketItemPage, MarketScope } from "../../../../api/industry";
import { MarketCategoryTree } from "../market-category-tree";

const api = vi.hoisted(() => ({
  listMarketItems: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return {
    ...actual,
    listMarketItems: api.listMarketItems,
  };
});

const categories: MarketCategoryNode[] = [
  {
    marketGroupId: 4,
    name: "Ships",
    itemCount: 5,
    children: [{ marketGroupId: 1361, name: "Frigates", itemCount: 5, children: [] }],
  },
  { marketGroupId: 9, name: "Modules", itemCount: 0, children: [] },
  // A leaf with no subcategories but a non-zero rolled-up count -- a
  // second genuinely expandable node, for tests that need two
  // independent item-loading nodes.
  { marketGroupId: 27, name: "Implants", itemCount: 2, children: [] },
];

const scope: MarketScope = { regionId: 10_000_002, locationId: 60_003_760 };

function emptyPage(): MarketItemPage {
  return { rows: [], totalCount: 0, page: 1, pageSize: 200 };
}

function pageWith(items: { typeId: number; typeName: string }[], totalCount = items.length): MarketItemPage {
  return {
    rows: items.map((item) => ({
      typeId: item.typeId,
      typeName: item.typeName,
      bestSell: null,
      bestBuy: null,
      spread: null,
      sellOrderCount: 0,
      buyOrderCount: 0,
      observedAt: null,
    })),
    totalCount,
    page: 1,
    pageSize: 200,
  };
}

function renderTree(overrides: Partial<Parameters<typeof MarketCategoryTree>[0]> = {}) {
  const onSelect = vi.fn();
  const onSelectItem = vi.fn();
  const utils = render(
    <MarketCategoryTree
      categories={categories}
      onSelect={onSelect}
      onSelectItem={onSelectItem}
      scope={scope}
      selectedMarketGroupId={null}
      selectedTypeId={null}
      {...overrides}
    />,
  );
  return { ...utils, onSelect, onSelectItem };
}

describe("MarketCategoryTree", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.listMarketItems.mockResolvedValue(emptyPage());
  });

  test("every node starts collapsed -- a child is not in the document until its parent is expanded", () => {
    renderTree();

    expect(screen.getByRole("button", { name: "Ships 5" })).toBeInTheDocument();
    expect(screen.queryByText("Frigates")).not.toBeInTheDocument();
  });

  test("expanding a category reveals its children", async () => {
    const user = userEvent.setup();
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));

    expect(screen.getByRole("button", { name: "Frigates 5" })).toBeInTheDocument();
  });

  test("a category with no items shows no count", () => {
    renderTree();

    expect(screen.getByRole("button", { name: "Modules" })).toBeInTheDocument();
  });

  test("selecting a category calls onSelect with its market group id", async () => {
    const user = userEvent.setup();
    const { onSelect } = renderTree();

    await user.click(screen.getByRole("button", { name: "Ships 5" }));

    expect(onSelect).toHaveBeenCalledWith(4);
  });

  test("expanding a category lazy-loads its direct items via listMarketItems", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockResolvedValueOnce(pageWith([{ typeId: 587, typeName: "Rifter" }]));
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));

    expect(api.listMarketItems).toHaveBeenCalledWith(
      expect.objectContaining({
        marketGroupId: 4,
        regionId: 10_000_002,
        locationId: 60_003_760,
        page: 1,
        pageSize: 200,
      }),
    );
    expect(await screen.findByRole("button", { name: /Rifter/ })).toBeInTheDocument();
  });

  test("direct items render as leaf rows, distinct from category rows", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockResolvedValueOnce(pageWith([{ typeId: 587, typeName: "Rifter" }]));
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));
    const rifter = await screen.findByRole("button", { name: /Rifter/ });

    // A leaf item row has no expand/collapse affordance, unlike a category row.
    expect(rifter).not.toHaveAttribute("aria-expanded");
  });

  test("collapsing and re-expanding a category does not issue another items request", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockResolvedValueOnce(pageWith([{ typeId: 587, typeName: "Rifter" }]));
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));
    await screen.findByRole("button", { name: /Rifter/ });
    await user.click(screen.getByRole("button", { name: "Collapse Ships" }));
    expect(screen.queryByRole("button", { name: /Rifter/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));

    expect(await screen.findByRole("button", { name: /Rifter/ })).toBeInTheDocument();
    expect(api.listMarketItems).toHaveBeenCalledTimes(1);
  });

  test("expanding another category independently loads and caches its own items", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockImplementation((query: { marketGroupId?: number }) => {
      if (query.marketGroupId === 4) return Promise.resolve(pageWith([{ typeId: 587, typeName: "Rifter" }]));
      if (query.marketGroupId === 27) return Promise.resolve(pageWith([{ typeId: 999, typeName: "Damage Control" }]));
      return Promise.resolve(emptyPage());
    });
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));
    await screen.findByRole("button", { name: /Rifter/ });
    await user.click(screen.getByRole("button", { name: "Expand Implants" }));
    await screen.findByRole("button", { name: /Damage Control/ });

    // Both nodes' own items are present and distinct -- neither response
    // leaked into the other node's cache entry.
    expect(screen.getByRole("button", { name: /Rifter/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Damage Control/ })).toBeInTheDocument();
    expect(api.listMarketItems).toHaveBeenCalledTimes(2);
  });

  test("a category with zero direct items renders normally with no fake empty/error row", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockResolvedValueOnce(emptyPage());
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));
    await waitFor(() => expect(api.listMarketItems).toHaveBeenCalled());

    expect(screen.queryByText(/no items/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/error/i)).not.toBeInTheDocument();
  });

  test("a failed items fetch shows a localized error for that node only, without breaking the rest of the tree", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockRejectedValueOnce(new Error("boom"));
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));
    await screen.findByText("ISK Works could not complete the request.");

    // The rest of the tree still works -- a different category can still
    // be expanded and populated, unaffected by Ships' own failure.
    api.listMarketItems.mockResolvedValueOnce(pageWith([{ typeId: 999, typeName: "Damage Control" }]));
    await user.click(screen.getByRole("button", { name: "Expand Implants" }));

    expect(await screen.findByRole("button", { name: /Damage Control/ })).toBeInTheDocument();
  });

  test("clicking an item leaf calls onSelectItem with its type id and name", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockResolvedValueOnce(pageWith([{ typeId: 587, typeName: "Rifter" }]));
    const { onSelectItem } = renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));
    await user.click(await screen.findByRole("button", { name: /Rifter/ }));

    expect(onSelectItem).toHaveBeenCalledWith(587, "Rifter");
  });

  test("a response with more items than were returned shows an indication that more exist", async () => {
    const user = userEvent.setup();
    api.listMarketItems.mockResolvedValueOnce(pageWith([{ typeId: 587, typeName: "Rifter" }], 339));
    renderTree();

    await user.click(screen.getByRole("button", { name: "Expand Ships" }));

    expect(await screen.findByText("+338 more")).toBeInTheDocument();
  });
});

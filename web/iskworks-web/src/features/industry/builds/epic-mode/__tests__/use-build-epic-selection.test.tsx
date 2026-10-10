import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OrderSummary } from "../../../../../api/industry/orders";
import { EpicSelector } from "../epic-selector";
import { rememberEpic } from "../epic-url-state";
import { isSelectableEpic, useBuildEpicSelection } from "../use-build-epic-selection";

vi.mock("../../../../../api/industry/orders", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../../api/industry/orders")>()),
  listOrders: vi.fn(),
}));

import { listOrders } from "../../../../../api/industry/orders";

function epic(id: string, overrides: Partial<OrderSummary> = {}): OrderSummary {
  return {
    id,
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    sourceBuildId: "build-1",
    sourceBuildRevision: 1,
    displayName: `Manufacture Muninn ${id}`,
    runs: 4,
    recipeFingerprint: "fp",
    priceSnapshotId: "snapshot-1",
    estimatedMaterialCost: "0.0000",
    expectedRevenue: null,
    estimatedMargin: null,
    missingPriceCount: 0,
    createdAt: "2026-10-08T10:00:00Z",
    updatedAt: "2026-10-08T10:00:00Z",
    startedAt: null,
    completedAt: null,
    canceledAt: null,
    archivedAt: null,
    planningSnapshotVersion: 3,
    status: "blocked",
    rollup: { satisfied: 0, needsAction: 1, inProgress: 0, total: 1 },
    ...overrides,
  } as OrderSummary;
}

function Harness() {
  const selection = useBuildEpicSelection("build-1");
  const location = useLocation();
  return (
    <>
      <EpicSelector
        disabled={selection.loading}
        epics={selection.epics}
        onSelect={(epicId) => selection.selectEpic(epicId, (params) => {
          if (epicId) params.set("view", "plan");
        })}
        selectedEpicId={selection.selectedEpicId}
      />
      <output data-testid="search">{location.search}</output>
    </>
  );
}

function renderAt(search: string) {
  render(
    <MemoryRouter initialEntries={[`/builds/build-1${search}`]}>
      <Harness />
    </MemoryRouter>,
  );
}

describe("useBuildEpicSelection", () => {
  beforeEach(() => {
    vi.mocked(listOrders).mockResolvedValue([
      epic("epic-a", { createdAt: "2026-10-08T10:00:00Z" }),
      epic("epic-b", { createdAt: "2026-10-09T10:00:00Z" }),
      epic("other-build", { sourceBuildId: "build-2" }),
      epic("canceled", { canceledAt: "2026-10-08T11:00:00Z" }),
      epic("completed", { completedAt: "2026-10-08T11:00:00Z" }),
      epic("legacy", { planningSnapshotVersion: 2 }),
    ]);
  });
  afterEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  it("lists only this Build's open version-3 Epics", async () => {
    renderAt("");
    const select = screen.getByRole("combobox", { name: "Epic" });
    await waitFor(() => expect(select).toBeEnabled());
    const options = Array.from((select as HTMLSelectElement).options).map((option) => option.value);
    expect(options).toEqual(["", "epic-a", "epic-b"]);
    expect(select).toHaveValue("");
  });

  it("shows the URL's Epic", async () => {
    renderAt("?view=plan&epic=epic-b");
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Epic" })).toHaveValue("epic-b"));
  });

  it("drops a closed Epic from the URL and shows free stock", async () => {
    rememberEpic("build-1", "epic-a");
    renderAt("?view=plan&epic=canceled");
    await waitFor(() => expect(screen.getByTestId("search")).toHaveTextContent(/^\?view=plan$/));
    expect(screen.getByRole("combobox", { name: "Epic" })).toHaveValue("");
  });

  it("restores the remembered Epic into the URL", async () => {
    rememberEpic("build-1", "epic-a");
    renderAt("?view=plan");
    await waitFor(() => expect(screen.getByTestId("search")).toHaveTextContent("epic=epic-a"));
    expect(screen.getByRole("combobox", { name: "Epic" })).toHaveValue("epic-a");
  });

  it("selecting an Epic remembers it and opens the Plan tab", async () => {
    const user = userEvent.setup();
    renderAt("");
    const select = screen.getByRole("combobox", { name: "Epic" });
    await waitFor(() => expect(select).toBeEnabled());

    await user.selectOptions(select, "epic-b");
    expect(screen.getByTestId("search")).toHaveTextContent("epic=epic-b");
    expect(screen.getByTestId("search")).toHaveTextContent("view=plan");
    expect(window.localStorage.getItem("iskworks:build-epic:build-1")).toBe("epic-b");

    await user.selectOptions(select, "");
    expect(screen.getByTestId("search")).not.toHaveTextContent("epic=");
    expect(window.localStorage.getItem("iskworks:build-epic:build-1")).toBeNull();
  });

  it("treats an Epic from another Build as not selectable", () => {
    expect(isSelectableEpic(epic("x", { sourceBuildId: "build-2" }), "build-1")).toBe(false);
    expect(isSelectableEpic(epic("x", { archivedAt: "2026-10-08T11:00:00Z" }), "build-1")).toBe(false);
    expect(isSelectableEpic(epic("x", { completedAt: "2026-10-08T11:00:00Z" }), "build-1")).toBe(false);
    expect(isSelectableEpic(epic("x"), "build-1")).toBe(true);
  });
});

describe("EpicSelector notes", () => {
  const epics = [epic("epic-a", { sourceBuildRevision: 4 })];

  it("says nothing with No Epic selected", () => {
    render(<EpicSelector buildRevision={4} epics={epics} onSelect={vi.fn()} selectedEpicId={null} />);
    expect(screen.queryByText(/Read-only/)).not.toBeInTheDocument();
  });

  it("notes the page is read-only while an Epic is shown", () => {
    render(<EpicSelector buildRevision={4} epics={epics} onSelect={vi.fn()} selectedEpicId="epic-a" />);
    expect(screen.getByText("Read-only · choose No Epic to edit")).toBeInTheDocument();
    expect(screen.queryByText("Build changed since this Epic")).not.toBeInTheDocument();
  });

  it("flags a Build edited after the Epic was frozen", () => {
    render(<EpicSelector buildRevision={9} epics={epics} onSelect={vi.fn()} selectedEpicId="epic-a" />);
    expect(screen.getByText("Build changed since this Epic")).toHaveAttribute(
      "title",
      expect.stringContaining("as it was when the Epic was created"),
    );
  });
});

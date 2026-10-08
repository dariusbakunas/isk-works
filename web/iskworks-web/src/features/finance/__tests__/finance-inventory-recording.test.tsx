import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { FinanceInventoryRecording, FinanceTransaction } from "../../../api/finance";
import type { InventoryPreview } from "../../../api/inventory";
import { TransactionsPage } from "../transactions-page";

const none: FinanceInventoryRecording = {
  state: "unrecorded", recordingId: null, recordedAt: null, revertedAt: null, quantity: null, totalBasis: null,
};
const recorded: FinanceInventoryRecording = {
  state: "recorded",
  recordingId: "rec-1",
  recordedAt: "2026-08-05T19:00:00Z",
  revertedAt: null,
  quantity: 50000,
  totalBasis: "36460000.0000",
};
const preview: InventoryPreview = {
  current: {
    key: { workspaceId: "w", ownerId: "o", typeId: 17425 },
    typeName: "Nitrogen Isotopes",
    quantity: 50000,
    totalHistoricalCost: "35000000.0000",
    averageUnitCost: "700.0000",
    revision: 3,
    lastActivityAt: null,
  },
  posting: { kind: "purchase", quantityDelta: 50000, totalCostDelta: "36460000.0000", unitCost: "729.2000", costQuality: "known" },
  resulting: {
    key: { workspaceId: "w", ownerId: "o", typeId: 17425 },
    typeName: "Nitrogen Isotopes",
    quantity: 100000,
    totalHistoricalCost: "71460000.0000",
    averageUnitCost: "714.6000",
    revision: 4,
    lastActivityAt: null,
  },
  warnings: [],
};
const reverted: FinanceInventoryRecording = { ...recorded, state: "reverted", revertedAt: "2026-08-05T20:00:00Z" };

function row(overrides: Partial<FinanceTransaction> = {}): FinanceTransaction {
  return {
    observationId: "obs-buy",
    transactionId: 1,
    connectionId: "connection-1",
    characterName: "Aura Valex",
    transactionType: "marketBuy",
    typeId: 17425,
    typeName: "Nitrogen Isotopes",
    quantity: 50000,
    unitPrice: "729.2000",
    totalPrice: "36460000.0000",
    transactedAt: "2026-08-05T18:42:00Z",
    counterpartyName: "Caldari Navy",
    locationName: "Jita IV",
    regionName: "The Forge",
    inventoryRecording: none,
    ...overrides,
  };
}

function pageOf(rows: FinanceTransaction[]) {
  return {
    rows,
    summary: {
      walletBalance: "1.0000", income: "0.0000", expenses: "36460000.0000",
      netIsk: "-36460000.0000", transactionCount: rows.length, averageDailyIsk: "0.0000",
    },
    availableCharacters: [],
    totalCount: rows.length,
    page: 1,
    pageSize: 100,
  };
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((onResolve) => { resolve = onResolve; });
  return { promise, resolve };
}

/** Serves the Finance list plus programmable inventory mutations. */
function mockApi(rows: FinanceTransaction[], mutate?: (url: string, init?: RequestInit) => Promise<Response> | Response) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url = String(input);
    if (url.includes("saved-filters")) return json([]);
    if (url.endsWith("/inventory-recording/preview")) return json(preview);
    if (url.includes("inventory-recording")) {
      if (!mutate) throw new Error(`unexpected mutation ${url}`);
      return mutate(url, init);
    }
    return json(pageOf(rows));
  });
}

function recordCalls(fetchMock: ReturnType<typeof mockApi>) {
  return fetchMock.mock.calls.filter(([url]) => /\/inventory-recording$/.test(String(url)));
}

/** Clicks an add button, waits for the server preview, and confirms it. */
async function addThroughPreview(button: HTMLElement) {
  fireEvent.click(button);
  const dialog = await screen.findByRole("dialog", { name: "Add to inventory" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Add to Inventory" }));
  return dialog;
}

function listCalls(fetchMock: ReturnType<typeof mockApi>) {
  return fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/finance/transactions?")).length;
}

function renderPage() {
  return render(<MemoryRouter><TransactionsPage /></MemoryRouter>);
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Finance inventory recording column", () => {
  test("shows + Inventory only for eligible Market Buys", async () => {
    mockApi([
      row(),
      row({ observationId: "obs-sell", transactionType: "marketSell", typeName: "Rifter", inventoryRecording: null }),
      row({ observationId: "obs-unavailable", typeName: "Mystery", inventoryRecording: { ...none, state: "unavailable" } }),
    ]);
    renderPage();

    const add = await screen.findByRole("button", { name: "Add 50,000 Nitrogen Isotopes to inventory" });
    expect(add).toHaveTextContent("+ Inventory");
    expect(add).toHaveAttribute("title", "Preview adding this purchase to inventory");
    expect(screen.getAllByRole("button", { name: /to inventory/ })).toHaveLength(1);
  });

  test("+ Inventory previews the cost effect and records only on confirm", async () => {
    const pending = deferred<Response>();
    const fetchMock = mockApi([row()], () => pending.promise);
    renderPage();
    const add = await screen.findByRole("button", { name: /Add 50,000 Nitrogen Isotopes to inventory/ });
    const listCallsBefore = listCalls(fetchMock);

    fireEvent.click(add);
    const dialog = await screen.findByRole("dialog", { name: "Add to inventory" });
    expect(within(dialog).getByText("Posting preview")).toBeInTheDocument();
    expect(within(dialog).getByText("714.6 ISK")).toBeInTheDocument();
    expect(within(dialog).getByTestId("average-cost-change")).toHaveTextContent("Increases by+14.6 ISK (+2.1%)");
    expect(within(dialog).getByText("from 700 ISK")).toBeInTheDocument();
    // Previewing writes nothing.
    expect(recordCalls(fetchMock)).toHaveLength(0);
    const previewCall = fetchMock.mock.calls.find(([url]) => String(url).endsWith("/preview"))!;
    expect(String(previewCall[0])).toContain("/api/finance/transactions/obs-buy/inventory-recording/preview");
    expect(previewCall[1]?.body).toBeUndefined();

    fireEvent.click(within(dialog).getByRole("button", { name: "Add to Inventory" }));
    expect(await within(dialog).findByRole("button", { name: "Adding…" })).toBeDisabled();
    // Not optimistic: nothing claims success yet.
    expect(screen.queryByText("✓ Inventory")).not.toBeInTheDocument();
    const [post] = recordCalls(fetchMock);
    expect(post[1]).toMatchObject({ method: "POST" });
    expect(post[1]?.body).toBeUndefined();

    pending.resolve(json(recorded, 201));
    expect(await screen.findByText("✓ Inventory")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(listCalls(fetchMock)).toBe(listCallsBefore);
  });

  test("cancelling the preview records nothing", async () => {
    const fetchMock = mockApi([row()], () => json(recorded, 201));
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: /to inventory/ }));
    const dialog = await screen.findByRole("dialog", { name: "Add to inventory" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(recordCalls(fetchMock)).toHaveLength(0);
    expect(screen.getByRole("button", { name: /to inventory/ })).toHaveTextContent("+ Inventory");
  });

  test("an already-recorded race resolves to the recorded state", async () => {
    mockApi([row()], () => json(recorded, 200));
    renderPage();
    await addThroughPreview(await screen.findByRole("button", { name: /to inventory/ }));
    expect(await screen.findByText("✓ Inventory")).toBeInTheDocument();
  });

  test("a failed record keeps the preview open with the reason and the row unrecorded", async () => {
    const fetchMock = mockApi([row(), row({ observationId: "obs-2", typeName: "Other", transactionId: 2 })], () =>
      json({ error: { code: "persistence_unavailable", message: "Database is busy." } }, 503));
    renderPage();
    const search = await screen.findByRole("searchbox");
    fireEvent.change(search, { target: { value: "Nitro" } });
    const [first] = await screen.findAllByRole("button", { name: /Add 50,000 Nitrogen Isotopes to inventory/ });
    const listCallsBefore = listCalls(fetchMock);

    const dialog = await addThroughPreview(first);
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Database is busy.");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.getAllByRole("button", { name: /to inventory/ })).toHaveLength(2);
    expect(screen.getByRole("searchbox")).toHaveValue("Nitro");
    expect(screen.getByText("Other")).toBeInTheDocument();
    expect(listCalls(fetchMock)).toBe(listCallsBefore);
  });

  test("a failed preview flags the row and opens nothing", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.includes("saved-filters")) return json([]);
      if (url.endsWith("/preview")) {
        return json({ error: { code: "validation_failed", message: "The EVE type is not in the active SDE." } }, 400);
      }
      return json(pageOf([row()]));
    });
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: /to inventory/ }));
    expect(await screen.findByRole("alert")).toHaveAttribute("title", "The EVE type is not in the active SDE.");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("keyboard activation opens the preview", async () => {
    mockApi([row()], () => json(recorded, 201));
    renderPage();
    const add = await screen.findByRole("button", { name: /to inventory/ });
    add.focus();
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByRole("dialog", { name: "Add to inventory" })).toBeInTheDocument();
  });

  test("recorded rows show details and revert only after confirmation", async () => {
    const fetchMock = mockApi([row({ inventoryRecording: recorded })], (url) => {
      expect(url).toContain("/inventory-recording/rec-1/revert");
      return json(reverted);
    });
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Inventory recording for 50,000 Nitrogen Isotopes" }));

    const details = await screen.findByRole("dialog", { name: "Recorded in inventory" });
    expect(within(details).getByText("50,000")).toBeInTheDocument();
    expect(within(details).getByText("Acquisition cost")).toBeInTheDocument();
    expect(within(details).getByText("36,460,000 ISK")).toBeInTheDocument();

    fireEvent.click(within(details).getByRole("button", { name: "Revert inventory recording" }));
    const confirm = await screen.findByRole("dialog", { name: "Revert this inventory recording?" });
    expect(confirm).toHaveTextContent("remove 50,000 Nitrogen Isotopes");
    expect(confirm).toHaveTextContent("36,460,000 ISK of recorded acquisition cost");
    expect(confirm).toHaveTextContent("original Finance transaction will remain unchanged");
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/revert"))).toBe(false);

    fireEvent.click(within(confirm).getByRole("button", { name: "Revert recording" }));
    expect(await screen.findByText("Reverted")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("a rejected revert keeps the recording active and explains why", async () => {
    mockApi([row({ inventoryRecording: recorded })], () =>
      json({ error: { code: "validation_failed", message: "This recording can't be reverted because some of the purchased stock has since been used or removed." } }, 400));
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: /Inventory recording for/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Revert inventory recording" }));
    fireEvent.click(await screen.findByRole("button", { name: "Revert recording" }));

    expect(await screen.findByText(/can't be reverted because some of the purchased stock/)).toBeInTheDocument();
    expect(screen.getByText("✓ Inventory")).toBeInTheDocument();
  });

  test("a reverted row can be explicitly added again", async () => {
    mockApi([row({ inventoryRecording: reverted })], () => json({ ...recorded, recordingId: "rec-2" }, 201));
    renderPage();
    expect(await screen.findByText("Reverted")).toBeInTheDocument();
    await addThroughPreview(screen.getByRole("button", { name: "Add 50,000 Nitrogen Isotopes to inventory again" }));
    expect(await screen.findByText("✓ Inventory")).toBeInTheDocument();
    expect(screen.queryByText("Reverted")).not.toBeInTheDocument();
  });

  test("Inventory is a column-chooser option, and hiding it removes the cells", async () => {
    mockApi([row()]);
    const { container } = renderPage();
    await screen.findByRole("button", { name: /to inventory/ });
    expect(screen.getByRole("columnheader", { name: "Inventory" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Columns" }));
    fireEvent.click(screen.getByLabelText("Inventory"));
    expect(screen.queryByRole("columnheader", { name: "Inventory" })).not.toBeInTheDocument();
    expect(container.querySelector('td[data-finance-column="inventory"]')).toBeNull();
  });

  test("the Inventory header is not sortable and export never asks for it", async () => {
    const fetchMock = mockApi([row()]);
    renderPage();
    await screen.findByRole("button", { name: /to inventory/ });
    expect(within(screen.getByRole("columnheader", { name: "Inventory" })).queryByRole("button")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /Export/ }));
    await waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/export?"))).toBe(true));
    const exportUrl = String(fetchMock.mock.calls.find(([url]) => String(url).includes("/export?"))![0]);
    expect(decodeURIComponent(exportUrl)).toContain("columns=time,character");
    expect(exportUrl).not.toContain("inventory");
  });
});

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PreviewBuildPlanCommand } from "../../../../../api/industry/builds";
import { CreateEpicDialog } from "../create-epic-dialog";

const COMMAND = { buildId: "build-1", runs: 1 } as unknown as PreviewBuildPlanCommand;

const ORDER = {
  id: "order-1",
  workspaceId: "workspace-1",
  ownerId: "owner-1",
  sourceBuildId: "build-1",
  sourceBuildRevision: 1,
  displayName: "Manufacture Rifter",
  runs: 1,
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
  status: "blocked",
  rollup: { satisfied: 0, needsAction: 1, inProgress: 0, total: 1 },
  requirements: [],
};

function respond(body: unknown, status = 200) {
  return Promise.resolve(
    new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } }),
  );
}

interface Call {
  url: string;
  body: Record<string, unknown> | null;
}

function stubFetch(createResponses: Array<() => Promise<Response>>) {
  const calls: Call[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, body: init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : null });
    if (url.endsWith("/orders/preview")) {
      return respond({ reuse: [{ typeId: 34, typeName: "Tritanium", quantity: 800 }] });
    }
    if (url.endsWith("/orders")) {
      const next = createResponses.shift();
      if (!next) throw new Error("unexpected create");
      return next();
    }
    throw new Error(`unexpected fetch ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return calls;
}

function renderDialog(onCreated = vi.fn()) {
  render(
    <CreateEpicDialog buildId="build-1" command={COMMAND} onCancel={vi.fn()} onCreated={onCreated} open />,
  );
  return { onCreated };
}

const DRIFT = {
  error: {
    code: "reservation_drift",
    message: "Free inventory changed since the preview. Review the updated reuse and confirm again.",
    retryable: true,
    preview: { reuse: [{ typeId: 34, typeName: "Tritanium", quantity: 600 }] },
    decreased: [{ typeId: 34, expected: 800, now: 600 }],
    shortfalls: [],
  },
};

describe("CreateEpicDialog", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("previews reuse and reserves it by default", async () => {
    const user = userEvent.setup();
    const calls = stubFetch([() => respond(ORDER, 201)]);
    const { onCreated } = renderDialog();

    const dialog = screen.getByRole("dialog", { name: "Create Epic" });
    expect(await within(dialog).findByText("Tritanium")).toBeInTheDocument();
    expect(within(dialog).getByText("800")).toBeInTheDocument();
    expect(within(dialog).getByRole("checkbox", { name: /Reserve inventory/ })).toBeChecked();

    await user.click(within(dialog).getByRole("button", { name: "Create Epic" }));

    await waitFor(() => expect(onCreated).toHaveBeenCalledWith(expect.objectContaining({ id: "order-1" })));
    const create = calls.find((call) => call.url.endsWith("/orders"));
    expect(create?.body).toMatchObject({
      buildId: "build-1",
      reservation: { expectedReuse: [{ typeId: 34, quantity: 800 }] },
    });
  });

  it("creates without a reservation when Reserve is unticked", async () => {
    const user = userEvent.setup();
    const calls = stubFetch([() => respond(ORDER, 201)]);
    renderDialog();

    const dialog = screen.getByRole("dialog", { name: "Create Epic" });
    await user.click(await within(dialog).findByRole("checkbox", { name: /Reserve inventory/ }));
    await user.click(within(dialog).getByRole("button", { name: "Create Epic" }));

    await waitFor(() => expect(calls.some((call) => call.url.endsWith("/orders"))).toBe(true));
    const create = calls.find((call) => call.url.endsWith("/orders"));
    expect(create?.body).not.toHaveProperty("reservation");
  });

  it("shows the drift and refreshes from the 409 body without another request", async () => {
    const user = userEvent.setup();
    const calls = stubFetch([() => respond(DRIFT, 409), () => respond(ORDER, 201)]);
    const { onCreated } = renderDialog();

    const dialog = screen.getByRole("dialog", { name: "Create Epic" });
    await within(dialog).findByText("Tritanium");
    await user.click(within(dialog).getByRole("button", { name: "Create Epic" }));

    expect(await within(dialog).findByText("Free inventory changed")).toBeInTheDocument();
    const changes = within(dialog).getByRole("table", { name: "Inventory changes" });
    expect(within(changes).getByText("800")).toBeInTheDocument();
    expect(within(changes).getByText("600")).toBeInTheDocument();

    const requestsBeforeRefresh = calls.length;
    await user.click(within(dialog).getByRole("button", { name: "Refresh" }));
    expect(calls.length).toBe(requestsBeforeRefresh);
    expect(within(dialog).queryByText("Free inventory changed")).not.toBeInTheDocument();
    expect(within(dialog).getByText("600")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Create Epic" }));
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    const creates = calls.filter((call) => call.url.endsWith("/orders"));
    expect(creates[1].body).toMatchObject({
      reservation: { expectedReuse: [{ typeId: 34, quantity: 600 }] },
    });
  });

  it("offers to create without reserving after a drift", async () => {
    const user = userEvent.setup();
    const calls = stubFetch([() => respond(DRIFT, 409), () => respond(ORDER, 201)]);
    const { onCreated } = renderDialog();

    const dialog = screen.getByRole("dialog", { name: "Create Epic" });
    await within(dialog).findByText("Tritanium");
    await user.click(within(dialog).getByRole("button", { name: "Create Epic" }));
    await user.click(await within(dialog).findByRole("button", { name: "Create without reserving" }));

    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    const creates = calls.filter((call) => call.url.endsWith("/orders"));
    expect(creates[1].body).not.toHaveProperty("reservation");
  });

  it("shows a reuse increase before opening the Epic", async () => {
    const user = userEvent.setup();
    stubFetch([
      () => respond({ ...ORDER, reuseIncreased: [{ typeId: 34, expected: 800, now: 1000 }] }, 201),
    ]);
    const { onCreated } = renderDialog();

    const dialog = screen.getByRole("dialog", { name: "Create Epic" });
    await within(dialog).findByText("Tritanium");
    await user.click(within(dialog).getByRole("button", { name: "Create Epic" }));

    expect(await within(dialog).findByText(/reserved more/)).toBeInTheDocument();
    expect(within(dialog).getByText("1,000")).toBeInTheDocument();
    expect(onCreated).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole("button", { name: "Open Epic" }));
    expect(onCreated).toHaveBeenCalledWith(expect.objectContaining({ id: "order-1" }));
  });
});

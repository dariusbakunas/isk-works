import { expect, test, type Page, type Route } from "@playwright/test";

type Ticket = Record<string, unknown> & {
  id: string;
  status: string;
  recording: Record<string, unknown>;
  recordings: Array<Record<string, unknown>>;
};

const workspace = {
  configured: true,
  workspace: { id: "workspace-1", name: "Industry", ownerId: "owner-1", createdAt: "2026-09-25T10:00:00Z", updatedAt: "2026-09-25T10:00:00Z" },
  owner: { id: "owner-1", workspaceId: "workspace-1", kind: "manual", displayName: "Industry", hidden: true },
};

function recordingSummary(requestedQuantity: number, recordedQuantity: number) {
  const remainingQuantity = Math.max(requestedQuantity - recordedQuantity, 0);
  return {
    state: recordedQuantity === 0 ? "notRecorded" : remainingQuantity === 0 ? "recorded" : "partiallyRecorded",
    requestedQuantity,
    recordedQuantity,
    remainingQuantity,
    surplusQuantity: Math.max(recordedQuantity - requestedQuantity, 0),
  };
}

function ticket(overrides: Record<string, unknown>): Ticket {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-2401",
    kind: "acquisition",
    typeId: 34,
    capturedName: "Tritanium",
    quantity: 100,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: null,
    status: "inProgress",
    estimatedUnitCost: "5.0000",
    estimatedLineTotal: "500.0000",
    actualUnitCost: null,
    actualLineTotal: null,
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: null,
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-09-25T10:00:00Z",
    updatedAt: "2026-09-25T10:00:00Z",
    archivedAt: null,
    blockedBy: [],
    prerequisites: [],
    recording: recordingSummary(100, 100),
    recordings: [],
    ...overrides,
  } as Ticket;
}

function acquisitionRecording(status: "recorded" | "reversed", id = "recording-acq") {
  return {
    id,
    ticketId: "ticket-acq",
    kind: "acquisition",
    recordedQuantity: 100,
    runsCompleted: null,
    installationCost: null,
    outputTypeId: null,
    outputQuantity: null,
    locationNote: "Jita 4-4",
    note: "Original purchase",
    recordedAt: "2026-09-25T10:15:00Z",
    revertedAt: status === "reversed" ? "2026-09-25T10:30:00Z" : null,
    status,
    effects: [{ eventId: `event-${id}`, kind: "purchase", typeId: 34, capturedName: "Tritanium", quantityDelta: 100, totalCostDelta: "500.0000" }],
  };
}

function productionRecording(status: "recorded" | "reversed") {
  return {
    id: "recording-prod",
    ticketId: "ticket-prod",
    kind: "production",
    recordedQuantity: null,
    runsCompleted: 2,
    installationCost: "20.0000",
    outputTypeId: 587,
    outputQuantity: 2,
    locationNote: "Astrahus Alpha",
    note: "Frozen plan execution",
    recordedAt: "2026-09-25T11:00:00Z",
    revertedAt: status === "reversed" ? "2026-09-25T11:05:00Z" : null,
    status,
    effects: [
      { eventId: "event-output", kind: "productionOutput", typeId: 587, capturedName: "Rifter", quantityDelta: 2, totalCostDelta: "140.0000" },
      { eventId: "event-input", kind: "consumption", typeId: 34, capturedName: "Tritanium", quantityDelta: -30, totalCostDelta: "-120.0000" },
    ],
  };
}

async function installBoardApi(page: Page, initial: Ticket) {
  let current = structuredClone(initial);
  await page.route("**/api/**", async (route: Route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (!path.startsWith("/api/")) {
      await route.continue();
      return;
    }
    let body: unknown = [];

    if (path === "/api/auth/session") {
      body = { authenticated: true, characterName: "Aeva Stark", workspaceId: "workspace-1", inviteRequired: false };
    } else if (path === "/api/workspace") {
      body = workspace;
    } else if (path === "/api/tickets" && request.method() === "GET") {
      body = [current];
    } else if (path.match(/^\/api\/tickets\/[^/]+\/recordings\/[^/]+\/revert$/) && request.method() === "POST") {
      current.recordings = current.recordings.map((recording) => ({
        ...recording,
        status: "reversed",
        revertedAt: "2026-09-25T12:00:00Z",
      }));
      const requested = Number(current.recording.requestedQuantity);
      current.recording = recordingSummary(requested, 0);
      body = { recording: current.recordings[0], summary: current.recording, recordings: current.recordings };
    } else if (path.endsWith("/record-acquisition") && request.method() === "POST") {
      const input = request.postDataJSON() as { quantity: number; unitCost?: string; locationNote?: string; note?: string };
      const corrected = {
        ...acquisitionRecording("recorded", "recording-corrected"),
        recordedQuantity: input.quantity,
        locationNote: input.locationNote ?? "",
        note: input.note ?? "",
        recordedAt: "2026-09-25T12:05:00Z",
        effects: [{ eventId: "event-corrected", kind: "purchase", typeId: 34, capturedName: "Tritanium", quantityDelta: input.quantity, totalCostDelta: `${(Number(input.unitCost ?? 5) * input.quantity).toFixed(4)}` }],
      };
      current.recordings = [...current.recordings, corrected];
      current.recording = recordingSummary(100, input.quantity);
      body = { recording: corrected, summary: current.recording };
    } else if (path.endsWith("/record-production") && request.method() === "POST") {
      const input = request.postDataJSON() as { runsCompleted: number; output: { quantity: number } };
      const corrected = {
        ...productionRecording("recorded"),
        id: "recording-prod-corrected",
        runsCompleted: input.runsCompleted,
        outputQuantity: input.output.quantity,
        recordedAt: "2026-09-25T12:10:00Z",
        effects: [],
      };
      current.recordings = [...current.recordings, corrected];
      current.recording = recordingSummary(2, input.runsCompleted);
      body = { recording: corrected, summary: current.recording };
    }

    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });
  });
}

test("reverts an acquisition and records a corrected replacement without changing workflow status", async ({ page }) => {
  const original = acquisitionRecording("recorded");
  await installBoardApi(page, ticket({ id: "ticket-acq", recordings: [original] }));
  await page.goto("/board?ticket=ticket-acq");

  const drawer = page.getByRole("complementary", { name: "Tritanium" });
  await expect(drawer.locator("span").filter({ hasText: /^In Progress$/ })).toBeVisible();
  await drawer.getByRole("button", { name: "Revert recording" }).click();
  const dialog = page.getByRole("dialog", { name: "Revert recording?" });
  await expect(dialog).toContainText("+100 × Tritanium at Jita 4-4");
  await dialog.getByRole("button", { name: "Revert recording" }).click();

  await expect(drawer.getByText("Reverted", { exact: true })).toBeVisible();
  await expect(drawer.locator("span").filter({ hasText: /^In Progress$/ })).toBeVisible();
  await drawer.getByRole("button", { name: "Record acquisition" }).click();
  const form = drawer.getByRole("form", { name: "Record acquisition" });
  await form.getByLabel("Quantity").fill("90");
  await form.getByLabel("Unit cost (ISK)").fill("6");
  await form.getByLabel(/Location \/ reference/).fill("Amarr VIII");
  await form.getByRole("button", { name: "Record", exact: true }).click();

  await expect(drawer.getByText("90 × Tritanium")).toBeVisible();
  await expect(drawer.getByText("Reverted", { exact: true })).toBeVisible();
  await expect(drawer.locator("span").filter({ hasText: /^In Progress$/ })).toBeVisible();
});

test.describe("375px mobile", () => {
  test.use({ viewport: { width: 375, height: 812 } });

  test("shows exact production effects and the persistent reverted row", async ({ page }) => {
    const recording = productionRecording("recorded");
    await installBoardApi(page, ticket({
      id: "ticket-prod",
      displayId: "ISK-2402",
      kind: "manufacturing",
      typeId: 587,
      capturedName: "Rifter",
      quantity: 2,
      sourceBuildId: "deleted-build",
      recording: recordingSummary(2, 2),
      recordings: [recording],
      prerequisites: [{ id: "prereq-trit", ticketId: "ticket-prod", typeId: 34, capturedName: "Tritanium", kind: "buy", sourceBuildId: null, requiredQuantity: 30, fulfillmentScope: "full", reusedQuantity: 0, freshQuantity: 30, estimatedUnitCost: "4.0000", estimatedLineTotal: "120.0000", reusedLineTotal: "0.0000" }],
      executionSnapshot: { runs: 2, blueprint: null, facility: null, durationSeconds: 12000, installationCost: { total: "20.0000" }, materialValue: "120.0000" },
    }));
    await page.goto("/board?ticket=ticket-prod");

    const drawer = page.getByRole("complementary", { name: "Rifter" });
    await expect(drawer).toBeVisible();
    await drawer.getByRole("button", { name: "Revert recording" }).click();
    const dialog = page.getByRole("dialog", { name: "Revert recording?" });
    await expect(dialog).toContainText("This will reverse 2 inventory changes.");
    await expect(dialog).toContainText("+2 × Rifter at Astrahus Alpha");
    await expect(dialog).toContainText("-30 × Tritanium at Astrahus Alpha");
    await dialog.getByRole("button", { name: "Revert recording" }).click();

    await expect(drawer.getByText("Reverted", { exact: true })).toBeVisible();
    await expect(drawer.locator("span").filter({ hasText: /^In Progress$/ })).toBeVisible();
    await drawer.getByRole("button", { name: "Record production" }).click();
    await drawer
      .getByRole("form", { name: "Record production" })
      .getByRole("button", { name: "Record", exact: true })
      .click();
    await expect(drawer.getByRole("button", { name: "Revert recording" })).toBeVisible();
    await expect(drawer.getByText("Reverted", { exact: true })).toBeVisible();
    const box = await drawer.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(375);
  });
});

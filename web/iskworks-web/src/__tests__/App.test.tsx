import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { App } from "../App";

const configuredWorkspace = {
  configured: true,
  workspace: {
    id: "workspace-1",
    name: "Personal Industry",
    ownerId: "owner-1",
    createdAt: "2026-07-25T10:00:00Z",
    updatedAt: "2026-07-25T10:00:00Z",
  },
  owner: {
    id: "owner-1",
    workspaceId: "workspace-1",
    kind: "manual",
    displayName: "Personal Industry",
    hidden: true,
  },
  version: "v1.2.3 (a1b2c3d)",
};

const authenticatedSession = {
  authenticated: true,
  characterName: "Fixture Character",
  workspaceId: "workspace-1",
};

function plannedBuildFixture() {
  return {
    id: "build-ready",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    name: "Ready Rifter build",
    status: "planned",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "sde-1",
      sourceSdeVersion: "3389399",
      blueprintTypeId: 691,
      blueprintName: "Rifter Blueprint",
      durationSecondsPerRun: 6000,
      materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, sortOrder: 0 }],
      products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, sortOrder: 0 }],
      fingerprint: "recipe",
    },
    runs: 1,
    notes: "",
    activePlanId: "plan-1",
    revision: 2,
    createdAt: "2026-07-27T10:00:00Z",
    updatedAt: "2026-07-27T10:00:00Z",
    plannedAt: "2026-07-27T10:00:00Z",
    productionStartedAt: null,
    completedAt: null,
    historicalMaterialCost: null,
    historicalMaterialCostQuality: null,
    productionPlanRevision: null,
    plans: [{
      id: "plan-1",
      revision: 1,
      runs: 1,
      recipeFingerprint: "recipe",
      snapshot: {
        id: "snapshot-1",
        priceSourceId: "source-1",
        sourceName: "C-J6MT",
        sourceRevision: 1,
        createdAt: "2026-07-27T10:00:00Z",
        items: [
          { typeId: 34, typeName: "Tritanium", price: "4.0000", pricingPolicy: "highestBuy", missing: false, sourceNote: "", sortOrder: 0 },
          { typeId: 587, typeName: "Rifter", price: "1000000.0000", pricingPolicy: "lowestSell", missing: false, sourceNote: "", sortOrder: 1 },
        ],
      },
      pricingComplete: true,
      estimatedMaterialCost: "128000.0000",
      expectedRevenue: "1000000.0000",
      estimatedMargin: "872000.0000",
      missingPriceCount: 0,
      active: true,
      plannedAt: "2026-07-27T10:00:00Z",
      supersededAt: null,
      materialLines: [{
        typeId: 34,
        typeName: "Tritanium",
        quantityPerRun: 32000,
        totalQuantity: 32000,
        unitPrice: "4.0000",
        lineTotal: "128000.0000",
        missing: false,
      }],
      facility: null,
    }],
    recipeCurrency: "current",
    activeSdeVersion: "3389399",
  };
}

function worksheetFixture(required: number, covered: number, runs = 1, isBuildResolved = false) {
  return {
    groups: [{
      key: "mineral",
      label: "Mineral",
      items: [{
        typeId: 34,
        typeName: "Tritanium",
        role: "material",
        requiredQuantity: required,
        availableQuantity: 100000,
        coveredQuantity: covered,
        missingQuantity: Math.max(0, required - covered),
        coveragePercentage: required === 0 ? "0.00" : ((covered * 100) / required).toFixed(2),
        projectedInventoryCost: "5000.0000",
        pricing: {
          selectionKind: "default",
          effectivePolicy: "highestBuy",
          unitPrice: "4.0000",
          manualUnitPrice: null,
          missing: false,
          sourceNote: "",
        },
        lineTotal: `${required * 4}.0000`,
        contributions: [],
        isBuildResolved,
        installationCost: null,
      }],
    }],
    output: {
      key: "output",
      label: "Output",
      items: [{
        typeId: 5876,
        typeName: "Rifter",
        role: "output",
        requiredQuantity: runs,
        availableQuantity: 0,
        coveredQuantity: 0,
        missingQuantity: 0,
        coveragePercentage: "0.00",
        projectedInventoryCost: null,
        pricing: {
          selectionKind: "default",
          effectivePolicy: "lowestSell",
          unitPrice: "1000000.0000",
          manualUnitPrice: null,
          missing: false,
          sourceNote: "",
        },
        lineTotal: `${runs * 1000000}.0000`,
        contributions: [],
      }],
    },
    summary: {
      materialCost: `${required * 4}.0000`,
      installationCost: null,
      totalCost: null,
      expectedRevenue: `${runs * 1000000}.0000`,
      estimatedMargin: `${runs * 1000000 - required * 4}.0000`,
      pricingComplete: true,
      quantityCoverageComplete: covered >= required,
      costCoverageComplete: true,
      warnings: [],
    },
  };
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("App workspace gate", () => {
  test("setup is shown when backend reports unconfigured", async () => {
    mockFetchSequence([{ configured: false, workspace: null, owner: null }]);

    renderApp("/builds");

    expect(screen.getByText("Checking session...")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Create your workspace" })).toBeInTheDocument();
  });

  test("setup does not flash during initial loading", () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise(() => undefined)));

    renderApp("/setup");

    expect(screen.getByText("Checking session...")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Create your workspace" })).not.toBeInTheDocument();
  });

  test("successful creation redirects to the Characters home", async () => {
    mockFetchSequence([{ configured: false, workspace: null, owner: null }, configuredWorkspace, []]);
    const user = userEvent.setup();

    renderApp("/setup");

    await user.type(await screen.findByLabelText("Workspace name"), "Personal Industry");
    await user.click(screen.getByRole("button", { name: "Start workspace" }));

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(screen.getByText("Personal Industry")).toBeInTheDocument();
  });

  test("configured workspace redirects from setup to the Characters home", async () => {
    mockFetchSequence([configuredWorkspace, []]);

    renderApp("/setup");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Create your workspace" })).not.toBeInTheDocument();
  });

  test("setup no longer offers unfinished options", async () => {
    mockFetchSequence([{ configured: false, workspace: null, owner: null }]);

    renderApp("/setup");

    expect(await screen.findByRole("heading", { name: "Create your workspace" })).toBeInTheDocument();
    expect(screen.queryByText(/Coming next/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/not implemented yet/i)).not.toBeInTheDocument();
    expect(screen.queryByText("Import CSV")).not.toBeInTheDocument();
  });

  test("a workspace-state outage after a valid session is shown honestly", async () => {
    // Session verifies fine; the workspace load then fails -> WorkspaceApp's
    // own honest error, not the auth gate.
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
        if (url.endsWith("/api/workspace")) return mockResponse({ error: { code: "persistence_unavailable", message: "db down" } }, false, 503);
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderApp("/");

    expect(await screen.findByText("Workspace state unavailable")).toBeInTheDocument();
  });
});

describe("App auth gate fails closed", () => {
  function sessionOnly(handler: (url: string) => Promise<unknown> | unknown) {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return handler(url);
      throw new Error(`unexpected fetch while auth state is not resolved: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    return fetchMock;
  }

  test("a session network failure shows a retry state, never the product surface", async () => {
    const fetchMock = sessionOnly(() => Promise.reject(new Error("offline")));

    renderApp("/");

    expect(await screen.findByText("We couldn't verify your session")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Characters" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Sign in" })).not.toBeInTheDocument();
    // No workspace/data request may fire while the session is unverified.
    expect(fetchMock.mock.calls.every(([input]) => String(input).endsWith("/api/auth/session"))).toBe(true);
  });

  test("an unexpected 500 from the session endpoint fails closed, not into the app", async () => {
    sessionOnly(() => mockResponse({ error: { code: "internal", message: "boom" } }, false, 500));

    renderApp("/");

    expect(await screen.findByText("We couldn't verify your session")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Characters" })).not.toBeInTheDocument();
  });

  test("a non-auth 503 fails closed and is not mistaken for auth-disabled", async () => {
    sessionOnly(() => mockResponse({ error: { code: "persistence_unavailable", message: "db down" } }, false, 503));

    renderApp("/");

    expect(await screen.findByText("We couldn't verify your session")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Characters" })).not.toBeInTheDocument();
  });

  test("Retry re-checks the session and loads the app once it recovers", async () => {
    let call = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) {
        call += 1;
        if (call === 1) throw new Error("offline");
        return mockResponse(authenticatedSession);
      }
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/characters")) return mockResponse([]);
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/");

    await user.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
  });

  test("an explicit auth_not_configured (local/dev) still enters the legacy workspace path", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith("/api/auth/session")) {
          return mockResponse({ error: { code: "auth_not_configured", message: "off" } }, false, 503);
        }
        if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
        if (url.endsWith("/api/characters")) return mockResponse([]);
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderApp("/");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(screen.queryByText("We couldn't verify your session")).not.toBeInTheDocument();
  });
});

describe("About & Legal page", () => {
  test("is readable without a session check", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      throw new Error(`unexpected fetch: ${String(input)}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderApp("/legal");

    expect(await screen.findByRole("heading", { name: "About, privacy & legal" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "CCP notice" })).toBeInTheDocument();
    expect(screen.getAllByText(/© 2014 CCP hf\. All rights reserved\./).length).toBeGreaterThan(0);
    expect(screen.getByText(/contact the operator of this instance and we will erase it/)).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("Help page", () => {
  function stubNoFetch() {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      throw new Error(`unexpected fetch: ${String(input)}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    return fetchMock;
  }

  test("opens on Getting started without a session check", async () => {
    const fetchMock = stubNoFetch();

    renderApp("/help");

    expect(await screen.findByRole("heading", { level: 1, name: "Getting started" })).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Help sections" })).getByRole("link", { name: "Getting started" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("switches sections and redirects unknown ones to the first section", async () => {
    stubNoFetch();
    const user = userEvent.setup();

    renderApp("/help/no-such-section");

    expect(await screen.findByRole("heading", { level: 1, name: "Getting started" })).toBeInTheDocument();
    await user.click(within(screen.getByRole("navigation", { name: "Help sections" })).getByRole("link", { name: "Self-hosting" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Self-hosting" })).toBeInTheDocument();
  });

  test("shows a section's hero illustration above its text", async () => {
    stubNoFetch();

    renderApp("/help/where-it-runs");

    expect(await screen.findByRole("heading", { level: 1, name: "Where it runs" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /beaver in a space suit, sitting in a wooden barn office/i })).toBeInTheDocument();
  });

  test("sections without a hero render no illustration", async () => {
    stubNoFetch();

    renderApp("/help/getting-started");

    expect(await screen.findByRole("heading", { level: 1, name: "Getting started" })).toBeInTheDocument();
    expect(within(screen.getByRole("main")).queryByRole("img")).not.toBeInTheDocument();
  });
});

describe("EVE SSO login gate", () => {
  test("an unauthenticated session shows the login page instead of the app", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        if (String(input).endsWith("/api/auth/session")) {
          return mockResponse({ authenticated: false, characterName: null, workspaceId: null });
        }
        throw new Error(`unexpected fetch: ${String(input)}`);
      }),
    );

    renderApp("/");

    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue with EVE Online" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Characters" })).not.toBeInTheDocument();
  });

  test("an authenticated session passes through to the app with the character shown", async () => {
    mockFetchSequence([configuredWorkspace, []]);

    renderApp("/");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Sign in" })).not.toBeInTheDocument();
    expect(screen.getByText(authenticatedSession.characterName)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign out" })).toBeInTheDocument();
  });

  test("EVE SSO not being configured falls back to the legacy workspace gate", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith("/api/auth/session")) {
          return mockResponse(
            { error: { code: "auth_not_configured", message: "EVE SSO login is not configured." } },
            false,
            503,
          );
        }
        if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
        if (url.endsWith("/api/characters")) return mockResponse([]);
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderApp("/");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Sign in" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign out" })).not.toBeInTheDocument();
  });
});

describe("App routes", () => {
  test("shows the running API version in the header", async () => {
    mockFetchSequence([configuredWorkspace, []]);

    renderApp("/");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(screen.getByTitle("API v1.2.3 (a1b2c3d)")).toHaveTextContent("v1.2.3 (a1b2c3d)");
  });

  test("/ lands on the Characters home", async () => {
    mockFetchSequence([configuredWorkspace, []]);

    renderApp("/");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent("/characters");
  });

  test("the primary nav is the alpha surface: Characters first, Board present, no Overview or Sales", async () => {
    mockFetchSequence([configuredWorkspace, []]);

    renderApp("/");
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();

    const primaryNav = screen.getByRole("navigation", { name: "Primary navigation" });
    const navLabels = within(primaryNav)
      .getAllByRole("link")
      .map((link) => link.textContent?.trim());

    expect(navLabels[0]).toBe("Characters");
    expect(navLabels).toContain("Board");
    expect(navLabels).toContain("Planetary");
    expect(navLabels).not.toContain("Overview");
    expect(navLabels).not.toContain("Sales");
  });

  test("the Admin nav link appears only for admin sessions", async () => {
    const withSession = (session: unknown) =>
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL) => {
          const url = String(input);
          if (url.endsWith("/api/auth/session")) return mockResponse(session);
          if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
          if (url.endsWith("/api/characters")) return mockResponse([]);
          throw new Error(`unexpected fetch: ${url}`);
        }),
      );
    const navLabels = () =>
      within(screen.getByRole("navigation", { name: "Primary navigation" }))
        .getAllByRole("link")
        .map((link) => link.textContent?.trim());

    withSession({ ...authenticatedSession, isAdmin: true });
    const admin = renderApp("/");
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(navLabels()).toContain("Admin");
    admin.unmount();

    withSession(authenticatedSession);
    renderApp("/");
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
    expect(navLabels()).not.toContain("Admin");
  });

  test("navigation routes still render", async () => {
    mockFetchSequence([configuredWorkspace, [], [], [], [], [], [], []]);
    const user = userEvent.setup();

    renderApp("/");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: /Builds/ }));
    expect(await screen.findByRole("heading", { name: "Builds" })).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: /Inventory/ }));
    expect(await screen.findByRole("heading", { name: "Inventory" })).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: /Price Overrides/ }));
    expect(await screen.findByRole("heading", { name: "Price Overrides" })).toBeInTheDocument();

    const primaryNav = screen.getByRole("navigation", { name: "Primary navigation" });
    expect(primaryNav.closest("header")).toBeInTheDocument();
    expect(within(primaryNav).getByRole("link", { name: /^Board$/ })).toBeInTheDocument();
    expect(within(primaryNav).queryByRole("link", { name: /^Sales$/ })).not.toBeInTheDocument();
    await user.click(within(primaryNav).getByRole("link", { name: /^Characters$/ }));
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
  });

  test("the header logo links to the Characters home", async () => {
    mockFetchSequence([configuredWorkspace, []]);

    renderApp("/");
    await screen.findByRole("heading", { name: "Characters" });

    const header = screen.getByRole("navigation", { name: "Primary navigation" }).closest("header");
    const logoLink = within(header as HTMLElement).getByRole("link", { name: /ISK Works/ });
    expect(logoLink).toHaveAttribute("href", "/characters");
  });

  test("legacy /orders/:id still redirects to the Board epic deep link", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
        if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
        return mockResponse([]);
      }),
    );

    renderApp("/orders/order-42");

    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("/board"));
  });

  test("unknown route renders not-found state", async () => {
    mockFetchSequence([configuredWorkspace]);

    renderApp("/unknown");

    expect(await screen.findByRole("heading", { name: "Page not found" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to Characters" })).toBeInTheDocument();
  });
});

describe("Create Build planning", () => {
  test("shows an honest empty state when no SDE is active", async () => {
    mockFetchSequence([
      configuredWorkspace,
      { configured: false, active: null },
      [],
      [],
    ]);

    renderApp("/builds/new");

    expect(await screen.findByRole("heading", { name: "Create Build" })).toBeInTheDocument();
    expect(await screen.findByText("No SDE imported")).toBeInTheDocument();
  });

  test("searches manufacturing recipes and calculates a direct plan", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/sde")) return mockResponse({
        configured: true,
        active: {
          importId: "import-1",
          sourceVersion: "123456",
          sourceLabel: "fixture.zip",
          sourceChecksum: "abc",
          completedAt: "2026-07-25T12:00:00Z",
          counts: { types: 4, blueprints: 1, materialLines: 1, productLines: 1, skippedBlueprints: 0 },
        },
      });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.includes("/api/blueprints/search")) return mockResponse([
        {
          blueprintTypeId: 6830,
          blueprintName: "Rifter Blueprint",
          productTypeId: 5876,
          productName: "Rifter",
          groupName: "Frigate",
          published: true,
          manufacturingAvailable: true,
        },
      ]);
      if (url.includes("/api/reaction-formulas/search")) return mockResponse([]);
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
      if (url.includes("/api/blueprints/6830/plan")) return mockResponse({
        blueprintTypeId: 6830,
        blueprintName: "Rifter Blueprint",
        runs: 3,
        durationSeconds: 1800,
        materials: [
          { typeId: 34, typeName: "Tritanium", quantityPerRun: 1000, totalQuantity: 3000 },
        ],
        products: [
          { typeId: 5876, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 3 },
        ],
      });
      if (url.endsWith("/api/market/regions")) return mockResponse([]);
      if (url.includes("/api/market/regions/") && url.includes("/locations")) return mockResponse([]);
      throw new Error(`Unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/builds/new");

    expect(await screen.findByRole("dialog", { name: "Select a product" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Builds" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Create Build" })).not.toBeInTheDocument();
    await user.type(await screen.findByLabelText("Product"), "Rifter");
    const result = await screen.findByRole("button", { name: /Rifter.*Rifter Blueprint/s });
    expect(within(result).getByRole("img", { name: "Rifter" })).toBeInTheDocument();
    await user.click(result);
    expect(screen.queryByRole("dialog", { name: "Select a product" })).not.toBeInTheDocument();
    await user.clear(await screen.findByLabelText("Runs"));
    await user.type(screen.getByLabelText("Runs"), "3");

    expect(screen.queryByText("Recipe details")).not.toBeInTheDocument();
    const blueprintImage = screen.getByRole("img", { name: "Rifter Blueprint" });
    expect(blueprintImage).toHaveAttribute("src", expect.stringContaining("/bp?size=64"));
    expect(blueprintImage).toHaveAttribute("width", "64");
    expect(within(screen.getByRole("region", { name: "Planning assumptions" }))
      .queryByRole("img", { name: "Rifter Blueprint · ME 0 · TE 0" })).not.toBeInTheDocument();
    expect(screen.getByText("Rifter Blueprint · ME 0 · TE 0")).toBeInTheDocument();
    const manufacturingSummary = within(screen.getByRole("group", { name: "Manufacturing" }));
    expect(manufacturingSummary.getByText("Not selected")).toBeInTheDocument();
    expect(manufacturingSummary.getByText("Not selected")).toHaveClass("text-warning");
    await user.click(await screen.findByRole("button", { name: "Edit blueprint" }));
    expect(screen.getByRole("button", { name: "Enter manually" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    expect(screen.getByText("No available blueprints")).toBeInTheDocument();
    expect(screen.getByText("Blueprints found during your latest ESI synchronization.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Done" }));
    await user.click(screen.getByRole("button", { name: "Edit build settings" }));
    expect(screen.getByText("None -- market price only")).toBeInTheDocument();
  });

  test("automatically previews selected manufacturing assumptions", async () => {
    const build = plannedBuildFixture();
    const facility = {
      id: "facility-1",
      workspaceId: "workspace-1",
      name: "GEZ Sotiyo",
      kind: "manual",
      role: "manufacturing",
      structureId: 1050487654321,
      structureTypeId: 35827,
      structureTypeName: "Sotiyo",
      solarSystemId: 30000505,
      solarSystemName: "C-J6MT",
      securityClass: "nullSec",
      materialReductionPercent: "1.0",
      timeReductionPercent: "30.0",
      jobCostReductionPercent: "5.0",
      facilityTaxPercent: "1.0",
      sccSurchargePercent: "4.0",
      allianceSurchargePercent: "0",
      fixedSupplementalCost: "0.0000",
      manualSystemCostIndex: "0.0979",
      notes: "",
      rigs: [],
      archivedAt: null,
      revision: 1,
      createdAt: "2026-07-27T10:00:00Z",
      updatedAt: "2026-07-27T10:00:00Z",
    };
    const source = {
      id: "source-1",
      workspaceId: "workspace-1",
      name: "C-J6MT",
      kind: "manual",
      revision: 4,
      items: [
        { typeId: 34, typeName: "Tritanium", price: "5.0000", note: "" },
        { typeId: 5876, typeName: "Rifter", price: "500000.0000", note: "" },
      ],
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/sde")) return mockResponse({
        configured: true,
        active: {
          importId: "import-1",
          sourceVersion: "123456",
          sourceLabel: "fixture.zip",
          sourceChecksum: "abc",
          completedAt: "2026-07-25T12:00:00Z",
          counts: { types: 4, blueprints: 1, materialLines: 1, productLines: 1, skippedBlueprints: 0 },
        },
      });
      if (url.endsWith("/api/price-sources")) return mockResponse([source]);
      if (url.includes("/api/blueprints/search")) return mockResponse([{
        blueprintTypeId: 6830,
        blueprintName: "Rifter Blueprint",
        productTypeId: 5876,
        productName: "Rifter",
        groupName: "Frigate",
        published: true,
        manufacturingAvailable: true,
      }]);
      if (url.includes("/api/blueprints/6830/plan")) return mockResponse({
        blueprintTypeId: 6830,
        blueprintName: "Rifter Blueprint",
        runs: 1,
        durationSeconds: 600,
        materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 1000, totalQuantity: 1000 }],
        products: [{ typeId: 5876, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
      });
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([
        {
          id: "original-1",
          workspaceId: "workspace-1",
          ownerId: "owner-1",
          ownerName: "Valka",
          eveItemId: 1001,
          blueprintTypeId: 6830,
          blueprintName: "Rifter Blueprint",
          kind: "original",
          materialEfficiency: 10,
          timeEfficiency: 20,
          licensedRuns: null,
          locationId: 60003760,
          locationFlag: "Hangar",
          locationName: "C-J6MT - GEZ",
          observedAt: "2026-07-27T16:00:00Z",
          importedAt: "2026-07-27T16:00:00Z",
        },
        {
          id: "copy-1",
          workspaceId: "workspace-1",
          ownerId: "owner-1",
          ownerName: "Valka",
          eveItemId: 1002,
          blueprintTypeId: 6830,
          blueprintName: "Rifter Blueprint",
          kind: "copy",
          materialEfficiency: 10,
          timeEfficiency: 20,
          licensedRuns: 30,
          locationId: 60003760,
          locationFlag: "Hangar",
          locationName: "C-J6MT - GEZ",
          observedAt: "2026-07-27T16:00:00Z",
          importedAt: "2026-07-27T16:00:00Z",
        },
      ]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([facility]);
      if (url.endsWith("/api/blueprints/6830/estimated-item-value?runs=1")) {
        return mockResponse({
          value: "372548.0000",
          missingTypeIds: [],
          observedAt: "2026-07-27T20:00:00Z",
          expiresAt: "2026-07-27T21:00:00Z",
        });
      }
      if (url.endsWith("/api/recipes/for-product/34")) {
        return mockResponse({ mode: "manufacturing", blueprintTypeId: 90002 });
      }
      if (url.endsWith("/api/build-plans/candidate-preview")) {
        const candidateRequest = JSON.parse(String(init?.body));
        return mockResponse({
        candidateFingerprint: "create-candidate",
        canPlan: true,
        candidate: build.plans[0],
        coverage: {
          buildId: "preview-build",
          ownerId: "owner-1",
          buildStatus: "draft",
          buildRevision: 1,
          recipeFingerprint: "recipe",
          runs: 1,
          completeQuantityCoverage: false,
          completeCostCoverage: true,
          hasActiveReservations: false,
          canReserve: false,
          canRefreshReservations: false,
          canReleaseReservations: false,
          canStartProduction: false,
          materialLines: [{
            typeId: 34,
            typeName: "Tritanium",
            sortOrder: 0,
            requiredQuantity: 1000,
            accountedOwnedQuantity: 900,
            reservedForThisBuild: 0,
            reservedByOtherBuilds: 0,
            unreservedAvailableQuantity: 900,
            availableToThisBuild: 900,
            reservableAdditionalQuantity: 900,
            coveredQuantity: 900,
            missingQuantity: 100,
            averageHistoricalUnitCost: "5.0000",
            projectedHistoricalCost: "5000.0000",
            costQuality: "known",
            quantityCoverageState: "partiallyCovered",
            esiObservedQuantity: null,
            esiReconciliationDifference: null,
            esiObservedAt: null,
            explanation: "",
            warnings: [],
            inventoryRevision: 1,
          }],
          warnings: [],
        },
        projectedInventoryCost: "5000.0000",
        decision: {
          headline: "1 input type has a shortage.",
          supportingText: "100 total units must be acquired before production.",
          tone: "warning",
        },
        validation: { fields: [], blockers: [] },
        warnings: [],
        completeness: {
          materials: "complete",
          duration: "complete",
          pricing: "complete",
          installation: "notConfigured",
          inventoryCost: "complete",
          profitability: "qualified",
        },
        profitabilityBasis: {
          includedCosts: ["projectedInventoryCost"],
          excludedCosts: ["installationCost", "marketFees", "salesTax", "hauling"],
        },
        calculationEvidence: {
          profitMarginPercent: "87.2",
          systemCostIndexPercent: null,
          materialCost: {
            complete: true,
            baseMarketValue: "128000.0000",
            afterBlueprintMe: "128000.0000",
            afterStructure: "128000.0000",
            adjustedMaterialCost: "128000.0000",
            blueprintMultiplier: "1.0000",
            structureMultiplier: "1.0000",
            rigMultiplier: "1.0000",
            requirementTraces: [],
          },
          durationSteps: [],
        },
          worksheet: worksheetFixture(
            candidateRequest.runs * 1000,
            900,
            1,
            (candidateRequest.componentResolutions ?? []).some((resolution: { typeId: number }) => resolution.typeId === 34),
          ),
        });
      }
      throw new Error(`Unexpected request: ${init?.method ?? "GET"} ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/builds/new?blueprintTypeId=6830");
    expect(await screen.findByRole("heading", { name: "Rifter build" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Edit Build name" }));
    expect(screen.getByLabelText("Build name")).toHaveValue("Rifter build");
    await user.click(screen.getByRole("button", { name: "Cancel Build name edit" }));
    expect(screen.queryByRole("dialog", { name: "Select a product" })).not.toBeInTheDocument();

    await user.click(await screen.findByRole("button", { name: "Edit blueprint" }));
    expect(await screen.findByRole("button", { name: "Use available blueprint" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("combobox", { name: "Available blueprint" })).toHaveTextContent(/30 licensed runs/);
    await user.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/build-plans/candidate-preview"),
      expect.objectContaining({ method: "POST" }),
    ));
    expect(await screen.findByText("1 input type has a shortage.")).toBeInTheDocument();
    const profitability = screen.getByRole("region", { name: "Candidate summary" });
    expect(within(profitability).getByText("Profit margin")).toBeInTheDocument();
    expect(within(profitability).getByText("87.2%")).toBeInTheDocument();
    expect(screen.queryByText("Exact calculation evidence")).not.toBeInTheDocument();
    await user.click(within(profitability).getByRole("button", { name: "View revenue and profit calculation" }));
    const evidenceDialog = screen.getByRole("dialog", { name: "Calculation Evidence - Revenue and Profit" });
    expect(evidenceDialog).toBeInTheDocument();
    for (const section of [
      "Profit Calculation",
      "Assumptions",
      "Material Cost",
      "Installation Cost",
      "Duration",
      "Excluded From Estimated Profit",
    ]) {
      expect(within(evidenceDialog).getByRole("region", { name: section })).toBeInTheDocument();
    }
    expect(within(evidenceDialog).getByText("Exact formulas and provenance").closest("details")).not.toHaveAttribute("open");
    await user.click(screen.getByRole("button", { name: "Close calculation evidence" }));
    const tritaniumRow = screen.getByRole("row", { name: /Tritanium/ });
    expect(tritaniumRow).toHaveTextContent("1,000");
    expect(tritaniumRow).toHaveTextContent("900");
    await user.click(tritaniumRow);
    const inspector = screen.getByRole("complementary", { name: "Tritanium" });
    expect(inspector).toHaveTextContent("5,000");
    expect(screen.getByTestId("app-right-rail")).toContainElement(inspector);
    if (screen.queryByRole("radio", { name: "Pricing policy override" })) {
      await user.click(screen.getByRole("radio", { name: "Pricing policy override" }));
      expect(screen.getByRole("table", { name: "Production worksheet" })).toBeInTheDocument();
    }

    await user.click(await screen.findByRole("radio", { name: "Build" }));
    await waitFor(() => {
      const previewCalls = fetchMock.mock.calls.filter(([input]) =>
        String(input).endsWith("/api/build-plans/candidate-preview")
      );
      const latestRequest = JSON.parse(String(previewCalls.at(-1)?.[1]?.body));
      expect(latestRequest.componentResolutions).toEqual([
        { typeId: 34, recipe: { mode: "manufacturing", blueprintTypeId: 90002 } },
      ]);
    });
    await waitFor(() => {
      const tritaniumRow = screen.getByRole("row", { name: /Tritanium/ });
      expect(within(tritaniumRow).getByText("Build")).toBeInTheDocument();
    });
    expect(screen.getByRole("radio", { name: "Build" })).toBeChecked();

    // Opening Build Settings while a worksheet row is selected swaps the
    // inspector -- the two modes share one surface and are mutually exclusive.
    await user.click(screen.getByRole("button", { name: "Edit build settings" }));
    expect(screen.queryByRole("complementary", { name: "Tritanium" })).not.toBeInTheDocument();
    const settingsInspector = screen.getByRole("complementary", { name: "Build settings" });
    expect(screen.getByTestId("app-right-rail")).toContainElement(settingsInspector);
    // A live setting change updates the preview while the inspector stays open.
    await user.selectOptions(screen.getByLabelText("Manufacturing Facility"), facility.id);
    expect(await screen.findByText(/372,548 ISK/)).toBeInTheDocument();
    await waitFor(() => {
      const previewCalls = fetchMock.mock.calls.filter(([input]) =>
        String(input).endsWith("/api/build-plans/candidate-preview")
      );
      const latestRequest = JSON.parse(String(previewCalls.at(-1)?.[1]?.body));
      expect(latestRequest.manufacturingFacility.estimatedItemValue).toBe("372548.0000");
    });
    expect(screen.getByRole("complementary", { name: "Build settings" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close inspector" }));
    expect(screen.getByTestId("app-right-rail")).toBeEmptyDOMElement();

    await user.click(await screen.findByRole("button", { name: "Edit blueprint" }));
    await user.click(await screen.findByRole("combobox", { name: "Available blueprint" }));
    await user.click(await screen.findByRole("option", { name: /Blueprint Original/ }));
    await user.click(screen.getByRole("button", { name: "Done" }));
    const runsInput = screen.getByLabelText("Runs");
    expect(runsInput).toHaveAttribute("type", "number");
    expect(runsInput).toHaveAttribute("min", "1");
    expect(runsInput).toHaveAttribute("max", "1000000");
    expect(runsInput).toHaveAttribute("step", "1");
    await user.clear(runsInput);
    await user.type(runsInput, "31");
    await waitFor(() => {
      const previewCalls = fetchMock.mock.calls.filter(([input]) =>
        String(input).endsWith("/api/build-plans/candidate-preview")
      );
      const latestRequest = JSON.parse(String(previewCalls.at(-1)?.[1]?.body));
      expect(latestRequest.runs).toBe(31);
    });
    expect(await within(screen.getByRole("row", { name: /Tritanium/ })).findByText("31,000")).toBeInTheDocument();
  });

  test("creates and plans a reaction Build end-to-end", async () => {
    const source = {
      id: "source-1",
      workspaceId: "workspace-1",
      name: "Jita 4-4",
      kind: "manual",
      revision: 1,
      items: [
        { typeId: 37, typeName: "Isogen", price: "180.0000", note: "" },
        { typeId: 30306, typeName: "Methanofullerene", price: "3000.0000", note: "" },
      ],
    };
    let createBuildRequestBody: Record<string, unknown> | null = null;
    let planBuildRequestBody: Record<string, unknown> | null = null;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/sde")) return mockResponse({
        configured: true,
        active: {
          importId: "import-1",
          sourceVersion: "123456",
          sourceLabel: "fixture.zip",
          sourceChecksum: "abc",
          completedAt: "2026-07-25T12:00:00Z",
          counts: { types: 4, blueprints: 1, materialLines: 1, productLines: 1, skippedBlueprints: 0 },
        },
      });
      if (url.endsWith("/api/price-sources")) return mockResponse([source]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.endsWith("/api/builds") && (!init?.method || init.method === "GET")) return mockResponse([]);
      if (url.includes("/api/blueprints/search")) return mockResponse([]);
      if (url.includes("/api/reaction-formulas/search")) return mockResponse([{
        reactionFormulaTypeId: 46157,
        reactionFormulaName: "Methanofullerene Reaction Formula",
        productTypeId: 30306,
        productName: "Methanofullerene",
        groupName: "Composite Reaction Formulas",
        published: true,
      }]);
      if (url.includes("/api/reaction-formulas/46157/plan")) return mockResponse({
        reactionFormulaTypeId: 46157,
        reactionFormulaName: "Methanofullerene Reaction Formula",
        runs: 1,
        durationSeconds: 10800,
        materials: [{ typeId: 37, typeName: "Isogen", quantityPerRun: 300, totalQuantity: 300 }],
        products: [{ typeId: 30306, typeName: "Methanofullerene", quantityPerRun: 160, totalQuantity: 160 }],
      });
      if (url.endsWith("/api/build-plans/candidate-preview")) {
        return mockResponse({
          candidateFingerprint: "create-candidate",
          canPlan: true,
          candidate: {
            runs: 1,
            recipeFingerprint: "formula",
            priceSourceId: source.id,
            priceSourceName: source.name,
            priceSourceRevision: source.revision,
            priceLines: source.items,
            pricingComplete: true,
            estimatedMaterialCost: "54000.0000",
            expectedRevenue: "480000.0000",
            estimatedMargin: "426000.0000",
            missingPriceCount: 0,
            active: true,
            plannedAt: "2026-08-02T10:00:00Z",
            supersededAt: null,
            materialLines: [{
              typeId: 37, typeName: "Isogen", quantityPerRun: 300, totalQuantity: 300,
              unitPrice: "180.0000", lineTotal: "54000.0000", missing: false,
            }],
            facility: null,
          },
          coverage: {
            buildId: "preview-build",
            ownerId: "owner-1",
            buildStatus: "draft",
            buildRevision: 1,
            recipeFingerprint: "formula",
            runs: 1,
            completeQuantityCoverage: false,
            completeCostCoverage: true,
            hasActiveReservations: false,
            canReserve: false,
            canRefreshReservations: false,
            canReleaseReservations: false,
            canStartProduction: false,
            materialLines: [],
            warnings: [],
          },
          projectedInventoryCost: "0.0000",
          decision: { headline: "Ready to plan.", supportingText: "", tone: "success" },
          validation: { fields: [], blockers: [] },
          warnings: [],
          completeness: {
            materials: "complete", duration: "complete", pricing: "complete",
            installation: "notConfigured", inventoryCost: "complete", profitability: "complete",
          },
          profitabilityBasis: { includedCosts: [], excludedCosts: [] },
          calculationEvidence: {
            profitMarginPercent: "88.75", systemCostIndexPercent: null,
            materialCost: {
              complete: true, baseMarketValue: "54000.0000", afterBlueprintMe: "54000.0000",
              afterStructure: "54000.0000", adjustedMaterialCost: "54000.0000",
              blueprintMultiplier: "1.0000", structureMultiplier: "1.0000", rigMultiplier: "1.0000",
              requirementTraces: [],
            },
            durationSteps: [],
          },
          worksheet: worksheetFixture(300, 300),
        });
      }
      const reactionBuild = {
        id: "reaction-build-1",
        workspaceId: "workspace-1",
        ownerId: "owner-1",
        name: "Methanofullerene build",
        recipe: {
          kind: "reaction",
          sourceSdeDatasetId: "sde-1",
          sourceSdeVersion: "3389399",
          reactionFormulaTypeId: 46157,
          reactionFormulaName: "Methanofullerene Reaction Formula",
          durationSecondsPerRun: 10800,
          materials: [{ typeId: 37, typeName: "Isogen", quantityPerRun: 300, sortOrder: 0 }],
          products: [{ typeId: 30306, typeName: "Methanofullerene", quantityPerRun: 160, sortOrder: 0 }],
          fingerprint: "formula",
        },
        runs: 1,
        notes: "",
        revision: 1,
        createdAt: "2026-08-02T10:00:00Z",
        updatedAt: "2026-08-02T10:00:00Z",
        recipeCurrency: "current",
        activeSdeVersion: "3389399",
      };
      if (url.endsWith("/api/builds") && init?.method === "POST") {
        createBuildRequestBody = JSON.parse(String(init.body));
        return mockResponse(reactionBuild, true, 201);
      }
      if (url.endsWith("/api/builds/reaction-build-1") && (!init?.method || init.method === "GET")) {
        return mockResponse(reactionBuild);
      }
      if (url.endsWith("/api/builds/reaction-build-1") && init?.method === "PUT") {
        return mockResponse({ ...reactionBuild, revision: reactionBuild.revision + 1 });
      }
      if (url.endsWith("/api/builds/reaction-build-1/orders/preview") && init?.method === "POST") {
        return mockResponse({ reuse: [{ typeId: 16634, typeName: "Hydrocarbons", quantity: 120 }] });
      }
      if (url.endsWith("/api/builds/reaction-build-1/orders") && init?.method === "POST") {
        planBuildRequestBody = JSON.parse(String(init.body)) as Record<string, unknown>;
        return mockResponse({
          id: "order-1",
          workspaceId: "workspace-1",
          ownerId: "owner-1",
          sourceBuildId: "reaction-build-1",
          sourceBuildRevision: 1,
          displayName: "Methanofullerene build",
          runs: 1,
          recipeFingerprint: "formula",
          priceSnapshotId: "snapshot-1",
          estimatedMaterialCost: "54000.0000",
          expectedRevenue: "480000.0000",
          estimatedMargin: "426000.0000",
          missingPriceCount: 0,
          createdAt: "2026-08-02T10:00:00Z",
          updatedAt: "2026-08-02T10:00:00Z",
          startedAt: null,
          completedAt: null,
          canceledAt: null,
          archivedAt: null,
          status: "blocked",
          rollup: { satisfied: 0, needsAction: 1, inProgress: 0, total: 1 },
          requirements: [],
        }, true, 201);
      }
      if (url.endsWith("/api/orders/order-1")) {
        return mockResponse({
          id: "order-1",
          workspaceId: "workspace-1",
          ownerId: "owner-1",
          sourceBuildId: "reaction-build-1",
          sourceBuildRevision: 1,
          displayName: "Methanofullerene build",
          runs: 1,
          recipeFingerprint: "formula",
          priceSnapshotId: "snapshot-1",
          estimatedMaterialCost: "54000.0000",
          expectedRevenue: "480000.0000",
          estimatedMargin: "426000.0000",
          missingPriceCount: 0,
          createdAt: "2026-08-02T10:00:00Z",
          updatedAt: "2026-08-02T10:00:00Z",
          startedAt: null,
          completedAt: null,
          canceledAt: null,
          archivedAt: null,
          status: "blocked",
          rollup: { satisfied: 0, needsAction: 1, inProgress: 0, total: 1 },
          requirements: [],
        });
      }
      throw new Error(`Unexpected request: ${init?.method ?? "GET"} ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/builds/new");

    expect(await screen.findByRole("dialog", { name: "Select a product" })).toBeInTheDocument();
    await user.type(await screen.findByLabelText("Product"), "methano");
    const result = await screen.findByRole("button", { name: /Methanofullerene.*Methanofullerene Reaction Formula/s });
    await user.click(result);
    expect(screen.queryByRole("dialog", { name: "Select a product" })).not.toBeInTheDocument();

    expect(await screen.findByRole("img", { name: "Methanofullerene Reaction Formula" })).toHaveAttribute(
      "src",
      expect.stringContaining("/bp?size=64"),
    );
    expect(screen.queryByRole("button", { name: "Edit blueprint" })).not.toBeInTheDocument();
    expect(screen.getByText(/Methanofullerene Reaction Formula/)).toBeInTheDocument();

    await waitFor(
      () => expect(screen.queryByText("Candidate results will appear here.")).not.toBeInTheDocument(),
      { timeout: 3000 },
    );

    // Autosave (~800ms debounce) persists the draft and navigates from
    // /builds/new to /builds/reaction-build-1 -- "Create Epic" only
    // appears once the Build is actually saved (editor.initialBuild set).
    await waitFor(() => expect(createBuildRequestBody).not.toBeNull(), { timeout: 3000 });
    expect(createBuildRequestBody).toMatchObject({
      recipe: { mode: "reaction", reactionFormulaTypeId: 46157 },
    });
    expect(JSON.stringify(createBuildRequestBody)).not.toContain("blueprintTypeId");

    const createEpicButton = await screen.findByRole("button", { name: "Create Epic" }, { timeout: 3000 });
    await user.click(createEpicButton);

    // The dialog previews the inventory the Epic uses, with Reserve on.
    const dialog = await screen.findByRole("dialog", { name: "Create Epic" });
    expect(await within(dialog).findByText("Hydrocarbons")).toBeInTheDocument();
    expect(within(dialog).getByRole("checkbox", { name: /Reserve inventory/ })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Create Epic" }));

    await waitFor(() => expect(planBuildRequestBody).not.toBeNull());
    expect(planBuildRequestBody).toMatchObject({
      buildId: "reaction-build-1",
      reservation: { expectedReuse: [{ typeId: 16634, quantity: 120 }] },
    });

    // "Create Epic" lands on the Board with the new Epic's inspector
    // already open (`/board?epic=order-1`) rather than a separate Order
    // page -- LocationProbe only surfaces the pathname, not the
    // query string.
    expect(await screen.findByTestId("location")).toHaveTextContent("/board");
  });

  test("the product selection dialog searches manufacturing and reaction results together", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/sde")) return mockResponse({
        configured: true,
        active: {
          importId: "import-1",
          sourceVersion: "123456",
          sourceLabel: "fixture.zip",
          sourceChecksum: "abc",
          completedAt: "2026-07-25T12:00:00Z",
          counts: { types: 4, blueprints: 1, materialLines: 1, productLines: 1, skippedBlueprints: 0 },
        },
      });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.endsWith("/api/builds")) return mockResponse([]);
      if (url.includes("/api/blueprints/search")) return mockResponse([{
        blueprintTypeId: 6830,
        blueprintName: "Rifter Blueprint",
        productTypeId: 5876,
        productName: "Rifter",
        groupName: "Frigate",
        published: true,
        manufacturingAvailable: true,
      }]);
      if (url.includes("/api/reaction-formulas/search")) return mockResponse([{
        reactionFormulaTypeId: 46157,
        reactionFormulaName: "Methanofullerene Reaction Formula",
        productTypeId: 30306,
        productName: "Methanofullerene",
        groupName: "Composite Reaction Formulas",
        published: true,
      }]);
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/builds/new");

    expect(await screen.findByRole("dialog", { name: "Select a product" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Role")).not.toBeInTheDocument();
    await user.type(await screen.findByLabelText("Product"), "re");

    await screen.findByRole("button", { name: /Rifter.*Rifter Blueprint.*Manufacturing/s });
    await screen.findByRole("button", { name: /Methanofullerene.*Methanofullerene Reaction Formula.*Reaction/s });
    expect(fetchMock.mock.calls.some(([input]) => String(input).includes("/api/blueprints/search?q=re"))).toBe(true);
    expect(fetchMock.mock.calls.some(([input]) => String(input).includes("/api/reaction-formulas/search?q=re"))).toBe(true);
  });
});

describe("Manual Price Source", () => {
  test("searches items automatically and populates the selected item", async () => {
    const source = {
      id: "source-1",
      workspaceId: "workspace-1",
      name: "Manual Prices",
      description: "",
      kind: "manual",
      revision: 1,
      itemCount: 0,
      recentBuildCount: 0,
      items: [],
      createdAt: "2026-07-27T10:00:00Z",
      updatedAt: "2026-07-27T10:00:00Z",
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/price-sources/source-1")) return mockResponse(source);
      if (url.includes("/api/types/search?q=Tr")) {
        return mockResponse([{
          typeId: 34,
          typeName: "Tritanium",
          groupName: "Mineral",
          published: true,
        }]);
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/prices/source-1");

    await user.type(await screen.findByLabelText("Find an EVE item"), "Tr");
    const result = await screen.findByRole("button", { name: /Tritanium/ });
    expect(within(result).getByRole("img", { name: "Tritanium" })).toBeInTheDocument();
    await user.click(result);
    expect(screen.getByLabelText("EVE type ID")).toHaveValue("34");
    expect(screen.getByLabelText("Item name")).toHaveValue("Tritanium");
  });

  test("selects a price row into the edit form and removes it", async () => {
    const initialSource = {
      id: "source-1",
      workspaceId: "workspace-1",
      name: "Manual Prices",
      description: "",
      kind: "manual",
      revision: 1,
      itemCount: 1,
      recentBuildCount: 0,
      items: [
        { typeId: 34, typeName: "Tritanium", price: "5.5000", note: "Jita buy", updatedAt: "2026-07-27T10:00:00Z" },
      ],
      createdAt: "2026-07-27T10:00:00Z",
      updatedAt: "2026-07-27T10:00:00Z",
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/price-sources/source-1")) return mockResponse(initialSource);
      if (init?.method === "DELETE" && url.includes("/api/price-sources/source-1/items/34")) {
        return mockResponse({ ...initialSource, itemCount: 0, items: [] });
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/prices/source-1");

    expect(await screen.findByRole("table", { name: "Prices" })).toBeInTheDocument();
    await user.click(screen.getByRole("row", { name: /Tritanium/ }));

    expect(await screen.findByRole("heading", { name: "Editing Tritanium" })).toBeInTheDocument();
    expect(screen.getByLabelText("EVE type ID")).toHaveValue("34");
    expect(screen.getByLabelText("EVE type ID")).toBeDisabled();
    // MoneyInput re-groups the committed value for display and drops
    // insignificant trailing zeros ("5.5000" -> "5.5").
    expect(screen.getByLabelText("Unit price (ISK)")).toHaveValue("5.5");

    await user.click(screen.getByRole("button", { name: "Remove this price" }));

    expect(await screen.findByText("No matching prices")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/price-sources/source-1/items/34"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});

describe("Build detail planning", () => {
  test("an Epic link opens the Build's Plan in that Epic's read-only view", async () => {
    const draft = { ...plannedBuildFixture(), status: "draft", activePlanId: null, plans: [] };
    const epicSummary = {
      id: "epic-1",
      workspaceId: "workspace-1",
      ownerId: "owner-1",
      sourceBuildId: "build-ready",
      sourceBuildRevision: draft.revision,
      displayName: "Manufacture Rifter",
      runs: 1,
      recipeFingerprint: "recipe",
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
    };
    const epicDetail = {
      ...epicSummary,
      requirements: [{
        id: "req-trit",
        orderId: "epic-1",
        typeId: 34,
        capturedName: "Tritanium",
        kind: "buy",
        sourceBuildId: null,
        requiredQuantity: 32000,
        fulfillmentScope: "missing",
        reusedQuantity: 20000,
        freshQuantity: 12000,
        estimatedUnitCost: null,
        estimatedLineTotal: null,
        reusedLineTotal: null,
        state: "needsAction",
        linkedTickets: [],
        operationOccurrenceKey: "root:build-ready",
      }],
      productionPlan: {
        rootOccurrenceKey: "root:build-ready",
        dependencies: [],
        operations: [{
          id: "op-root",
          occurrenceKey: "root:build-ready",
          parentOccurrenceKey: null,
          buildId: "build-ready",
          productTypeId: 587,
          productName: "Rifter",
          runs: 1,
          producedQuantity: 1,
          stage: 0,
          ticketId: "ticket-1",
          ticketDisplayId: "T-1",
          ticketStatus: "todo",
          servedRequirementIds: [],
        }],
      },
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/builds/build-ready")) return mockResponse(draft);
      if (url.endsWith("/api/orders")) return mockResponse([epicSummary]);
      if (url.endsWith("/api/orders/epic-1")) return mockResponse(epicDetail);
      if (url.endsWith("/api/orders/epic-1/execution-plan")) return mockResponse({
        plan: {
          rootNodeId: "root:build-ready",
          stages: [{ index: 0, nodeIds: ["root:build-ready"] }],
          nodes: [{
            id: "root:build-ready", outputTypeId: 587, outputTypeName: "Rifter", activity: "manufacturing", stage: 0,
            occurrenceIds: ["root:build-ready"], facilityId: null, facilityName: null, effectiveMe: null, effectiveTe: null,
            requiredQuantity: 0, plannedInventoryQuantity: 0, productionDemand: 0, projectedOutput: 1, projectedRuns: 1,
            retainedSurplusQuantity: 0, retainedSurplusCost: null, materialComponentCost: null, ownInstallationCost: null,
            totalProductionCost: null, costComplete: true, consumers: [], productionMethods: [], unitProductionCost: null,
            availableQuantity: 0,
          }],
          edges: [],
          occurrences: [{ id: "root:build-ready", nodeId: "root:build-ready", isRoot: true, stage: 0, requirements: [] }],
          acquisitions: [{
            typeId: 34, typeName: "Tritanium", requiredQuantity: 32000, plannedInventoryQuantity: 20000,
            shortageQuantity: 12000, availableQuantity: 0, reservedQuantity: 0, sourceStrategy: "buy", consumers: [],
            productionMethods: [], freshCost: null, freshUnitPrice: null, freshPriceStale: false,
          }],
          unresolved: [],
          complete: true,
          warnings: [],
          generatedAt: "2026-10-08T10:00:00Z",
          logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
        },
        epic: {
          orderId: "epic-1",
          displayName: "Manufacture Rifter",
          sourceBuildRevision: draft.revision,
          nodes: { "root:build-ready": { ticketId: "ticket-1", ticketDisplayId: "T-1", ticketStatus: "todo", output: { reserved: 0, consumed: 0, remainingNeed: 0 } } },
          acquisitions: { "34": { reserved: 20000, consumed: 0, remainingNeed: 12000 } },
        },
      });
      if (url.endsWith("/api/sde")) return mockResponse({ active: { sourceVersion: "test" } });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
      if (url.includes("/api/blueprints/691/plan?runs=1")) return mockResponse({
        blueprintTypeId: 691,
        blueprintName: "Rifter Blueprint",
        runs: 1,
        durationSeconds: 6000,
        materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000 }],
        products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
      });
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderApp("/builds/build-ready?view=plan&epic=epic-1");

    expect(await screen.findByText("Epic: Manufacture Rifter")).toBeInTheDocument();
    // The Plan's own layout, showing the Epic's data.
    const inputs = screen.getByRole("table", { name: "Inputs to Source" });
    const tritanium = within(inputs).getByText("Tritanium").closest("tr")!;
    expect(within(tritanium).getByText("20,000 reserved · 0 used")).toBeInTheDocument();
    expect(within(tritanium).getByText("12,000")).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Final Production" })).toBeInTheDocument();
    expect(screen.getByText("T-1 · To do")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Epic" })).toHaveValue("epic-1");
    expect(screen.getByLabelText("Runs")).toBeDisabled();
    // The Build itself can't be edited while an Epic is shown.
    expect(screen.getByRole("button", { name: /Edit build settings/ })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Edit blueprint" })).not.toBeInTheDocument();
  });

  test("opens a saved draft in the worksheet planner", async () => {
    const draft = {
      ...plannedBuildFixture(),
      status: "draft",
      activePlanId: null,
      plans: [],
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/builds/build-ready")) return mockResponse(draft);
      if (url.endsWith("/api/builds/build-ready/name") && init?.method === "PATCH") {
        const request = JSON.parse(String(init.body));
        return mockResponse({ ...draft, name: request.name });
      }
      if (url.endsWith("/api/sde")) return mockResponse({ active: { sourceVersion: "test" } });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
      if (url.includes("/api/blueprints/691/plan?runs=1")) return mockResponse({
        blueprintTypeId: 691,
        blueprintName: "Rifter Blueprint",
        runs: 1,
        durationSeconds: 6000,
        materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000 }],
        products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
      });
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/builds/build-ready/edit");

    expect(await screen.findByRole("heading", { name: draft.name })).toBeInTheDocument();
    expect(screen.getByText("Draft")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Edit Build name" }));
    expect(screen.getByLabelText("Build name")).toHaveValue(draft.name);
    await user.keyboard("{Escape}");
    expect(screen.queryByLabelText("Build name")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Edit Build name" }));
    const nameInput = screen.getByLabelText("Build name");
    await user.clear(nameInput);
    await user.type(nameInput, "Rifter materials");
    await user.click(screen.getByRole("button", { name: "Save Build name" }));
    expect(await screen.findByRole("heading", { name: "Rifter materials" })).toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([input, request]) =>
      String(input).endsWith("/api/builds/build-ready/name") && request?.method === "PATCH"
    )).toBe(true);
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create Epic" })).toBeInTheDocument();
    expect(screen.queryByText("Edit Draft")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Replace blueprint")).not.toBeInTheDocument();
  });

  test("exports a verification workbook from the current editor overlay", async () => {
    const draft = {
      ...plannedBuildFixture(),
      status: "draft",
      activePlanId: null,
      plans: [],
    };
    let exportBody: unknown = null;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/builds/build-ready")) return mockResponse(draft);
      if (url.endsWith("/api/sde")) return mockResponse({ active: { sourceVersion: "test" } });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
      if (url.includes("/api/blueprints/691/plan?runs=1")) return mockResponse({
        blueprintTypeId: 691,
        blueprintName: "Rifter Blueprint",
        runs: 1,
        durationSeconds: 6000,
        materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000 }],
        products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
      });
      if (url.endsWith("/api/builds/build-ready/export-verification") && init?.method === "POST") {
        exportBody = JSON.parse(String(init.body));
        return {
          ok: true,
          status: 200,
          headers: {
            get: (name: string) =>
              name.toLowerCase() === "content-disposition"
                ? 'attachment; filename="Rifter-verification.xlsx"'
                : null,
          },
          blob: async () => new Blob(["PK"], { type: "application/octet-stream" }),
          json: async () => ({}),
        };
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const createObjectURL = vi.fn(() => "blob:mock");
    const revokeObjectURL = vi.fn();
    // Subclass rather than spread: the router still needs a working `new URL()`.
    vi.stubGlobal("URL", class extends URL {
      static createObjectURL = createObjectURL;
      static revokeObjectURL = revokeObjectURL;
    });
    const anchorClick = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => {});
    const user = userEvent.setup();

    renderApp("/builds/build-ready/edit");

    const exportButton = await screen.findByRole(
      "button",
      { name: "Export verification workbook" },
      { timeout: 3000 },
    );
    await waitFor(() => expect(exportButton).toBeEnabled(), { timeout: 3000 });
    await user.click(exportButton);

    await waitFor(() => expect(createObjectURL).toHaveBeenCalledTimes(1), { timeout: 3000 });
    expect(exportBody).toMatchObject({ runs: expect.any(Number) });
    expect(anchorClick).toHaveBeenCalled();
    anchorClick.mockRestore();
  });

  test("autosaves a draft edit without a Save button, and not on load", async () => {
    const draft = {
      ...plannedBuildFixture(),
      status: "draft",
      activePlanId: null,
      plans: [],
    };
    let putCount = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/builds/build-ready") && (!init?.method || init.method === "GET")) return mockResponse(draft);
      if (url.endsWith("/api/builds/build-ready") && init?.method === "PUT") {
        putCount += 1;
        const request = JSON.parse(String(init.body));
        return mockResponse({ ...draft, notes: request.notes, revision: draft.revision + 1 });
      }
      if (url.endsWith("/api/sde")) return mockResponse({ active: { sourceVersion: "test" } });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
      if (url.includes("/api/blueprints/691/plan?runs=1")) return mockResponse({
        blueprintTypeId: 691,
        blueprintName: "Rifter Blueprint",
        runs: 1,
        durationSeconds: 6000,
        materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000 }],
        products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
      });
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/builds/build-ready/edit");

    expect(await screen.findByRole("heading", { name: draft.name })).toBeInTheDocument();
    // The recipe plan fetch (which makes the draft "ready" to autosave)
    // settles shortly after load -- give it time, then confirm loading an
    // already-saved draft never fires a spurious PUT on its own.
    await new Promise((resolve) => setTimeout(resolve, 1000));
    expect(putCount).toBe(0);

    await user.type(screen.getByLabelText("Notes"), "Reserve materials before Friday.");

    await waitFor(() => expect(putCount).toBeGreaterThan(0), { timeout: 3000 });
    expect(await screen.findByText("All changes saved")).toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([input, request]) =>
      String(input).endsWith("/api/builds/build-ready") && request?.method === "PUT"
      && JSON.parse(String(request.body)).notes === "Reserve materials before Friday."
    )).toBe(true);
  });

  test("does not show Build-not-saved validation while a saved Build recipe hydrates", async () => {
    const draft = {
      ...plannedBuildFixture(),
      status: "draft",
      activePlanId: null,
      plans: [],
    };
    let resolveRecipe!: (response: ReturnType<typeof mockResponse>) => void;
    const recipeResponse = new Promise<ReturnType<typeof mockResponse>>((resolve) => { resolveRecipe = resolve; });
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/builds/build-ready") && (!init?.method || init.method === "GET")) {
        return mockResponse(draft);
      }
      if (url.endsWith("/api/sde")) return mockResponse({ active: { sourceVersion: "test" } });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
      if (url.includes("/api/blueprints/691/plan?runs=1")) return recipeResponse;
      if (url.includes("/linked-builds")) return mockResponse([]);
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderApp("/builds/build-ready");
    expect(await screen.findByRole("heading", { name: draft.name })).toBeInTheDocument();
    expect(screen.queryByText("Build not saved")).not.toBeInTheDocument();
    expect(screen.queryByText("Name, blueprint, and valid runs are required.")).not.toBeInTheDocument();

    resolveRecipe(mockResponse({
      blueprintTypeId: 691,
      blueprintName: "Rifter Blueprint",
      runs: 1,
      durationSeconds: 6000,
      materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000 }],
      products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
    }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Create Epic" })).toBeEnabled());
    expect(screen.queryByText("Build not saved")).not.toBeInTheDocument();
  });

  test("toggling to 'use available blueprint' with none observed yet does not crash the worksheet", async () => {
    const draft = {
      ...plannedBuildFixture(),
      status: "draft",
      activePlanId: null,
      plans: [],
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/builds/build-ready") && (!init?.method || init.method === "GET")) return mockResponse(draft);
      if (url.endsWith("/api/sde")) return mockResponse({ active: { sourceVersion: "test" } });
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
      if (url.includes("/api/blueprints/691/plan?runs=1")) return mockResponse({
        blueprintTypeId: 691,
        blueprintName: "Rifter Blueprint",
        runs: 1,
        durationSeconds: 6000,
        materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000 }],
        products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
      });
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/builds/build-ready");

    expect(await screen.findByRole("heading", { name: draft.name })).toBeInTheDocument();
    // Let the recipe plan fetch settle so the worksheet -- and the autosave
    // snapshot computed alongside it on every render -- has real material
    // data to work with, matching how a user actually hits this (not within
    // the first render, after the recipe has already loaded).
    await new Promise((resolve) => setTimeout(resolve, 1000));

    await user.click(screen.getByRole("button", { name: "Edit blueprint" }));
    await user.click(screen.getByRole("button", { name: "Use available blueprint" }));
    expect(screen.getByText("No available blueprints")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Done" }));

    // Regression: switching to "observedAsset" mode before picking one used
    // to throw inside the render-time autosave snapshot, crashing the whole
    // worksheet with no error boundary to catch it.
    expect(screen.getByRole("heading", { name: draft.name })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create Epic" })).toBeInTheDocument();
  });
});

describe("Inventory accounting", () => {
  test("shows the empty state and opens the opening-balance workflow", async () => {
    mockFetchSequence([configuredWorkspace, [], []]);
    const user = userEvent.setup();

    renderApp("/inventory");

    expect(await screen.findByText("No inventory recorded")).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Add Opening Balance" })[0]);
    expect(await screen.findByRole("heading", { name: "Add Opening Balance" })).toBeInTheDocument();
    expect(screen.getByLabelText("Unit cost (ISK)")).toBeInTheDocument();
    expect(screen.queryByText("Zero cost")).not.toBeInTheDocument();
  });

  test("searches inventory items automatically and shows item images", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/inventory")) return mockResponse([]);
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.includes("/api/types/search?q=Tr")) {
        return mockResponse([{
          typeId: 34,
          typeName: "Tritanium",
          groupName: "Mineral",
          published: true,
        }]);
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/inventory?intent=opening-balance");

    await user.type(await screen.findByLabelText("EVE item"), "Tr");
    expect(screen.queryByRole("button", { name: "Search items" })).not.toBeInTheDocument();
    const result = await screen.findByRole("button", { name: /Tritanium/ });
    expect(within(result).getByRole("img", { name: "Tritanium" })).toBeInTheDocument();
    await user.click(result);
    expect(screen.getByText("Tritanium")).toBeInTheDocument();
  });

  test("renders exact inventory values and warnings", async () => {
    const user = userEvent.setup();
    mockFetchSequence([
      configuredWorkspace,
      [{
        balance: {
          key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
          typeName: "Tritanium",
          quantity: 150,
          totalHistoricalCost: "1800.0000",
          averageUnitCost: "12.0000",
          revision: 2,
          lastActivityAt: "2026-07-25T12:00:00Z",
        },
        groupName: "Mineral",
        reservedQuantity: 40,
        availableQuantity: 110,
        costQuality: "known",
        currentPrice: "13.0000",
        currentValue: "1950.0000",
        historicalDifference: "150.0000",
        historicalComparisonComplete: true,
        priceSourceId: "source-1",
        priceSourceName: "Home Market",
        priceSourceUpdatedAt: "2026-07-25T12:00:00Z",
        warnings: [],
      }],
      [],
    ]);

    renderApp("/inventory");

    await user.click(await screen.findByRole("button", { name: "Expand Mineral" }));
    expect(await screen.findByText("Tritanium")).toBeInTheDocument();
    expect(screen.getByTitle("12 ISK")).toBeInTheDocument();
    expect(screen.getByTitle("1,800 ISK")).toBeInTheDocument();
    expect(screen.getByTitle("1,950 ISK")).toBeInTheDocument();
    expect(screen.getByText("40")).toBeInTheDocument();
    expect(screen.getByText("110")).toBeInTheDocument();
  });

  test("imports a bulk inventory export file and refreshes the list", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (init?.method === "POST" && url.endsWith("/api/inventory/import")) {
        return mockResponse({
          results: [
            { typeId: 34, typeName: "Tritanium", imported: true, message: null },
            { typeId: 35, typeName: "Pyerite", imported: false, message: "inventory changed while this form was open" },
          ],
        });
      }
      if (url.endsWith("/api/inventory")) return mockResponse([]);
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/inventory");

    await screen.findByText("No inventory recorded");
    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Import" }));

    const dialog = await screen.findByRole("dialog", { name: "Import Inventory" });
    const file = new File(
      [JSON.stringify({
        items: [
          { typeId: 34, typeName: "Tritanium", quantity: 100, averageUnitCost: "4.2500" },
          { typeId: 35, typeName: "Pyerite", quantity: 200, averageUnitCost: "1.0000" },
        ],
      })],
      "export.json",
      { type: "application/json" },
    );
    await user.upload(within(dialog).getByLabelText("Export file"), file);

    const importButton = await within(dialog).findByRole("button", { name: "Import 2 Items" });
    await user.click(importButton);

    expect(await within(dialog).findByText("1 imported")).toBeInTheDocument();
    expect(within(dialog).getByText("1 skipped")).toBeInTheDocument();
    expect(within(dialog).getByText("Tritanium")).toBeInTheDocument();
    expect(within(dialog).getByText("Imported")).toBeInTheDocument();
    expect(within(dialog).getByText("Pyerite")).toBeInTheDocument();
    expect(within(dialog).getByText("inventory changed while this form was open")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("dialog", { name: "Import Inventory" })).not.toBeInTheDocument();

    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining("/api/inventory/import"), expect.anything());
  });

  test("renders inventory as a grouped operational table and opens the inspector panel on row click", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (url.endsWith("/api/inventory")) {
        return mockResponse([
          {
            balance: {
              key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
              typeName: "Tritanium",
              quantity: 150,
              totalHistoricalCost: "1800.0000",
              averageUnitCost: "12.0000",
              revision: 2,
              lastActivityAt: "2026-07-25T12:00:00Z",
            },
            groupName: "Mineral",
            reservedQuantity: 40,
            availableQuantity: 110,
            costQuality: "known",
            currentPrice: "13.0000",
            currentValue: "1950.0000",
            historicalDifference: "150.0000",
            historicalComparisonComplete: true,
            priceSourceId: null,
            priceSourceName: null,
            priceSourceUpdatedAt: null,
            warnings: [],
          },
          {
            balance: {
              key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 587 },
              typeName: "Rifter",
              quantity: 1,
              totalHistoricalCost: "500000.0000",
              averageUnitCost: "500000.0000",
              revision: 1,
              lastActivityAt: "2026-07-25T12:00:00Z",
            },
            groupName: null,
            reservedQuantity: 0,
            availableQuantity: 1,
            costQuality: "known",
            currentPrice: null,
            currentValue: null,
            historicalDifference: null,
            historicalComparisonComplete: true,
            priceSourceId: null,
            priceSourceName: null,
            priceSourceUpdatedAt: null,
            warnings: [],
          },
        ]);
      }
      if (url.endsWith("/api/price-sources")) return mockResponse([]);
      if (url.includes("/api/inventory/34")) {
        return mockResponse({
          balance: {
            key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
            typeName: "Tritanium",
            quantity: 150,
            totalHistoricalCost: "1800.0000",
            averageUnitCost: "12.0000",
            revision: 2,
            lastActivityAt: "2026-07-25T12:00:00Z",
          },
          groupName: "Mineral",
          reservedQuantity: 40,
          availableQuantity: 110,
          costQuality: "known",
          currentPrice: "13.0000",
          currentValue: "1950.0000",
          historicalDifference: "150.0000",
          historicalComparisonComplete: true,
          priceSourceId: null,
          priceSourceName: null,
          priceSourceUpdatedAt: null,
          warnings: [],
          events: [],
          reservations: [],
        });
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/inventory");

    expect(await screen.findByRole("table", { name: "Inventory" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand Mineral" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "Expand Other" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));
    await user.click(screen.getByRole("row", { name: /Tritanium/ }));
    expect(screen.getByTestId("location")).toHaveTextContent("/inventory");
    const panel = screen.getByRole("complementary", { name: "Tritanium" });
    expect(within(panel).getByText("110")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/inventory/34"),
      expect.anything(),
    );

    await user.click(within(panel).getByRole("button", { name: "Close item inspector" }));
    expect(screen.queryByRole("complementary", { name: "Tritanium" })).not.toBeInTheDocument();
  });
});

describe("Facility export/import", () => {
  test("imports a bulk facilities export file and refreshes the list", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/api/auth/session")) return mockResponse(authenticatedSession);
      if (url.endsWith("/api/workspace")) return mockResponse(configuredWorkspace);
      if (init?.method === "POST" && url.endsWith("/api/industry/facilities/import/preview")) {
        return mockResponse({
          items: [
            { index: 0, name: "Raitaru", classification: "new", existingId: null, existingName: null, existingRevision: null, matchBasis: null, message: null },
            { index: 1, name: "Azbel", classification: "new", existingId: null, existingName: null, existingRevision: null, matchBasis: null, message: null },
          ],
        });
      }
      if (init?.method === "POST" && url.endsWith("/api/industry/facilities/import")) {
        return mockResponse({
          results: [
            { name: "Raitaru", status: "created", message: null },
            { name: "Azbel", status: "failed", message: "name must be between 1 to 120 characters" },
          ],
        });
      }
      if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    renderApp("/facilities");

    await screen.findByText("No Facility Profiles");
    await user.click(screen.getByRole("button", { name: "Import" }));

    const dialog = await screen.findByRole("dialog", { name: "Import Facilities" });
    const file = new File(
      [JSON.stringify({
        items: [
          {
            name: "Raitaru",
            kind: "upwellStructure",
            structureId: 1234567890,
            structureTypeId: 35825,
            structureTypeName: "Raitaru",
            solarSystemId: 30000142,
            solarSystemName: "Jita",
            securityClass: "highSec",
            materialReductionPercent: "1.000000",
            timeReductionPercent: "15.000000",
            jobCostReductionPercent: "3.000000",
            facilityTaxPercent: "1.500000",
            sccSurchargePercent: "0.500000",
            allianceSurchargePercent: "0.000000",
            fixedSupplementalCost: "1000.0000",
            manualSystemCostIndex: "0.048000",
            notes: "Home structure",
            rigs: [],
          },
          {
            name: "Azbel",
            kind: "manual",
            structureId: null,
            structureTypeId: null,
            structureTypeName: "",
            solarSystemId: null,
            solarSystemName: "",
            securityClass: "unknown",
            materialReductionPercent: "0",
            timeReductionPercent: "0",
            jobCostReductionPercent: "0",
            facilityTaxPercent: "0",
            sccSurchargePercent: "4",
            allianceSurchargePercent: "0",
            fixedSupplementalCost: "0",
            manualSystemCostIndex: null,
            notes: "",
            rigs: [],
          },
        ],
      })],
      "facilities-export.json",
      { type: "application/json" },
    );
    await user.upload(within(dialog).getByLabelText("Export file"), file);

    const importButton = await within(dialog).findByRole("button", { name: "Import 2 facilities" });
    await user.click(importButton);

    expect(await within(dialog).findByText("Raitaru")).toBeInTheDocument();
    expect(within(dialog).getByText("created")).toBeInTheDocument();
    expect(within(dialog).getByText("Azbel")).toBeInTheDocument();
    expect(within(dialog).getByText(/name must be between 1 to 120 characters/)).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("dialog", { name: "Import Facilities" })).not.toBeInTheDocument();

    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/industry/facilities/import"),
      expect.anything(),
    );
  });
});

describe("EVE observations", () => {
  test("/settings and /settings/eve redirect to the Characters page", async () => {
    mockFetchSequence([configuredWorkspace, []]);

    renderApp("/settings/eve");

    expect(await screen.findByRole("heading", { name: "Characters" })).toBeInTheDocument();
  });
});

function renderApp(initialEntry: string) {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <App />
      <LocationProbe />
    </MemoryRouter>,
  );
}

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}</output>;
}

// `App()`'s session check (GET /api/auth/session) always fires before
// anything in `responses` below -- every caller of this helper predates
// login and queues responses assuming their own first call is the first
// entry. Answering the session check out-of-band (not consuming a queue
// entry) keeps every existing caller's positional list correct.
function mockFetchSequence(responses: unknown[]) {
  const queue = [...responses];
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    if (String(input).endsWith("/api/auth/session")) {
      return mockResponse(authenticatedSession);
    }
    // The header's downtime badge polls this on its own schedule.
    if (String(input).endsWith("/api/esi/status")) {
      return mockResponse({ downtime: false, retryAfterSeconds: null });
    }
    return { ok: true, json: async () => queue.shift() };
  });
  vi.stubGlobal("fetch", fetchMock);
}

function mockResponse(body: unknown, ok = true, status = 200) {
  return {
    ok,
    status,
    json: async () => body,
  };
}

import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Build } from "../../../../api/industry";

const industryApi = vi.hoisted(() => ({
  getBuild: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

vi.mock("../build-worksheet-editor", () => ({
  BuildWorksheetEditor: ({ focusedProducer }: { focusedProducer?: Build }) => (
    <div>{focusedProducer ? `Focused worksheet: ${focusedProducer.name}` : "Worksheet editor"}</div>
  ),
}));

import { BuildWorkspacePage } from "../build-workspace-page";

function draftBuild(overrides: Partial<Build> = {}): Build {
  return {
    id: "build-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    name: "Rifter Hull Section build",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "dataset-1",
      sourceSdeVersion: "3389399",
      blueprintTypeId: 57_516,
      blueprintName: "Rifter Hull Section Blueprint",
      durationSecondsPerRun: 300,
      materials: [],
      products: [],
      fingerprint: "recipe",
    },
    runs: 2,
    notes: "",
    revision: 1,
    createdAt: "2026-08-05T00:00:00Z",
    updatedAt: "2026-08-05T00:00:00Z",
    draftPlanning: null,
    recipeCurrency: "current",
    activeSdeVersion: "3389399",
    productCategoryName: null,
    productGroupName: null,
    selectedBlueprintOrigin: null,
    hasOwnedBlueprint: false,
    ...overrides,
  };
}

function renderPage(buildId: string) {
  return render(
    <MemoryRouter initialEntries={[`/builds/${buildId}`]}>
      <Routes>
        <Route element={<BuildWorkspacePage />} path="/builds/:buildId" />
        <Route element={<div>Focused producer route</div>} path="/builds/:rootBuildId/producers/:producerBuildId" />
      </Routes>
    </MemoryRouter>,
  );
}

function renderFocusedPage(rootBuildId: string, producerBuildId: string) {
  return render(
    <MemoryRouter initialEntries={[`/builds/${rootBuildId}/producers/${producerBuildId}`]}>
      <Routes>
        <Route element={<BuildWorkspacePage />} path="/builds/:rootBuildId/producers/:producerBuildId" />
      </Routes>
    </MemoryRouter>,
  );
}

describe("BuildWorkspacePage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows no breadcrumb for a root Build", async () => {
    industryApi.getBuild.mockResolvedValue(draftBuild());

    renderPage("build-1");

    expect(await screen.findByText("Worksheet editor")).toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it("redirects a directly opened canonical descendant into its root-owned focused route", async () => {
    industryApi.getBuild.mockResolvedValue(
      draftBuild({
        id: "producer-1",
        planRootBuildId: "root-1",
        planRootBuildName: "Squall",
      }),
    );

    renderPage("producer-1");

    expect(await screen.findByText("Focused producer route")).toBeInTheDocument();
    expect(screen.queryByText("Worksheet editor")).not.toBeInTheDocument();
  });

  it("opens a producer as a scoped view of its canonical root", async () => {
    industryApi.getBuild
      .mockResolvedValueOnce(draftBuild({ id: "root-1", name: "Squall", planRootBuildId: "root-1" }))
      .mockResolvedValueOnce(draftBuild({
        id: "producer-1",
        name: "Auto-Integrity Preservation Seal",
        planRootBuildId: "root-1",
      }));

    renderFocusedPage("root-1", "producer-1");

    expect(await screen.findByText("Focused worksheet: Auto-Integrity Preservation Seal")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Squall/ })).toHaveAttribute(
      "href",
      "/builds/root-1?focusProducer=producer-1",
    );
  });

  it("rejects a focused producer that is not owned by the requested root", async () => {
    industryApi.getBuild
      .mockResolvedValueOnce(draftBuild({ id: "root-1", name: "Squall", planRootBuildId: "root-1" }))
      .mockResolvedValueOnce(draftBuild({
        id: "producer-1",
        planRootBuildId: "other-root",
      }));

    renderFocusedPage("root-1", "producer-1");

    expect(await screen.findByText("Focused producer unavailable")).toBeInTheDocument();
  });
});

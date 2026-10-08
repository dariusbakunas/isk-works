import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { CharacterDetail } from "../../../api/characters";
import { CharacterInspector } from "../character-inspector";

const charactersApi = vi.hoisted(() => ({
  getCharacter: vi.fn(),
  syncCharacter: vi.fn(),
}));

const esiApi = vi.hoisted(() => ({
  beginAuthorization: vi.fn(),
  disconnectConnection: vi.fn(),
  getConnection: vi.fn(),
}));

vi.mock("../../../api/characters", async () => {
  const actual = await vi.importActual<typeof import("../../../api/characters")>("../../../api/characters");
  return { ...actual, ...charactersApi };
});

vi.mock("../../../api/esi", async () => {
  const actual = await vi.importActual<typeof import("../../../api/esi")>("../../../api/esi");
  return { ...actual, ...esiApi };
});

function detailFixture(overrides: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    connectionId: "conn-1",
    eveCharacterId: 2_119_000_001,
    characterName: "Aeva Stark",
    corporationId: 98_000_001,
    corporationName: "Perimeter Industrial Holdings",
    securityStatus: "5.00000",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    walletBalance: "4820000000.0000",
    totalSp: 94_300_000,
    unallocatedSp: null,
    trainingQueue: [
      {
        skillId: 3327,
        skillName: "Caldari Industrial",
        finishedLevel: 5,
        queuePosition: 0,
        startDate: new Date(Date.now() - 12 * 60 * 60 * 1000).toISOString(),
        finishDate: new Date(Date.now() + 12 * 60 * 60 * 1000).toISOString(),
        trainingStartSp: null,
        levelStartSp: null,
        levelEndSp: null,
        currentTrainedLevel: null,
      },
    ],
    trainingObservedAt: new Date().toISOString(),
    trainingQueueScopeMissing: false,
    manufacturingActiveJobs: 8,
    manufacturingMaxJobs: 10,
    reactionActiveJobs: 0,
    reactionMaxJobs: 10,
    researchActiveJobs: 2,
    researchMaxJobs: 10,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: new Date(Date.now() - 4 * 60 * 1000).toISOString(),
    industryJobs: [],
    sources: [
      { sourceKind: "characterInfo", refreshState: "current", observedAt: new Date().toISOString(), nextRefreshAt: null, lastError: null },
      { sourceKind: "location", refreshState: "current", observedAt: new Date().toISOString(), nextRefreshAt: null, lastError: null },
      { sourceKind: "skills", refreshState: "current", observedAt: new Date().toISOString(), nextRefreshAt: null, lastError: null },
      { sourceKind: "wallet", refreshState: "current", observedAt: new Date().toISOString(), nextRefreshAt: null, lastError: null },
      {
        sourceKind: "industryJobs",
        refreshState: "failed",
        observedAt: null,
        nextRefreshAt: null,
        lastError: "missing scope: esi-industry.read_character_jobs.v1",
      },
    ],
    ...overrides,
  };
}

beforeEach(() => {
  charactersApi.getCharacter.mockReset();
  charactersApi.syncCharacter.mockReset();
  esiApi.beginAuthorization.mockReset();
  esiApi.disconnectConnection.mockReset();
  esiApi.getConnection.mockReset();
});

describe("CharacterInspector", () => {
  test("shows the character's overview fields by default", async () => {
    charactersApi.getCharacter.mockResolvedValue(detailFixture());

    render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);

    await screen.findByRole("tab", { name: "Overview" });
    expect(screen.getAllByText("Perimeter Industrial Holdings").length).toBeGreaterThan(0);
    expect(screen.getByText("Jita")).toBeInTheDocument();
    expect(screen.getByText("94,300,000")).toBeInTheDocument();
    expect(screen.getByText(/Caldari Industrial V/)).toBeInTheDocument();
    expect(screen.getByText("Manufacturing")).toBeInTheDocument();
    expect(screen.getByText((_, element) => element?.textContent === "8/10")).toBeInTheDocument();
    expect(screen.getByText("Research")).toBeInTheDocument();
    expect(screen.getByText((_, element) => element?.textContent === "2/10")).toBeInTheDocument();
  });

  test("the Industry tab is selectable and lists the character's active jobs", async () => {
    charactersApi.getCharacter.mockResolvedValue(
      detailFixture({
        industryJobs: [
          {
            jobId: 900_001,
            activity: "manufacturing",
            activityId: 1,
            status: "active",
            blueprintTypeId: 12_004,
            blueprintName: "Ishtar Blueprint",
            productTypeId: 12_005,
            productName: "Ishtar",
            runs: 1,
            facilityId: 1_050_000_000_001,
            facilityName: "Perimeter - ISK Works Factory",
            solarSystemName: "Perimeter",
            startDate: new Date(Date.now() - 3_600_000).toISOString(),
            endDate: new Date(Date.now() + 3_600_000).toISOString(),
          },
        ],
        sources: [
          { sourceKind: "industryJobs", refreshState: "current", observedAt: new Date().toISOString(), nextRefreshAt: null, lastError: null },
        ],
      }),
    );
    const user = userEvent.setup();

    render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });

    await user.click(screen.getByRole("tab", { name: "Industry" }));

    expect(await screen.findByText("Ishtar")).toBeInTheDocument();
    expect(screen.getByText("MFG")).toBeInTheDocument();
    expect(screen.getByText("Perimeter — ISK Works Factory")).toBeInTheDocument();
  });

  test("switching characters does not leave the previous character's jobs on screen", async () => {
    charactersApi.getCharacter.mockImplementation((id: string) =>
      Promise.resolve(
        detailFixture({
          industryJobs: [
            {
              jobId: id === "conn-1" ? 1 : 2,
              activity: "manufacturing",
              activityId: 1,
              status: "active",
              blueprintTypeId: 1,
              blueprintName: null,
              productTypeId: null,
              productName: id === "conn-1" ? "Ishtar" : "Vexor",
              runs: 1,
              facilityId: 1,
              facilityName: null,
              solarSystemName: null,
              startDate: new Date(Date.now() - 3_600_000).toISOString(),
              endDate: new Date(Date.now() + 3_600_000).toISOString(),
            },
          ],
          sources: [
            { sourceKind: "industryJobs", refreshState: "current", observedAt: new Date().toISOString(), nextRefreshAt: null, lastError: null },
          ],
        }),
      ),
    );
    const user = userEvent.setup();

    const view = render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });
    await user.click(screen.getByRole("tab", { name: "Industry" }));
    expect(await screen.findByText("Ishtar")).toBeInTheDocument();

    view.rerender(<CharacterInspector connectionId="conn-2" onChanged={vi.fn()} onClose={vi.fn()} />);

    expect(await screen.findByText("Vexor")).toBeInTheDocument();
    expect(screen.queryByText("Ishtar")).not.toBeInTheDocument();
  });

  test("the Skills tab is between Overview and Industry and shows the real skill queue", async () => {
    charactersApi.getCharacter.mockResolvedValue(detailFixture());
    const user = userEvent.setup();

    render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });

    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toEqual(["Overview", "Skills", "Industry", "Sync"]);

    await user.click(screen.getByRole("tab", { name: "Skills" }));

    expect(await screen.findByText("Skill Queue")).toBeInTheDocument();
    expect(screen.getByText("1 / 150 queued")).toBeInTheDocument();
    expect(screen.getByText("Training active")).toBeInTheDocument();
    expect(screen.getAllByText(/Caldari Industrial V/).length).toBeGreaterThan(0);
  });

  test("switching characters on the Skills tab drops the previous queue and shows the new one", async () => {
    charactersApi.getCharacter.mockImplementation((id: string) =>
      Promise.resolve(
        id === "conn-1"
          ? detailFixture()
          : detailFixture({
              totalSp: 12_000_000,
              trainingQueue: [
                {
                  skillId: 9001,
                  skillName: "Astrogeology",
                  finishedLevel: 4,
                  queuePosition: 0,
                  startDate: new Date(Date.now() - 60 * 60 * 1000).toISOString(),
                  finishDate: new Date(Date.now() + 60 * 60 * 1000).toISOString(),
                  trainingStartSp: 0,
                  levelStartSp: 0,
                  levelEndSp: 1_000_000,
                  currentTrainedLevel: null,
                },
              ],
            }),
      ),
    );
    const user = userEvent.setup();

    const view = render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });
    await user.click(screen.getByRole("tab", { name: "Skills" }));
    expect(await screen.findByText(/Caldari Industrial V/)).toBeInTheDocument();

    view.rerender(<CharacterInspector connectionId="conn-2" onChanged={vi.fn()} onClose={vi.fn()} />);

    expect(await screen.findByText(/Astrogeology IV/)).toBeInTheDocument();
    expect(screen.queryByText(/Caldari Industrial V/)).not.toBeInTheDocument();
    expect(screen.getByText(/12m/)).toBeInTheDocument();
  });

  test("switching to the Sync tab shows every source with its state", async () => {
    charactersApi.getCharacter.mockResolvedValue(detailFixture());
    const user = userEvent.setup();

    render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });

    await user.click(screen.getByRole("tab", { name: "Sync" }));

    expect(screen.getByText("Character Info")).toBeInTheDocument();
    expect(screen.getByText("Industry Jobs")).toBeInTheDocument();
    expect(screen.getByText("Missing: esi-industry.read_character_jobs.v1")).toBeInTheDocument();
  });

  test("Sync now reloads the character and notifies the roster", async () => {
    charactersApi.getCharacter.mockResolvedValue(detailFixture());
    charactersApi.syncCharacter.mockResolvedValue([]);
    const onChanged = vi.fn();
    const user = userEvent.setup();

    render(<CharacterInspector connectionId="conn-1" onChanged={onChanged} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });
    await user.click(screen.getByRole("tab", { name: "Sync" }));

    await user.click(screen.getByRole("button", { name: /Sync now/ }));

    await waitFor(() => expect(charactersApi.syncCharacter).toHaveBeenCalledWith("conn-1"));
    expect(charactersApi.getCharacter).toHaveBeenCalledTimes(2);
    expect(onChanged).toHaveBeenCalled();
  });

  test("Disconnect calls the API, notifies the roster, and closes the inspector", async () => {
    charactersApi.getCharacter.mockResolvedValue(detailFixture());
    esiApi.disconnectConnection.mockResolvedValue({});
    const onChanged = vi.fn();
    const onClose = vi.fn();
    const user = userEvent.setup();

    render(<CharacterInspector connectionId="conn-1" onChanged={onChanged} onClose={onClose} />);
    await screen.findByRole("tab", { name: "Overview" });
    await user.click(screen.getByRole("tab", { name: "Sync" }));

    await user.click(screen.getByRole("button", { name: "Disconnect character" }));

    await waitFor(() => expect(esiApi.disconnectConnection).toHaveBeenCalledWith("conn-1"));
    expect(onChanged).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  test("Reconnect in fixture mode reloads instead of redirecting", async () => {
    charactersApi.getCharacter.mockResolvedValue(detailFixture());
    esiApi.beginAuthorization.mockResolvedValue({
      authorizationUrl: "",
      fixtureMode: true,
      connection: null,
      requestedScopes: [],
    });
    const onChanged = vi.fn();
    const user = userEvent.setup();

    render(<CharacterInspector connectionId="conn-1" onChanged={onChanged} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });
    await user.click(screen.getByRole("tab", { name: "Sync" }));

    await user.click(screen.getByRole("button", { name: /Reconnect/ }));

    await waitFor(() => expect(esiApi.beginAuthorization).toHaveBeenCalled());
    expect(onChanged).toHaveBeenCalled();
    expect(charactersApi.getCharacter).toHaveBeenCalledTimes(2);
  });

  test("View token details fetches and shows the connection's token and scopes", async () => {
    charactersApi.getCharacter.mockResolvedValue(detailFixture());
    esiApi.getConnection.mockResolvedValue({
      id: "conn-1",
      ownerId: "owner-1",
      eveCharacterId: 2_119_000_001,
      characterName: "Aeva Stark",
      status: "connected",
      grantedScopes: ["esi-location.read_location.v1", "esi-skills.read_skills.v1"],
      accessTokenExpiresAt: "2026-08-21T12:00:00Z",
      lastRefreshedAt: "2026-08-21T11:00:00Z",
      lastErrorCode: null,
      lastErrorMessage: null,
      connectedAt: "2026-08-01T00:00:00Z",
      updatedAt: "2026-08-21T11:00:00Z",
      disconnectedAt: null,
      revision: 3,
    });
    const user = userEvent.setup();

    render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);
    await screen.findByRole("tab", { name: "Overview" });
    await user.click(screen.getByRole("tab", { name: "Sync" }));

    await user.click(screen.getByRole("button", { name: "View token details" }));

    expect(esiApi.getConnection).toHaveBeenCalledWith("conn-1");
    await screen.findByText("esi-location.read_location.v1");
    expect(screen.getByText("esi-skills.read_skills.v1")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "View token details" }));
    expect(screen.queryByText("esi-location.read_location.v1")).not.toBeInTheDocument();
  });

  test("shows an error message when the character fails to load", async () => {
    charactersApi.getCharacter.mockRejectedValue(new Error("boom"));

    render(<CharacterInspector connectionId="conn-1" onChanged={vi.fn()} onClose={vi.fn()} />);

    await waitFor(() =>
      expect(screen.getByText("The EVE integration request could not be completed.")).toBeInTheDocument(),
    );
  });
});

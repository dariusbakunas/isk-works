import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { CharacterRosterEntry } from "../../../api/characters";
import { CharactersPage } from "../characters-page";

const charactersApi = vi.hoisted(() => ({
  listCharacters: vi.fn(),
  getCharacter: vi.fn(),
}));

const esiApi = vi.hoisted(() => ({
  beginAuthorization: vi.fn(),
}));

vi.mock("../../../api/characters", async () => {
  const actual = await vi.importActual<typeof import("../../../api/characters")>("../../../api/characters");
  return { ...actual, ...charactersApi };
});

vi.mock("../../../api/esi", async () => {
  const actual = await vi.importActual<typeof import("../../../api/esi")>("../../../api/esi");
  return { ...actual, ...esiApi };
});

function entry(overrides: Partial<CharacterRosterEntry> = {}): CharacterRosterEntry {
  return {
    connectionId: `conn-${overrides.characterName ?? "1"}`,
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
        startDate: null,
        finishDate: null,
        trainingStartSp: null,
        levelStartSp: null,
        levelEndSp: null,
        currentTrainedLevel: null,
      },
    ],
    trainingObservedAt: null,
    trainingQueueScopeMissing: false,
    manufacturingActiveJobs: 0,
    manufacturingMaxJobs: null,
    reactionActiveJobs: 0,
    reactionMaxJobs: null,
    researchActiveJobs: 0,
    researchMaxJobs: null,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: new Date().toISOString(),
    ...overrides,
  };
}

function renderPage() {
  return render(
    <MemoryRouter>
      <CharactersPage />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  charactersApi.listCharacters.mockReset();
  charactersApi.getCharacter.mockReset();
  esiApi.beginAuthorization.mockReset();
});

describe("CharactersPage", () => {
  test("renders a card for every connected character", async () => {
    charactersApi.listCharacters.mockResolvedValue([
      entry({ characterName: "Aeva Stark" }),
      entry({ characterName: "Drake Orin", health: "stale" }),
    ]);

    renderPage();

    expect(await screen.findByText("Aeva Stark")).toBeInTheDocument();
    expect(screen.getByText("Drake Orin")).toBeInTheDocument();
    expect(screen.getByText(/2 characters/)).toBeInTheDocument();
  });

  test("shows an empty state with a connect action when there are no characters", async () => {
    charactersApi.listCharacters.mockResolvedValue([]);

    renderPage();

    expect(await screen.findByText("No connected characters")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Connect EVE Character" })).toBeInTheDocument();
  });

  test("filtering to Needs Attention hides healthy characters", async () => {
    charactersApi.listCharacters.mockResolvedValue([
      entry({ characterName: "Aeva Stark", health: "healthy" }),
      entry({ characterName: "Drake Orin", health: "reconnectRequired" }),
    ]);
    const user = userEvent.setup();

    renderPage();
    await screen.findByText("Aeva Stark");

    await user.click(screen.getByRole("button", { name: "Needs Attention" }));

    expect(screen.queryByText("Aeva Stark")).not.toBeInTheDocument();
    expect(screen.getByText("Drake Orin")).toBeInTheDocument();
  });

  test("searching filters by character and corporation name", async () => {
    charactersApi.listCharacters.mockResolvedValue([
      entry({ characterName: "Aeva Stark", corporationName: "Perimeter Industrial Holdings" }),
      entry({ characterName: "Drake Orin", corporationName: "Center for Advanced Studies" }),
    ]);
    const user = userEvent.setup();

    renderPage();
    await screen.findByText("Aeva Stark");

    await user.type(screen.getByLabelText("Search characters"), "advanced");

    expect(screen.queryByText("Aeva Stark")).not.toBeInTheDocument();
    expect(screen.getByText("Drake Orin")).toBeInTheDocument();
  });

  test("connecting a character in fixture mode reloads the roster instead of redirecting", async () => {
    charactersApi.listCharacters.mockResolvedValueOnce([entry({ characterName: "Aeva Stark" })]);
    esiApi.beginAuthorization.mockResolvedValue({
      authorizationUrl: "",
      fixtureMode: true,
      connection: null,
      requestedScopes: [],
    });
    charactersApi.listCharacters.mockResolvedValueOnce([
      entry({ characterName: "Aeva Stark" }),
      entry({ characterName: "Drake Orin" }),
    ]);
    const user = userEvent.setup();

    renderPage();
    await screen.findByText("Aeva Stark");

    await user.click(screen.getByRole("button", { name: "Connect Character" }));

    expect(await screen.findByText("Drake Orin")).toBeInTheDocument();
    expect(charactersApi.listCharacters).toHaveBeenCalledTimes(2);
  });

  test("shows an error message when the roster fails to load", async () => {
    charactersApi.listCharacters.mockRejectedValue(new Error("boom"));

    renderPage();

    await waitFor(() => expect(screen.getByText("Couldn't load characters")).toBeInTheDocument());
  });

  test("clicking a card opens the inspector, and closing it returns to the roster", async () => {
    charactersApi.listCharacters.mockResolvedValue([entry({ characterName: "Aeva Stark" })]);
    charactersApi.getCharacter.mockResolvedValue({
      ...entry({ characterName: "Aeva Stark" }),
      sources: [],
      industryJobs: [],
    });
    const user = userEvent.setup();

    renderPage();
    await screen.findByText("Aeva Stark");

    await user.click(screen.getByRole("button", { name: /Aeva Stark/ }));

    expect(await screen.findByRole("tab", { name: "Overview" })).toBeInTheDocument();
    expect(charactersApi.getCharacter).toHaveBeenCalledWith("conn-Aeva Stark");

    await user.click(screen.getByRole("button", { name: "Close character inspector" }));

    expect(screen.queryByRole("tab", { name: "Overview" })).not.toBeInTheDocument();
  });

  test("a training completion boundary reconciles just that character via a targeted refetch", async () => {
    const alreadyExpired = {
      skillId: 3327,
      skillName: "Caldari Industrial",
      finishedLevel: 5,
      queuePosition: 0,
      startDate: new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString(),
      finishDate: new Date(Date.now() - 60 * 60 * 1000).toISOString(),
      trainingStartSp: null,
      levelStartSp: null,
      levelEndSp: null,
      currentTrainedLevel: null,
    };
    charactersApi.listCharacters.mockResolvedValue([
      entry({ characterName: "Aeva Stark", trainingQueue: [alreadyExpired] }),
    ]);
    charactersApi.getCharacter.mockResolvedValue({
      ...entry({ characterName: "Aeva Stark", trainingQueue: [] }),
      sources: [],
      industryJobs: [],
    });

    renderPage();
    await screen.findByText("Aeva Stark");

    await waitFor(() => expect(charactersApi.getCharacter).toHaveBeenCalledWith("conn-Aeva Stark"));
    expect(charactersApi.listCharacters).toHaveBeenCalledTimes(1);
  });

  describe("background polling", () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    test("polls the roster every 5 minutes and stops on unmount", async () => {
      vi.useFakeTimers();
      charactersApi.listCharacters.mockResolvedValue([]);

      const { unmount } = renderPage();
      await act(async () => {
        await Promise.resolve();
      });
      expect(charactersApi.listCharacters).toHaveBeenCalledTimes(1);

      await act(async () => {
        vi.advanceTimersByTime(5 * 60 * 1000);
        await Promise.resolve();
      });
      expect(charactersApi.listCharacters).toHaveBeenCalledTimes(2);

      unmount();
      await act(async () => {
        vi.advanceTimersByTime(5 * 60 * 1000);
        await Promise.resolve();
      });
      expect(charactersApi.listCharacters).toHaveBeenCalledTimes(2);
    });
  });
});

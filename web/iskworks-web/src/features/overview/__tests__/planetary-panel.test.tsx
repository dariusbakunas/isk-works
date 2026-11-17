import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { PlanetaryCharacter } from "../../../api/planetary";
import { OverviewDataProvider } from "../overview-data";
import { PlanetaryPanel } from "../planetary-panel";
import { fetchByUrl, hoursFromNow, NOW, planetaryOverview } from "./overview-fixtures";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function piCharacter(scopeGranted: boolean): PlanetaryCharacter {
  return {
    connectionId: "11111111-1111-1111-1111-111111111111",
    eveCharacterId: 2_119_000_001,
    name: "Kira Vantor",
    piSkillLevel: 5,
    scopeGranted,
    sync: { observedAt: null, refreshState: "current", lastError: null },
    iskPerMonth: "0",
    nextExpiryAt: null,
    alertCount: 0,
    planets: [],
  };
}

function renderPanel() {
  return render(
    <MemoryRouter>
      <OverviewDataProvider>
        <PlanetaryPanel />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

const summary = planetaryOverview().summary;

describe("PlanetaryPanel", () => {
  test("income, colonies, the next extractor expiry and alert counts", async () => {
    fetchByUrl({
      planetary: planetaryOverview({
        characters: [piCharacter(true)],
        summary: {
          ...summary,
          nextAction: { characterName: "Ana Belle", planetName: "Barren VII-4", at: hoursFromNow(3.2) },
          alerts: { expired: 2, storageFull: 1, starved: 0 },
        },
      }),
    });
    renderPanel();

    const panel = await screen.findByRole("region", { name: "Planetary" });
    const totals = await within(panel).findByRole("group", { name: "Planetary totals" });
    expect(within(totals).getByText("412M")).toBeInTheDocument();
    expect(within(totals).getByText("6")).toBeInTheDocument();
    const next = within(panel).getByText(/Extractor expires/).closest("p")!;
    expect(next).toHaveTextContent("Extractor expires: Ana Belle · Barren VII-4 · in 3h 12m");
    expect(next).toHaveClass("text-warning");
    expect(within(panel).getByText("Expired").parentElement).toHaveTextContent("Expired 2");
    expect(within(panel).getByText("Starved").parentElement).toHaveClass("text-muted");
  });

  test("an expiry more than a day out isn't highlighted", async () => {
    fetchByUrl({ planetary: planetaryOverview({ characters: [piCharacter(true)] }) });
    renderPanel();

    const next = (await screen.findByText(/Extractor expires/)).closest("p")!;
    expect(next).not.toHaveClass("text-warning");
  });

  test("without running extractors", async () => {
    fetchByUrl({ planetary: planetaryOverview({ characters: [piCharacter(true)], summary: { ...summary, nextAction: null } }) });
    renderPanel();

    expect(await screen.findByText("No running extractors.")).toBeInTheDocument();
  });

  test("no character has granted the PI scope", async () => {
    fetchByUrl({ planetary: planetaryOverview({ characters: [piCharacter(false)], summary: { ...summary, planetCount: 0 } }) });
    renderPanel();

    const panel = await screen.findByRole("region", { name: "Planetary" });
    expect(await within(panel).findByText(/No PI scope granted/)).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: "Connect" })).toHaveAttribute("href", "/characters");
  });
});

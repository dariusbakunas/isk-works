import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { activeJobs } from "../../jobs/__tests__/fixtures";
import { AttentionPanel } from "../attention-panel";
import { OverviewDataProvider } from "../overview-data";
import { fetchByUrl, marketOverview, NOW, rosterEntry } from "./overview-fixtures";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function renderPanel() {
  return render(
    <MemoryRouter>
      <OverviewDataProvider>
        <AttentionPanel />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

const fullSlots = { manufacturing: { used: 12, max: 12 }, reaction: { used: 10, max: 10 }, research: { used: 20, max: 20 } };

describe("AttentionPanel", () => {
  test("each chip links to the page that resolves it, most urgent first", async () => {
    fetchByUrl({
      characters: [rosterEntry({ health: "reconnectRequired" })],
      jobs: activeJobs({ summary: { running: 3, ready: 4, endingWithinDay: 0 }, slots: fullSlots }),
      market: marketOverview({ summary: { ...marketOverview().summary, outbid: 7 } }),
    });
    renderPanel();

    const region = await screen.findByRole("region", { name: "Attention" });
    // Chips appear as their sources arrive; wait for the last one.
    await within(region).findByRole("link", { name: /jobs ready to deliver/ });
    const links = within(region).getAllByRole("link");
    expect(links.map((link) => link.textContent)).toEqual([
      "1character needs reconnect",
      "7orders outbid",
      "4jobs ready to deliver",
    ]);
    expect(links.map((link) => link.getAttribute("href"))).toEqual(["/characters", "/market-orders", "/jobs"]);
  });

  test("nothing pending reads as all clear", async () => {
    fetchByUrl({ jobs: activeJobs({ summary: { running: 3, ready: 0, endingWithinDay: 0 }, slots: fullSlots }) });
    renderPanel();

    expect(await screen.findByText("All clear: nothing needs you right now.")).toBeInTheDocument();
  });

  test("failed sources are counted, and never read as all clear", async () => {
    fetchByUrl({
      jobs: activeJobs({ summary: { running: 3, ready: 0, endingWithinDay: 0 }, slots: fullSlots }),
      market: new Error("market down"),
      runway: new Error("runway down"),
    });
    renderPanel();

    expect(await screen.findByText("2 sources unavailable")).toBeInTheDocument();
    expect(screen.queryByText(/All clear/)).not.toBeInTheDocument();
  });
});

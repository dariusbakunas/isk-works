import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { CharactersStrip } from "../characters-strip";
import { OverviewDataProvider } from "../overview-data";
import { fetchByUrl, hoursFromNow, NOW, rosterEntry } from "./overview-fixtures";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function renderStrip() {
  return render(
    <MemoryRouter>
      <OverviewDataProvider>
        <CharactersStrip />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

const entry = rosterEntry().trainingQueue[0]!;

async function card(name: string) {
  const strip = await screen.findByRole("region", { name: "Characters" });
  return (await within(strip).findByText(name)).closest("a")!;
}

describe("CharactersStrip", () => {
  test("a training character: system, wallet, skill, queue end, progress and slots", async () => {
    fetchByUrl({
      characters: [
        rosterEntry({
          trainingQueue: [entry, { ...entry, queuePosition: 1, skillName: "Reactions", finishedLevel: 4, startDate: hoursFromNow(200), finishDate: hoursFromNow(436) }],
          manufacturingActiveJobs: 4,
          manufacturingMaxJobs: 5,
          reactionMaxJobs: null,
        }),
      ],
    });
    renderStrip();

    const kira = await card("Kira Vantor");
    expect(kira).toHaveAttribute("href", "/characters");
    expect(within(kira).getByText("Jita")).toBeInTheDocument();
    expect(within(kira).getByText("4.8B")).toBeInTheDocument();
    expect(within(kira).getByText("Caldari Industrial V")).toBeInTheDocument();
    expect(within(kira).getByText("queue ends in 18d 4h")).toBeInTheDocument();
    expect(within(kira).getByRole("progressbar", { name: "Current skill progress" })).toBeInTheDocument();
    expect(within(kira).getByText("M 4/5")).toBeInTheDocument();
    expect(within(kira).getByText("R 1/?")).toBeInTheDocument();
    expect(within(kira).getByLabelText("Healthy")).toBeInTheDocument();
    expect(screen.getByText("1 connected")).toBeInTheDocument();
  });

  test("empty, paused and scope-less queues", async () => {
    fetchByUrl({
      characters: [
        rosterEntry({ characterName: "Mira Dax", trainingQueue: [] }),
        rosterEntry({ characterName: "Mox Halden", trainingQueue: [{ ...entry, startDate: null, finishDate: null }], health: "reconnectRequired" }),
        rosterEntry({ characterName: "Tyra Wolfe", trainingQueue: [], trainingQueueScopeMissing: true }),
      ],
    });
    renderStrip();

    expect(within(await card("Mira Dax")).getByText("Queue empty")).toHaveClass("text-danger");
    expect(within(await card("Mox Halden")).getByText("Training paused")).toBeInTheDocument();
    expect(within(await card("Mox Halden")).getByLabelText("Reconnect required")).toBeInTheDocument();
    expect(within(await card("Tyra Wolfe")).getByText("No queue scope")).toBeInTheDocument();
  });
});

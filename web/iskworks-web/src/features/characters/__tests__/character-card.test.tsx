import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CharacterRosterEntry, SkillQueueEntry } from "../../../api/characters";
import { CharacterCard } from "../character-card";

function trainingQueueFixture(overrides: Partial<SkillQueueEntry> = {}): SkillQueueEntry[] {
  return [
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
      ...overrides,
    },
  ];
}

function entryFixture(overrides: Partial<CharacterRosterEntry> = {}): CharacterRosterEntry {
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
    trainingQueue: trainingQueueFixture(),
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
    ...overrides,
  };
}

describe("CharacterCard", () => {
  it("shows the character's name, corporation, location, wallet, and SP", () => {
    render(<CharacterCard entry={entryFixture()} />);

    expect(screen.getByText("Aeva Stark")).toBeInTheDocument();
    expect(screen.getByText("Perimeter Industrial Holdings")).toBeInTheDocument();
    expect(screen.getByText("Jita")).toBeInTheDocument();
    expect(screen.getByText("94.3m SP")).toBeInTheDocument();
  });

  it("falls back to a placeholder corporation label when the name isn't cached yet", () => {
    render(<CharacterCard entry={entryFixture({ corporationName: null })} />);

    expect(screen.getByText("Unknown corporation")).toBeInTheDocument();
  });

  it("shows the health badge and a relative sync age", () => {
    render(<CharacterCard entry={entryFixture({ health: "reconnectRequired" })} />);

    expect(screen.getByText(/Reconnect required/)).toBeInTheDocument();
    expect(screen.getByText(/4m ago/)).toBeInTheDocument();
  });

  it("shows a no-skill-queue note when the character isn't training anything", () => {
    render(<CharacterCard entry={entryFixture({ trainingQueue: [] })} />);

    expect(screen.getByText("No skill queue")).toBeInTheDocument();
  });

  it("shows the training skill's name, level, and a progress bar", () => {
    render(<CharacterCard entry={entryFixture()} />);

    expect(screen.getByText("Caldari Industrial V")).toBeInTheDocument();
  });

  it("falls back to a generic label when the skill name isn't resolved yet", () => {
    render(<CharacterCard entry={entryFixture({ trainingQueue: trainingQueueFixture({ skillName: null }) })} />);

    expect(screen.getByText("Training")).toBeInTheDocument();
  });

  it("shows a per-category active/max stat for each category with active jobs", () => {
    render(<CharacterCard entry={entryFixture()} />);

    expect(screen.getByText("MFG")).toBeInTheDocument();
    expect(screen.getByText("8/10")).toBeInTheDocument();
    expect(screen.getByText("RES")).toBeInTheDocument();
    expect(screen.getByText("2/10")).toBeInTheDocument();
    expect(screen.queryByText("RXN")).not.toBeInTheDocument();
  });

  it("shows a no-active-jobs note when nothing is active in any category", () => {
    render(
      <CharacterCard
        entry={entryFixture({ manufacturingActiveJobs: 0, reactionActiveJobs: 0, researchActiveJobs: 0 })}
      />,
    );

    expect(screen.getByText("No active jobs")).toBeInTheDocument();
  });

  it("is not interactive when no onOpen handler is given", () => {
    render(<CharacterCard entry={entryFixture()} />);

    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("calls onOpen with the connection id when clicked", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    render(<CharacterCard entry={entryFixture()} onOpen={onOpen} />);

    await user.click(screen.getByRole("button"));

    expect(onOpen).toHaveBeenCalledWith("conn-1");
  });
});

import { render, screen, within } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import type { CharacterDetail, CharacterSourceDetail, IndustryJob } from "../../../api/characters";
import { CharacterIndustryTab } from "../character-industry-tab";

const NOW = Date.parse("2026-09-09T12:00:00Z");

function source(overrides: Partial<CharacterSourceDetail> & Pick<CharacterSourceDetail, "sourceKind">): CharacterSourceDetail {
  return {
    refreshState: "current",
    observedAt: new Date(NOW - 5 * 60_000).toISOString(),
    nextRefreshAt: null,
    lastError: null,
    ...overrides,
  };
}

function job(overrides: Partial<IndustryJob> = {}): IndustryJob {
  return {
    jobId: 1,
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
    startDate: new Date(NOW - 3_600_000).toISOString(),
    endDate: new Date(NOW + 3_600_000).toISOString(),
    ...overrides,
  };
}

function detail(overrides: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    connectionId: "conn-1",
    eveCharacterId: 2_119_000_001,
    characterName: "Aeva Stark",
    corporationId: null,
    corporationName: null,
    securityStatus: null,
    solarSystemId: null,
    solarSystemName: null,
    walletBalance: null,
    totalSp: null,
    unallocatedSp: null,
    trainingQueue: [],
    trainingObservedAt: null,
    trainingQueueScopeMissing: false,
    manufacturingActiveJobs: 1,
    manufacturingMaxJobs: 10,
    reactionActiveJobs: 1,
    reactionMaxJobs: 4,
    researchActiveJobs: 1,
    researchMaxJobs: 11,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: new Date(NOW - 5 * 60_000).toISOString(),
    sources: [source({ sourceKind: "industryJobs" })],
    industryJobs: [job()],
    ...overrides,
  };
}

describe("CharacterIndustryTab", () => {
  test("renders three slot cards with skill-derived active/capacity values", () => {
    render(<CharacterIndustryTab detail={detail()} nowMs={NOW} />);

    for (const label of ["Manufacturing", "Reactions", "Research"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(screen.getByText((_, el) => el?.textContent === "1/10")).toBeInTheDocument();
    expect(screen.getByText((_, el) => el?.textContent === "1/4")).toBeInTheDocument();
    // Research capacity is the character's real Laboratory Operation pool
    // (11), never a hardcoded /10.
    expect(screen.getByText((_, el) => el?.textContent === "1/11")).toBeInTheDocument();
  });

  test("a manufacturing row shows product, location, remaining time and progress", () => {
    render(<CharacterIndustryTab detail={detail()} nowMs={NOW} />);

    const row = screen.getByText("Ishtar").closest("li") as HTMLElement;
    const scoped = within(row);
    expect(scoped.getByText("MFG")).toBeInTheDocument();
    expect(scoped.getByText("Perimeter — ISK Works Factory")).toBeInTheDocument();
    expect(scoped.getByText("1h 0m")).toBeInTheDocument();
    expect(scoped.getByText("50%")).toBeInTheDocument();
  });

  test("a reaction row renders with the RXN badge and output name", () => {
    render(
      <CharacterIndustryTab
        detail={detail({
          industryJobs: [
            job({
              jobId: 2,
              activity: "reaction",
              activityId: 9,
              blueprintName: "Nanoelectrical Microprocessor Reaction Formula",
              productName: "Nanoelectrical Microprocessor",
              runs: 400,
            }),
          ],
        })}
        nowMs={NOW}
      />,
    );

    const row = screen.getByText("Nanoelectrical Microprocessor ×400").closest("li") as HTMLElement;
    expect(within(row).getByText("RXN")).toBeInTheDocument();
  });

  test("a research row renders with the RESEARCH badge and the ME subtype folded into the name", () => {
    render(
      <CharacterIndustryTab
        detail={detail({
          industryJobs: [
            job({
              jobId: 3,
              activity: "materialEfficiencyResearch",
              activityId: 4,
              blueprintName: "Merlin Blueprint",
              productName: null,
            }),
          ],
        })}
        nowMs={NOW}
      />,
    );

    const row = screen.getByText("Merlin Blueprint ME").closest("li") as HTMLElement;
    expect(within(row).getByText("RESEARCH")).toBeInTheDocument();
  });

  test("a job whose timer elapsed reads as Ready at 100%, not a negative countdown", () => {
    render(
      <CharacterIndustryTab
        detail={detail({
          industryJobs: [
            job({ startDate: new Date(NOW - 7_200_000).toISOString(), endDate: new Date(NOW - 60_000).toISOString() }),
          ],
        })}
        nowMs={NOW}
      />,
    );

    const row = screen.getByText("Ishtar").closest("li") as HTMLElement;
    expect(within(row).getByText("Ready")).toBeInTheDocument();
    expect(within(row).getByText("100%")).toBeInTheDocument();
  });

  test("shows the empty state but keeps the capacity cards when there are no jobs", () => {
    render(<CharacterIndustryTab detail={detail({ industryJobs: [] })} nowMs={NOW} />);

    expect(screen.getByText("No active industry jobs.")).toBeInTheDocument();
    expect(screen.getByText("Manufacturing")).toBeInTheDocument();
  });

  test("distinguishes a missing scope from a genuine zero", () => {
    render(
      <CharacterIndustryTab
        detail={detail({
          industryJobs: [],
          sources: [
            source({
              sourceKind: "industryJobs",
              refreshState: "failed",
              observedAt: null,
              lastError: "missing scope: esi-industry.read_character_jobs.v1",
            }),
          ],
        })}
        nowMs={NOW}
      />,
    );

    expect(screen.getByText("Industry jobs need re-authorization")).toBeInTheDocument();
    expect(screen.getByText("esi-industry.read_character_jobs.v1")).toBeInTheDocument();
    expect(screen.queryByText("No active industry jobs.")).not.toBeInTheDocument();
  });

  test("shows a not-synced-yet state before the first industry sync", () => {
    render(
      <CharacterIndustryTab
        detail={detail({
          industryJobs: [],
          sources: [source({ sourceKind: "industryJobs", refreshState: "missing", observedAt: null })],
        })}
        nowMs={NOW}
      />,
    );

    expect(screen.getByText("Not synced yet.")).toBeInTheDocument();
  });

  test("capacity cards show a dash when skills have not synced", () => {
    render(
      <CharacterIndustryTab
        detail={detail({ manufacturingMaxJobs: null, manufacturingActiveJobs: null, industryJobs: [] })}
        nowMs={NOW}
      />,
    );

    expect(screen.getByText((_, el) => el?.textContent === "—/—")).toBeInTheDocument();
  });
});

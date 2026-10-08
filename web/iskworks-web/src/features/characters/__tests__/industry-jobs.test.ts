import { describe, expect, test } from "vitest";

import type { IndustryJob } from "../../../api/characters";
import {
  formatJobRemaining,
  industryActivityBadge,
  industryActivitySubtype,
  industryJobLocation,
  industryJobName,
  industryJobProgress,
} from "../industry-jobs";

const NOW = Date.parse("2026-09-09T12:00:00Z");

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
    startDate: new Date(NOW - 4 * 3_600_000).toISOString(),
    endDate: new Date(NOW + 4 * 3_600_000).toISOString(),
    ...overrides,
  };
}

describe("industryActivityBadge / subtype", () => {
  test("keeps the three job families and their tones", () => {
    expect(industryActivityBadge("manufacturing")).toEqual({ label: "MFG", tone: "warning" });
    expect(industryActivityBadge("reaction")).toEqual({ label: "RXN", tone: "reaction" });
    for (const activity of ["materialEfficiencyResearch", "timeEfficiencyResearch", "copying", "invention"] as const) {
      expect(industryActivityBadge(activity)).toEqual({ label: "RESEARCH", tone: "primary" });
    }
  });

  test("exposes a subtype tag only for science activities", () => {
    expect(industryActivitySubtype("materialEfficiencyResearch")).toBe("ME");
    expect(industryActivitySubtype("timeEfficiencyResearch")).toBe("TE");
    expect(industryActivitySubtype("copying")).toBe("Copy");
    expect(industryActivitySubtype("invention")).toBe("Invent");
    expect(industryActivitySubtype("manufacturing")).toBeNull();
    expect(industryActivitySubtype("reaction")).toBeNull();
  });
});

describe("industryJobName", () => {
  test("manufacturing shows the product, with ×runs when above 1", () => {
    expect(industryJobName(job({ runs: 1 }))).toBe("Ishtar");
    expect(industryJobName(job({ runs: 2 }))).toBe("Ishtar ×2");
    expect(industryJobName(job({ runs: 3_625 }))).toBe("Ishtar ×3,625");
  });

  test("reactions show the reaction output", () => {
    expect(
      industryJobName(
        job({ activity: "reaction", productName: "Nanoelectrical Microprocessor", runs: 400 }),
      ),
    ).toBe("Nanoelectrical Microprocessor ×400");
  });

  test("copying shows the blueprint with a (copy) hint and copy count", () => {
    expect(
      industryJobName(
        job({ activity: "copying", blueprintName: "Rifter Blueprint", productName: null, runs: 5 }),
      ),
    ).toBe("Rifter Blueprint (copy) ×5");
  });

  test("research folds the subtype into the blueprint name", () => {
    expect(
      industryJobName(
        job({ activity: "materialEfficiencyResearch", blueprintName: "Merlin Blueprint", productName: null }),
      ),
    ).toBe("Merlin Blueprint ME");
    expect(
      industryJobName(job({ activity: "timeEfficiencyResearch", blueprintName: "Merlin Blueprint", productName: null })),
    ).toBe("Merlin Blueprint TE");
  });

  test("falls back gracefully when names are unresolved", () => {
    expect(industryJobName(job({ productName: null, blueprintName: null }))).toBe("Unknown job");
  });
});

describe("industryJobLocation", () => {
  test("joins system and structure, stripping a redundant leading system token", () => {
    expect(industryJobLocation(job())).toBe("Perimeter — ISK Works Factory");
    expect(
      industryJobLocation(job({ solarSystemName: "Q-3HS5", facilityName: "Q-3HS5 - Q-3HS5 Trading Hub" })),
    ).toBe("Q-3HS5 — Q-3HS5 Trading Hub");
  });

  test("keeps the full facility name when it does not lead with the system", () => {
    expect(industryJobLocation(job({ solarSystemName: "Jita", facilityName: "4-4 Caldari Navy Assembly Plant" }))).toBe(
      "Jita — 4-4 Caldari Navy Assembly Plant",
    );
    // "Perim" must not be stripped from "Perimeter" (word-boundary guard).
    expect(industryJobLocation(job({ solarSystemName: "Perim", facilityName: "Perimeter Gate Keep" }))).toBe(
      "Perim — Perimeter Gate Keep",
    );
  });

  test("collapses to the system alone when the structure is named exactly after it", () => {
    expect(industryJobLocation(job({ solarSystemName: "Q-3HS5", facilityName: "Q-3HS5" }))).toBe("Q-3HS5");
  });

  test("degrades to whichever half is known, then to a neutral label", () => {
    expect(industryJobLocation(job({ solarSystemName: null }))).toBe("Perimeter - ISK Works Factory");
    expect(industryJobLocation(job({ facilityName: null }))).toBe("Perimeter");
    expect(industryJobLocation(job({ facilityName: null, solarSystemName: null }))).toBe("Unknown structure");
  });
});

describe("industryJobProgress (deterministic clock)", () => {
  test("not started yet -> 0%", () => {
    const p = industryJobProgress(
      job({ startDate: new Date(NOW + 3_600_000).toISOString(), endDate: new Date(NOW + 7_200_000).toISOString() }),
      NOW,
    );
    expect(p.percent).toBe(0);
    expect(p.state).toBe("running");
  });

  test("half complete -> 50%", () => {
    const p = industryJobProgress(
      job({ startDate: new Date(NOW - 3_600_000).toISOString(), endDate: new Date(NOW + 3_600_000).toISOString() }),
      NOW,
    );
    expect(p.percent).toBe(50);
    expect(p.remainingLabel).toBe("1h 0m");
  });

  test("nearly complete -> 96% and a minute-scale remainder", () => {
    const p = industryJobProgress(
      job({
        startDate: new Date(NOW - 96 * 60_000).toISOString(),
        endDate: new Date(NOW + 4 * 60_000).toISOString(),
      }),
      NOW,
    );
    expect(p.percent).toBe(96);
    expect(p.remainingLabel).toBe("4m");
  });

  test("timer elapsed but still 'active' -> 100% / Ready, no negative countdown", () => {
    const p = industryJobProgress(
      job({ startDate: new Date(NOW - 7_200_000).toISOString(), endDate: new Date(NOW - 60_000).toISOString() }),
      NOW,
    );
    expect(p.percent).toBe(100);
    expect(p.state).toBe("ready");
    expect(p.remainingLabel).toBe("Ready");
    expect(p.remainingMs).toBe(0);
  });

  test("explicit ready status -> Ready even if timestamps are missing", () => {
    const p = industryJobProgress(job({ status: "ready", startDate: null, endDate: null }), NOW);
    expect(p.percent).toBe(100);
    expect(p.remainingLabel).toBe("Ready");
  });

  test("paused status -> Paused label", () => {
    const p = industryJobProgress(job({ status: "paused" }), NOW);
    expect(p.state).toBe("paused");
    expect(p.remainingLabel).toBe("Paused");
  });

  test("zero / invalid duration is defensive, not a divide-by-zero", () => {
    const sameInstant = new Date(NOW).toISOString();
    const p = industryJobProgress(job({ startDate: sameInstant, endDate: sameInstant }), NOW);
    expect(Number.isFinite(p.percent)).toBe(true);
    expect(p.percent).toBe(100); // now >= end, so it reads as done
    const future = industryJobProgress(
      job({ startDate: new Date(NOW + 1_000).toISOString(), endDate: new Date(NOW + 1_000).toISOString() }),
      NOW,
    );
    expect(future.percent).toBe(0);
  });
});

describe("formatJobRemaining", () => {
  test("uses the compact status vocabulary", () => {
    expect(formatJobRemaining(48 * 60_000)).toBe("48m");
    expect(formatJobRemaining((4 * 60 + 12) * 60_000)).toBe("4h 12m");
    expect(formatJobRemaining((26 * 60) * 60_000)).toBe("1d 2h");
    expect(formatJobRemaining((3 * 24 * 60 + 60) * 60_000)).toBe("3d 1h");
    expect(formatJobRemaining(30_000)).toBe("<1m");
    expect(formatJobRemaining(0)).toBe("<1m");
  });
});

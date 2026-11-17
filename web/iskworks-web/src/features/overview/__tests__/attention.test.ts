import { describe, expect, test } from "vitest";

import { activeJobs } from "../../jobs/__tests__/fixtures";
import { attentionChips, type AttentionSources } from "../attention";
import { hoursFromNow, marketOverview, NOW, planetaryOverview, rosterEntry, runway } from "./overview-fixtures";

const now = NOW.getTime();

function ready<T>(data: T) {
  return { data, error: null };
}

/** Every source loaded, with nothing that needs attention. */
function calm(): AttentionSources {
  return {
    characters: ready([rosterEntry()]),
    jobs: ready(
      activeJobs({
        summary: { running: 2, ready: 0, endingWithinDay: 0 },
        slots: { manufacturing: { used: 12, max: 12 }, reaction: { used: 10, max: 10 }, research: { used: 20, max: 20 } },
      }),
    ),
    market: ready(marketOverview()),
    planetary: ready(planetaryOverview()),
    runway: ready(runway()),
  };
}

function ids(sources: AttentionSources) {
  return attentionChips(sources, now).chips.map((chip) => chip.id);
}

describe("attentionChips", () => {
  test("nothing pending: no chips, nothing unavailable, nothing loading", () => {
    expect(attentionChips(calm(), now)).toEqual({ chips: [], unavailable: 0, pending: false });
  });

  test("a character that needs reconnecting, or with sync trouble", () => {
    const sources = calm();
    sources.characters = ready([
      rosterEntry({ health: "reconnectRequired" }),
      rosterEntry({ health: "stale" }),
      rosterEntry({ health: "partialPerms" }),
      rosterEntry({ health: "syncPending" }),
    ]);
    const { chips } = attentionChips(sources, now);

    expect(chips.find((chip) => chip.id === "reconnect")).toMatchObject({
      tone: "critical",
      count: 1,
      label: "character needs reconnect",
      to: "/characters",
    });
    expect(chips.find((chip) => chip.id === "syncIssues")).toMatchObject({ tone: "warn", count: 2, label: "characters with sync issues" });
  });

  test("an empty skill queue, but not a paused one or one without the queue scope", () => {
    const sources = calm();
    sources.characters = ready([
      rosterEntry({ trainingQueue: [] }),
      rosterEntry({
        trainingQueue: [{ ...rosterEntry().trainingQueue[0]!, startDate: hoursFromNow(-30), finishDate: hoursFromNow(-2) }],
      }),
      rosterEntry({ trainingQueue: [{ ...rosterEntry().trainingQueue[0]!, startDate: null, finishDate: null }] }),
      rosterEntry({ trainingQueue: [], trainingQueueScopeMissing: true }),
    ]);

    expect(attentionChips(sources, now).chips.find((chip) => chip.id === "queueEmpty")).toMatchObject({
      tone: "critical",
      count: 2,
      label: "skill queues empty",
      to: "/skills",
    });
  });

  test("a skill queue that ends within a day", () => {
    const sources = calm();
    const entry = rosterEntry().trainingQueue[0]!;
    sources.characters = ready([
      rosterEntry({ trainingQueue: [{ ...entry, finishDate: hoursFromNow(5) }] }),
      rosterEntry({ trainingQueue: [{ ...entry, finishDate: hoursFromNow(5) }, { ...entry, queuePosition: 1, startDate: hoursFromNow(5), finishDate: hoursFromNow(40) }] }),
    ]);

    expect(attentionChips(sources, now).chips.find((chip) => chip.id === "queueEnding")).toMatchObject({
      tone: "warn",
      count: 1,
      label: "skill queue ends < 24h",
    });
  });

  test("ready jobs and idle slots, skipping slots whose max isn't known", () => {
    const sources = calm();
    sources.jobs = ready(
      activeJobs({
        summary: { running: 3, ready: 4, endingWithinDay: 1 },
        slots: { manufacturing: { used: 8, max: 12 }, reaction: { used: 1, max: null }, research: { used: 18, max: 20 } },
      }),
    );
    const { chips } = attentionChips(sources, now);

    expect(chips.find((chip) => chip.id === "jobsReady")).toMatchObject({ tone: "info", count: 4, label: "jobs ready to deliver", to: "/jobs" });
    expect(chips.find((chip) => chip.id === "idleSlots")).toMatchObject({ tone: "info", count: 6, label: "idle slots" });
  });

  test("outbid and likely-filled market orders", () => {
    const sources = calm();
    sources.market = ready(marketOverview({ summary: { ...marketOverview().summary, outbid: 7, likelyFilled: 1 } }));
    const { chips } = attentionChips(sources, now);

    expect(chips.find((chip) => chip.id === "outbid")).toMatchObject({ tone: "warn", count: 7, label: "orders outbid", to: "/market-orders?outbid=1" });
    expect(chips.find((chip) => chip.id === "likelyFilled")).toMatchObject({ tone: "info", count: 1, label: "order likely filled" });
  });

  test("planetary alerts, one chip each", () => {
    const sources = calm();
    sources.planetary = ready(
      planetaryOverview({ summary: { ...planetaryOverview().summary, alerts: { expired: 2, storageFull: 1, starved: 3 } } }),
    );
    const { chips } = attentionChips(sources, now);

    expect(chips.find((chip) => chip.id === "extractorsExpired")).toMatchObject({ tone: "critical", count: 2, label: "extractors expired", to: "/planetary" });
    expect(chips.find((chip) => chip.id === "storageFull")).toMatchObject({ tone: "warn", count: 1, label: "storage full" });
    expect(chips.find((chip) => chip.id === "starved")).toMatchObject({ tone: "warn", count: 3, label: "factories starved" });
  });

  test("blocked Epics", () => {
    const sources = calm();
    sources.runway = ready(runway({ kpis: { ...runway().kpis, epicsBlocked: 1 } }));

    expect(attentionChips(sources, now).chips.find((chip) => chip.id === "epicsBlocked")).toMatchObject({
      tone: "warn",
      count: 1,
      label: "Epic blocked",
      to: "/inventory/runway",
    });
  });

  test("chips run critical, then warn, then info", () => {
    const sources = calm();
    sources.jobs = ready(activeJobs({ summary: { running: 1, ready: 2, endingWithinDay: 0 } }));
    sources.market = ready(marketOverview({ summary: { ...marketOverview().summary, outbid: 1 } }));
    sources.characters = ready([rosterEntry({ health: "reconnectRequired" })]);

    expect(ids(sources).slice(0, 2)).toEqual(["reconnect", "outbid"]);
    expect(attentionChips(sources, now).chips.map((chip) => chip.tone)).toEqual(
      [...attentionChips(sources, now).chips.map((chip) => chip.tone)].sort(
        (a, b) => ["critical", "warn", "info"].indexOf(a) - ["critical", "warn", "info"].indexOf(b),
      ),
    );
  });

  test("a failed source counts as unavailable and adds no chips", () => {
    const sources = calm();
    sources.market = { data: null, error: new Error("boom") };
    sources.planetary = { data: null, error: new Error("boom") };

    expect(attentionChips(sources, now)).toMatchObject({ unavailable: 2, pending: false });
  });

  test("a source still loading is pending, neither a chip nor unavailable", () => {
    const sources = calm();
    sources.jobs = { data: null, error: null };

    expect(attentionChips(sources, now)).toEqual({ chips: [], unavailable: 0, pending: true });
  });
});

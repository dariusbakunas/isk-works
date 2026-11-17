import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { activeEpics, EpicsPanel } from "../epics-panel";
import { OverviewDataProvider } from "../overview-data";
import { epic, fetchByUrl, hoursFromNow, NOW, runway } from "./overview-fixtures";

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
        <EpicsPanel />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

describe("activeEpics", () => {
  test("open Epics only, oldest first, capped", () => {
    const orders = [
      epic({ id: "new", createdAt: hoursFromNow(-1) }),
      epic({ id: "done", status: "complete" }),
      epic({ id: "old", createdAt: hoursFromNow(-100), status: "blocked" }),
      epic({ id: "gone", status: "canceled" }),
      epic({ id: "mid", createdAt: hoursFromNow(-50), status: "ready" }),
    ];

    expect(activeEpics(orders, 6).map((order) => order.id)).toEqual(["old", "mid", "new"]);
    expect(activeEpics(orders, 2).map((order) => order.id)).toEqual(["old", "mid"]);
  });
});

describe("EpicsPanel", () => {
  test("each Epic shows its status, progress, what can start and its material cover", async () => {
    fetchByUrl({
      orders: [
        epic({ id: "rorqual", displayName: "Rorqual ×1", status: "blocked", createdAt: hoursFromNow(-300) }),
        epic({ id: "ishtars", displayName: "Doctrine Ishtars ×20", rollup: { satisfied: 43, needsAction: 17, inProgress: 0, total: 60, readyToStart: 8 } }),
        epic({ id: "untracked", displayName: "Athanor core", status: "ready", createdAt: hoursFromNow(-1) }),
      ],
      runway: runway({
        kpis: { ...runway().kpis, epicsBlocked: 1, epicsTotal: 3, shortfallValue: "184000000.0000" },
        epics: [
          { orderId: "rorqual", displayName: "Rorqual ×1", createdAt: hoursFromNow(-300), coveredPercent: 61, shortValue: "184000000", blocked: true },
          { orderId: "ishtars", displayName: "Doctrine Ishtars ×20", createdAt: hoursFromNow(-240), coveredPercent: 96.4, shortValue: "0", blocked: false },
        ],
      }),
    });
    renderPanel();

    const panel = await screen.findByRole("region", { name: "Epics" });
    expect(await within(panel).findByText("1 of 3 blocked · short 184M")).toBeInTheDocument();

    const rorqual = within(panel).getByText("Rorqual ×1").closest("a")!;
    expect(rorqual).toHaveAttribute("href", "/board?epic=rorqual");
    expect(within(rorqual).getByText("Blocked")).toBeInTheDocument();
    expect(within(rorqual).getByText("31 / 48")).toBeInTheDocument();
    expect(within(rorqual).getByText("5 ready to start")).toBeInTheDocument();
    expect(within(rorqual).getByText("61% · blocked")).toBeInTheDocument();

    const ishtars = within(panel).getByText("Doctrine Ishtars ×20").closest("a")!;
    expect(within(ishtars).getByText("In progress")).toBeInTheDocument();
    expect(within(ishtars).getByText("96% covered")).toBeInTheDocument();

    const athanor = within(panel).getByText("Athanor core").closest("a")!;
    expect(within(athanor).getByText("Ready")).toBeInTheDocument();
    expect(within(athanor).getByText("—")).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: "View all Epics" })).toHaveAttribute("href", "/board");
  });

  test("no open Epics", async () => {
    fetchByUrl({ orders: [epic({ status: "complete" })] });
    renderPanel();

    expect(await screen.findByText("No active Epics.")).toBeInTheDocument();
  });
});

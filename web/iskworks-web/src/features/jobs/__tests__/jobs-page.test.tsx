import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { ActiveJobs, JobHistory } from "../../../api/jobs";
import { JobsPage } from "../jobs-page";
import { activeJobs, jobHistory, NOW, TICKET } from "./fixtures";
import { WithToolWindows } from "../../../test/tool-windows";

function stubFetch(active: ActiveJobs = activeJobs(), history: JobHistory = jobHistory()) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const url = String(input);
    const body = url.includes("/api/jobs/history") ? history : active;
    return new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
  });
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}{location.search}</div>;
}

function renderPage(path = "/jobs", wrapper?: typeof WithToolWindows) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<><JobsPage /><LocationProbe /></>} path="/jobs/*" />
      </Routes>
    </MemoryRouter>,
    { wrapper },
  );
}

function rowFor(table: HTMLElement, name: string) {
  return within(table).getByText(name).closest("tr")!;
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Jobs › Active", () => {
  test("shows the summary, slot bars and jobs grouped by character", async () => {
    stubFetch();
    renderPage();

    expect(screen.getByRole("status", { name: "Loading jobs" })).toBeInTheDocument();
    const kpis = await screen.findAllByTestId("kpi-card");
    expect(kpis[0]).toHaveTextContent("3");
    expect(kpis[1]).toHaveTextContent("Ready to deliver1");
    expect(screen.getAllByTestId("slot-bar")[0]).toHaveTextContent("1/24");
    expect(screen.getByText("Synced 12 min ago")).toBeInTheDocument();

    const table = screen.getByRole("table", { name: "Active jobs" });
    const headers = within(table).getAllByRole("rowheader").map((header) => header.textContent);
    expect(headers[0]).toContain("Kira Vantor");
    expect(headers[0]).toContain("1/12 mfg · 1/10 rxn · 0/20 rsc");
    expect(headers[1]).toContain("Mox Halden");

    const ishtar = rowFor(table, "Ishtar Blueprint");
    expect(within(ishtar).getByText("3h 0m")).toBeInTheDocument();
    expect(within(ishtar).getByRole("link", { name: "TK-142" })).toHaveAttribute("href", `/board?ticket=${TICKET}`);
    expect(within(ishtar).getByTitle("12,400,000 ISK")).toHaveTextContent("12.4M");

    const fernite = rowFor(table, "Fernite Carbide Reaction Formula");
    expect(within(fernite).getAllByText("Ready")).toHaveLength(2);
    expect(within(fernite).getByText("Untracked")).toBeInTheDocument();

    const revelation = rowFor(table, "Revelation Blueprint");
    expect(within(revelation).getByText("+5 ME levels")).toBeInTheDocument();
    expect(within(revelation).getAllByText("Paused")).toHaveLength(2);
  });

  test("asks a character missing the jobs permission to re-authorize", async () => {
    stubFetch();
    renderPage();
    const notice = await screen.findByText(/is missing the industry jobs permission/);
    expect(notice.closest("[role=status]")).toHaveTextContent("Mox Halden");
    expect(screen.getByRole("button", { name: "Re-authorize" })).toBeInTheDocument();
  });

  test("groups by activity and filters by character, activity and search", async () => {
    stubFetch();
    renderPage();
    const table = await screen.findByRole("table", { name: "Active jobs" });

    fireEvent.click(within(screen.getByRole("group", { name: "Group by" })).getByRole("button", { name: "Activity" }));
    expect(within(table).getAllByRole("rowheader").map((header) => header.textContent)).toEqual([
      expect.stringContaining("Manufacturing"),
      expect.stringContaining("Reaction"),
      expect.stringContaining("ME research"),
    ]);
    expect(screen.getByTestId("location")).toHaveTextContent("group=activity");

    fireEvent.click(within(screen.getByRole("group", { name: "Characters" })).getByRole("button", { name: "Mox Halden" }));
    expect(within(table).queryByText("Revelation Blueprint")).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Activity"), { target: { value: "reaction" } });
    expect(within(table).queryByText("Ishtar Blueprint")).not.toBeInTheDocument();
    expect(within(table).getByText("Fernite Carbide Reaction Formula")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Search jobs"), { target: { value: "ishtar" } });
    expect(await screen.findByText("No matching jobs")).toBeInTheDocument();
  });

  test("a row opens the job inspector with its Board ticket", async () => {
    stubFetch();
    renderPage();
    const table = await screen.findByRole("table", { name: "Active jobs" });

    fireEvent.click(rowFor(table, "Ishtar Blueprint"));

    expect(screen.getByTestId("location")).toHaveTextContent("job=1");
    const inspector = screen.getByRole("complementary", { name: "Ishtar Blueprint" });
    expect(within(inspector).getByText("Running · 70%")).toBeInTheDocument();
    expect(within(inspector).getByText("12,400,000 ISK")).toBeInTheDocument();
    expect(within(inspector).getByRole("link", { name: /TK-142/ })).toHaveAttribute("href", `/board?ticket=${TICKET}`);

    fireEvent.click(within(inspector).getByRole("button", { name: "Close job inspector" }));
    expect(screen.queryByRole("complementary", { name: "Ishtar Blueprint" })).not.toBeInTheDocument();
  });

  test("warns about characters that haven't synced in over an hour", async () => {
    stubFetch(activeJobs({ characters: [activeJobs().characters[0], { ...activeJobs().characters[1], syncState: "stale" }] }));
    renderPage();
    expect(await screen.findByText("1 character hasn't synced in over an hour")).toBeInTheDocument();
  });

  test("with no running jobs it shows every character's free slots", async () => {
    stubFetch(activeJobs({ jobs: [], summary: { running: 0, ready: 0, endingWithinDay: 0 } }));
    renderPage();
    expect(await screen.findByText("All slots idle")).toBeInTheDocument();
    expect(screen.getByText(/0\/12 mfg · 0\/10 rxn · 1\/25 rsc/)).toBeInTheDocument();
  });

  test("with no characters it points to connecting one", async () => {
    stubFetch(activeJobs({ characters: [], jobs: [] }));
    renderPage();
    expect(await screen.findByText("No characters connected")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Connect a character" })).toHaveAttribute("href", "/characters");
  });
});

describe("Jobs › History", () => {
  test("loads the last 30 days with KPIs, chart and completed jobs", async () => {
    const fetchMock = stubFetch();
    renderPage("/jobs/history");

    const kpis = await screen.findAllByTestId("kpi-card");
    expect(kpis[0]).toHaveTextContent("3");
    expect(kpis[2]).toHaveTextContent("14.6M ISK");
    expect(kpis[3]).toHaveTextContent("40%");
    expect(kpis[3]).toHaveTextContent("4/10 runs");
    const historyUrl = fetchMock.mock.calls.map(([url]) => String(url)).find((url) => url.includes("/api/jobs/history"))!;
    const query = new URL(historyUrl, "http://x").searchParams;
    expect([query.get("from"), query.get("to")]).toEqual(["2026-09-21", "2026-10-20"]);

    expect(screen.getByRole("img", { name: /Jobs completed per week: 3 in 2 weeks/ })).toBeInTheDocument();
    const table = screen.getByRole("table", { name: "Completed jobs" });
    const cerberus = rowFor(table, "Cerberus Blueprint");
    expect(within(cerberus).getByText("4/10")).toBeInTheDocument();
    expect(within(cerberus).getByText("Delivered")).toBeInTheDocument();
    expect(within(table).getByText("Cancelled")).toBeInTheDocument();
    expect(screen.getByText(/ISK Works keeps them from the day it first saw them \(Oct 20\)/)).toBeInTheDocument();
  });

  test("the 7d range refetches and is kept in the URL", async () => {
    const fetchMock = stubFetch();
    renderPage("/jobs/history");
    await screen.findAllByTestId("kpi-card");

    fireEvent.click(within(screen.getByRole("group", { name: "Range" })).getByRole("button", { name: "7d" }));

    await waitFor(() => {
      const calls = fetchMock.mock.calls.map(([url]) => String(url)).filter((url) => url.includes("/api/jobs/history"));
      expect(new URL(calls.at(-1)!, "http://x").searchParams.get("from")).toBe("2026-10-14");
    });
    expect(screen.getByTestId("location")).toHaveTextContent("/jobs/history?range=7d");
  });

  test("before any job has been stored it explains where history comes from", async () => {
    stubFetch(activeJobs(), jobHistory({ jobs: [], earliestObservedAt: null }));
    renderPage("/jobs/history");
    expect(await screen.findByText("No history yet")).toBeInTheDocument();
  });
});

test("items open in Type Info from the table", async () => {
  window.localStorage.clear();
  stubFetch();
  renderPage("/jobs", WithToolWindows);
  const table = await screen.findByRole("table", { name: "Active jobs" });
  const buttons = await within(table).findAllByRole("button", { name: /^Show info: / });
  const name = buttons[0].getAttribute("aria-label")!.replace("Show info: ", "");
  fireEvent.click(buttons[0]);
  expect(screen.getByRole("dialog", { name })).toBeInTheDocument();
});

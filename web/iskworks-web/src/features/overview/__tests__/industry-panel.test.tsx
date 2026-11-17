import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { activeJobs, job, NOW as JOBS_NOW } from "../../jobs/__tests__/fixtures";
import { IndustryPanel } from "../industry-panel";
import { OverviewDataProvider } from "../overview-data";
import { fetchByUrl } from "./overview-fixtures";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(JOBS_NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function renderPanel() {
  return render(
    <MemoryRouter>
      <OverviewDataProvider>
        <IndustryPanel />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

async function region() {
  return screen.findByRole("region", { name: "Industry" });
}

describe("IndustryPanel", () => {
  test("headline counts and a slot meter per activity", async () => {
    fetchByUrl({
      jobs: activeJobs({
        summary: { running: 23, ready: 4, endingWithinDay: 9 },
        slots: { manufacturing: { used: 18, max: 22 }, reaction: { used: 5, max: null }, research: { used: 11, max: 11 } },
      }),
    });
    renderPanel();

    const panel = await region();
    const totals = await within(panel).findByRole("group", { name: "Job totals" });
    expect(within(totals).getByText("Running").nextSibling).toHaveTextContent("23");
    expect(within(totals).getByText("Ready").nextSibling).toHaveTextContent("4");
    expect(within(totals).getByText("Ending < 24h").nextSibling).toHaveTextContent("9");

    const manufacturing = within(panel).getByRole("meter", { name: "Manufacturing slots" });
    expect(manufacturing).toHaveAttribute("aria-valuenow", "18");
    expect(manufacturing).toHaveAttribute("aria-valuemax", "22");
    expect(within(panel).getByText("4 free")).toBeInTheDocument();
    expect(within(panel).getByText("full")).toBeInTheDocument();
    expect(within(panel).getByText("— (skills not synced)")).toBeInTheDocument();
  });

  test("the soonest jobs show who, where, and the countdown or Ready", async () => {
    fetchByUrl();
    renderPanel();

    const panel = await region();
    const ready = (await within(panel).findByText(/Fernite Carbide/)).closest("li")!;
    expect(within(ready).getByText("Ready")).toBeInTheDocument();
    const ishtar = within(panel).getByText(/Ishtar ×2/).closest("li")!;
    expect(within(ishtar).getByText("3h 0m")).toBeInTheDocument();
    expect(within(ishtar).getByText("Kira Vantor")).toBeInTheDocument();
    expect(within(ishtar).getByText("Perimeter")).toBeInTheDocument();
    expect(within(ishtar).getByRole("link", { name: "TK-142" })).toHaveAttribute("href", "/board?ticket=99999999-9999-9999-9999-999999999999");
    expect(within(panel).getByRole("link", { name: "View all Industry" })).toHaveAttribute("href", "/jobs");
  });

  test("at most five jobs are listed", async () => {
    fetchByUrl({ jobs: activeJobs({ jobs: Array.from({ length: 8 }, (_, index) => job({ jobId: index + 1, ticket: null })) }) });
    renderPanel();

    const panel = await region();
    expect(await within(panel).findAllByRole("listitem")).toHaveLength(5);
  });

  test("no jobs running keeps the slot meters", async () => {
    fetchByUrl({ jobs: activeJobs({ jobs: [], summary: { running: 0, ready: 0, endingWithinDay: 0 } }) });
    renderPanel();

    const panel = await region();
    expect(await within(panel).findByText("No jobs running.")).toBeInTheDocument();
    expect(within(panel).getByRole("meter", { name: "Manufacturing slots" })).toBeInTheDocument();
  });
});

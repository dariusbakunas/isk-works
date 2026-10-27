import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { FIT_ID, meta, stats, statsView, stubFetch } from "./fixtures";
import { renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

async function rail() {
  const panel = await screen.findByRole("complementary", { name: "Fit stats" });
  await within(panel).findByText("Resources");
  return panel;
}

describe("Fittings › stats", () => {
  test("resources show used of total, and run red when over", async () => {
    const over = stats();
    over.resources.cpu = { used: 380, total: 362.5 };
    stubFetch({ statsView: statsView({ stats: over }) });
    renderPage(`/fittings/${FIT_ID}`);

    const panel = await rail();
    const cpu = within(panel).getByRole("meter", { name: "CPU" });
    expect(cpu).toHaveAttribute("aria-valuenow", "380");
    expect(cpu).toHaveAttribute("data-over", "true");
    expect(within(panel).getByRole("meter", { name: "Powergrid" })).toHaveAttribute("data-over", "false");
    expect(within(panel).getByText("380 / 362.5 tf")).toBeInTheDocument();
    expect(within(panel).getByText("216.5 / 250 MW")).toBeInTheDocument();
  });

  test("the capacitor either depletes or settles", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    expect(within(await rail()).getByText("Depletes in 3:10")).toBeInTheDocument();
  });

  test("a stable capacitor shows where it settles", async () => {
    const stable = stats();
    stable.capacitor.state = { kind: "stable", level: 0.735 };
    stubFetch({ statsView: statsView({ stats: stable }) });
    renderPage(`/fittings/${FIT_ID}`);
    expect(within(await rail()).getByText("Stable 73.5%")).toBeInTheDocument();
  });

  test("offense, defense, targeting and navigation read off the calculation", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const panel = await rail();

    expect(within(panel).getByTestId("total-dps")).toHaveTextContent("215.9");
    expect(within(panel).getByTestId("total-ehp")).toHaveTextContent("90,790");
    const grid = within(panel).getByRole("table", { name: "Resists" });
    const shield = within(grid).getByRole("row", { name: /Shield/ });
    expect(within(shield).getAllByRole("cell").map((cell) => cell.textContent)).toEqual(["81.7%", "92.9%", "89.4%", "82.3%", "9,900"]);
    expect(within(panel).getByText("22.4 km")).toBeInTheDocument();
    expect(within(panel).getByText("655.3 m/s")).toBeInTheDocument();
    expect(within(panel).getByText("19.71 s")).toBeInTheDocument();
    expect(within(panel).getByText("3.5 AU/s")).toBeInTheDocument();
  });

  test("Why? lists what changed a stat, in order, from base to final", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const panel = await rail();

    fireEvent.click(within(panel).getByRole("button", { name: "Why? CPU" }));
    const popover = await screen.findByRole("dialog", { name: "Why CPU output?" });
    expect(await within(popover).findByText("CPU Management")).toBeInTheDocument();
    expect(within(popover).getByText("+25%")).toBeInTheDocument();
    expect(within(popover).getByText("Co-Processor II")).toBeInTheDocument();
    expect(within(popover).getByText("stacking penalised to 86.9%")).toBeInTheDocument();
    expect(within(popover).getByTestId("why-base")).toHaveTextContent("290");
    expect(within(popover).getByTestId("why-final")).toHaveTextContent("362.5");
    await waitFor(() =>
      expect(
        fetchSpy.mock.calls.some(([input]) =>
          String(input).includes(`/api/fittings/${FIT_ID}/explain?target=ship&attribute=cpuOutput&pilot=allV&profile=uniform`),
        ),
      ).toBe(true),
    );
  });

  test("without engine data the rail says so and shows no numbers", async () => {
    stubFetch({ meta: meta(false) });
    renderPage(`/fittings/${FIT_ID}`);

    const panel = await screen.findByRole("complementary", { name: "Fit stats" });
    expect(await within(panel).findByText("Stats unavailable")).toBeInTheDocument();
    expect(within(panel).queryByRole("meter")).not.toBeInTheDocument();
  });
});

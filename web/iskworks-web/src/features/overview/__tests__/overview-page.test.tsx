import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import { OverviewPage } from "../overview-page";
import { fetchByUrl, sourceUrls } from "./overview-fixtures";

afterEach(() => {
  vi.unstubAllGlobals();
});

function renderPage() {
  return render(
    <MemoryRouter>
      <OverviewPage />
    </MemoryRouter>,
  );
}

describe("OverviewPage", () => {
  test("lays out the attention strip and the panels", async () => {
    fetchByUrl();
    renderPage();

    expect(await screen.findByRole("heading", { name: "Overview" })).toBeInTheDocument();
    for (const name of ["Attention", "Next 48 h", "Industry", "Market", "Money", "Epics", "Planetary", "Characters"]) {
      expect(await screen.findByRole("region", { name })).toBeInTheDocument();
    }
    expect(await screen.findByText(/^Updated /)).toBeInTheDocument();
  });

  test("Refresh fetches every source again", async () => {
    const fetchMock = fetchByUrl();
    renderPage();
    await screen.findByText(/^Updated /);

    await userEvent.setup().click(screen.getByRole("button", { name: "Refresh overview" }));

    await waitFor(() => {
      for (const url of sourceUrls) {
        expect(fetchMock.mock.calls.filter(([input]) => String(input).includes(url))).toHaveLength(2);
      }
    });
  });

  test("with every source but the roster failing, each panel says so and the strip counts them", async () => {
    const down = new Error("unavailable");
    fetchByUrl({ jobs: down, market: down, planetary: down, runway: down, calendar: down, finance: down, orders: down });
    renderPage();

    expect(await screen.findByText("4 sources unavailable")).toBeInTheDocument();
    for (const name of ["Next 48 h", "Industry", "Market", "Money", "Epics", "Planetary"]) {
      const region = screen.getByRole("region", { name });
      expect(await within(region).findByRole("button", { name: "Retry" })).toBeInTheDocument();
    }
    expect(screen.queryByText(/All clear/)).not.toBeInTheDocument();
  });

  test("while the roster loads, the panels show their loading state", () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise(() => undefined)));
    renderPage();

    expect(screen.getByRole("status", { name: "Loading Attention" })).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Loading Industry" })).toBeInTheDocument();
  });

  test("a roster failure is shown once, not as a wall of panels", async () => {
    fetchByUrl({ characters: new Error("roster down") });
    renderPage();

    expect(await screen.findByText("Couldn't load your characters")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Industry" })).not.toBeInTheDocument();
  });
});

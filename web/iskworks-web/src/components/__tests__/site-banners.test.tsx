import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { formatLead, SiteBanners } from "../site-banners";

function mockHealth(body: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } })),
  );
}

describe("SiteBanners", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    document.title = "";
    document.documentElement.style.removeProperty("--iw-banner-h");
  });

  test("shows the label and prefixes the tab title when the API reports one", async () => {
    document.title = "ISK Works";
    mockHealth({ status: "healthy", version: "dev", environmentLabel: "Stage" });

    render(<SiteBanners />);

    expect(await screen.findByRole("status")).toHaveTextContent(/stage environment/i);
    // The title is set by an effect that runs after the banner renders.
    await waitFor(() => expect(document.title).toBe("[STAGE] ISK Works"));
  });

  test("publishes its height so viewport-height layouts can leave room for it", async () => {
    mockHealth({ status: "healthy", version: "dev", environmentLabel: "Stage" });

    const { unmount } = render(<SiteBanners />);

    await screen.findByRole("status");
    await waitFor(() =>
      expect(document.documentElement.style.getPropertyValue("--iw-banner-h")).toMatch(/^\d+px$/),
    );
    unmount();
    expect(document.documentElement.style.getPropertyValue("--iw-banner-h")).toBe("");
  });

  test("renders nothing on production (no label)", async () => {
    document.title = "ISK Works";
    mockHealth({ status: "healthy", version: "v1.0.0" });

    const { container } = render(<SiteBanners />);

    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
    expect(document.title).toBe("ISK Works");
    expect(document.documentElement.style.getPropertyValue("--iw-banner-h")).toBe("");
  });

  test("renders nothing when the health check fails", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new Error("offline"))));

    const { container } = render(<SiteBanners />);

    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  describe("maintenance notice", () => {
    function mockApi(health: unknown, notices: () => unknown) {
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL) => {
          const body = String(input).endsWith("/api/maintenance-notice")
            ? notices()
            : health;
          if (body instanceof Error) throw body;
          return new Response(JSON.stringify(body), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }),
      );
    }

    const production = { status: "healthy", version: "v1.0.0" };
    const inTenMinutes = () => new Date(Date.now() + 10 * 60_000).toISOString();

    test("shows the message and how long is left, and publishes its height on production", async () => {
      mockApi(production, () => ({ notice: { message: "Please save your work.", startsAt: inTenMinutes() } }));

      render(<SiteBanners />);

      const banner = await screen.findByRole("status");
      expect(banner).toHaveTextContent(/in about 10 minutes/);
      expect(banner).toHaveTextContent("Please save your work.");
      await waitFor(() =>
        expect(document.documentElement.style.getPropertyValue("--iw-banner-h")).toMatch(/^\d+px$/),
      );
    });

    test("stacks with the environment marker in one measured wrapper", async () => {
      mockApi(
        { status: "healthy", version: "dev", environmentLabel: "Stage" },
        () => ({ notice: { message: "Please save your work.", startsAt: inTenMinutes() } }),
      );

      render(<SiteBanners />);

      await waitFor(() => expect(screen.getAllByRole("status")).toHaveLength(2));
      const [environment, maintenance] = screen.getAllByRole("status");
      expect(environment).toHaveTextContent(/stage environment/i);
      expect(maintenance).toHaveTextContent("Please save your work.");
      expect(environment.parentElement).toBe(maintenance.parentElement);
    });

    test("an open tab picks up a new notice on the next poll", async () => {
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
      let notice: unknown = null;
      mockApi(production, () => ({ notice }));

      render(<SiteBanners />);
      await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
      expect(screen.queryByRole("status")).not.toBeInTheDocument();

      notice = { message: "Please save your work.", startsAt: inTenMinutes() };
      await vi.advanceTimersByTimeAsync(60_000);

      expect(await screen.findByRole("status")).toHaveTextContent("Please save your work.");
    });

    test("disappears at its start time without waiting for the API", async () => {
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
      const startsAt = new Date(Date.now() + 30_000).toISOString();
      let calls = 0;
      mockApi(production, () => {
        calls += 1;
        return calls === 1 ? { notice: { message: "Soon.", startsAt } } : new Error("offline");
      });

      render(<SiteBanners />);
      expect(await screen.findByRole("status")).toHaveTextContent("Soon.");

      await vi.advanceTimersByTimeAsync(31_000);

      await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
    });

    test("a failed poll keeps the notice it already has", async () => {
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
      let calls = 0;
      const startsAt = inTenMinutes();
      mockApi(production, () => {
        calls += 1;
        return calls === 1 ? { notice: { message: "Soon.", startsAt } } : new Error("offline");
      });

      render(<SiteBanners />);
      expect(await screen.findByRole("status")).toHaveTextContent("Soon.");

      await vi.advanceTimersByTimeAsync(60_000);
      await waitFor(() => expect(calls).toBeGreaterThan(1));
      // Let the rejected poll settle before checking it changed nothing.
      await vi.advanceTimersByTimeAsync(0);

      expect(screen.getByRole("status")).toHaveTextContent("Soon.");
    });

    test.each([
      [20_000, "in less than a minute"],
      [60_000, "in about a minute"],
      [10 * 60_000, "in about 10 minutes"],
      [3 * 3_600_000, "in about 3 hours"],
      [72 * 3_600_000, "in about 3 days"],
    ])("describes %i ms as %s", (remaining, expected) => {
      expect(formatLead(remaining)).toBe(expected);
    });
  });
});

import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { EnvironmentBanner } from "../environment-banner";

function mockHealth(body: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } })),
  );
}

describe("EnvironmentBanner", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    document.title = "";
    document.documentElement.style.removeProperty("--iw-banner-h");
  });

  test("shows the label and prefixes the tab title when the API reports one", async () => {
    document.title = "ISK Works";
    mockHealth({ status: "healthy", version: "dev", environmentLabel: "Stage" });

    render(<EnvironmentBanner />);

    expect(await screen.findByRole("status")).toHaveTextContent(/stage environment/i);
    // The title is set by an effect that runs after the banner renders.
    await waitFor(() => expect(document.title).toBe("[STAGE] ISK Works"));
  });

  test("publishes its height so viewport-height layouts can leave room for it", async () => {
    mockHealth({ status: "healthy", version: "dev", environmentLabel: "Stage" });

    const { unmount } = render(<EnvironmentBanner />);

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

    const { container } = render(<EnvironmentBanner />);

    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
    expect(document.title).toBe("ISK Works");
    expect(document.documentElement.style.getPropertyValue("--iw-banner-h")).toBe("");
  });

  test("renders nothing when the health check fails", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new Error("offline"))));

    const { container } = render(<EnvironmentBanner />);

    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });
});

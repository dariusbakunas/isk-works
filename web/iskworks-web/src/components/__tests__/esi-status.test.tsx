import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { EsiStatusItem } from "../esi-status";

function mockStatuses(...bodies: unknown[]) {
  const responses = [...bodies];
  const fetchMock = vi.fn(async () => {
    const body = responses.length > 1 ? responses.shift() : responses[0];
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("EsiStatusItem", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  test("reports paused data while ESI is down", async () => {
    mockStatuses({ downtime: true, retryAfterSeconds: 30 });

    render(<EsiStatusItem expanded />);

    const status = screen.getByRole("status");
    await waitFor(() => expect(status).toHaveTextContent("ESI data paused"));
    expect(status).toHaveAttribute("title", expect.stringMatching(/servers are down/i));
  });

  test("reports a connected ESI while it is up", async () => {
    const fetchMock = mockStatuses({ downtime: false, retryAfterSeconds: null });

    render(<EsiStatusItem expanded />);

    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/esi/status", expect.anything()));
    expect(screen.getByRole("status")).toHaveTextContent("ESI connected");
  });

  test("keeps the label for screen readers when collapsed", async () => {
    mockStatuses({ downtime: false, retryAfterSeconds: null });

    render(<EsiStatusItem expanded={false} />);

    expect(screen.getByText("ESI connected")).toHaveClass("sr-only");
  });

  test("recovers on its own once ESI is back", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mockStatuses(
      { downtime: true, retryAfterSeconds: 30 },
      { downtime: false, retryAfterSeconds: null },
    );

    render(<EsiStatusItem expanded />);
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("ESI data paused"));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("ESI connected"));
  });
});

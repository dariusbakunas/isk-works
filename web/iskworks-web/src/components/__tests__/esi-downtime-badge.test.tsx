import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { EsiDowntimeBadge } from "../esi-downtime-badge";

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

describe("EsiDowntimeBadge", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  test("shows a downtime badge while ESI is paused", async () => {
    mockStatuses({ downtime: true, retryAfterSeconds: 30 });

    render(<EsiDowntimeBadge />);

    const badge = await screen.findByRole("status");
    expect(badge).toHaveTextContent(/eve downtime/i);
    expect(badge).toHaveAttribute("title", expect.stringMatching(/daily downtime/i));
  });

  test("renders nothing while ESI is up", async () => {
    const fetchMock = mockStatuses({ downtime: false, retryAfterSeconds: null });

    const { container } = render(<EsiDowntimeBadge />);

    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/esi/status", expect.anything()));
    expect(container).toBeEmptyDOMElement();
  });

  test("disappears on its own once ESI is back", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mockStatuses(
      { downtime: true, retryAfterSeconds: 30 },
      { downtime: false, retryAfterSeconds: null },
    );

    render(<EsiDowntimeBadge />);
    expect(await screen.findByRole("status")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
  });
});

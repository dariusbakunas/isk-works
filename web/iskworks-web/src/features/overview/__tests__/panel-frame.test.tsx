import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, test, vi } from "vitest";

import type { Loaded } from "../../../hooks/use-load";
import { PanelFrame, panelState } from "../panel-frame";

function renderFrame(ui: React.ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>);
}

function loaded<T>(overrides: Partial<Loaded<T>>): Loaded<T> {
  return {
    data: null,
    loading: false,
    refreshing: false,
    error: null,
    errorMessage: null,
    retry: vi.fn(),
    reload: vi.fn(),
    ...overrides,
  };
}

describe("PanelFrame", () => {
  test("a ready panel shows its title, stamp, link and content", () => {
    renderFrame(
      <PanelFrame link={{ to: "/jobs" }} stamp="synced 12 min ago" state={{ kind: "ready" }} title="Industry">
        <p>slot bars</p>
      </PanelFrame>,
    );

    expect(screen.getByRole("region", { name: "Industry" })).toBeInTheDocument();
    expect(screen.getByText("synced 12 min ago")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "View all Industry" })).toHaveAttribute("href", "/jobs");
    expect(screen.getByText("slot bars")).toBeInTheDocument();
  });

  test("without a link there is no View all", () => {
    renderFrame(<PanelFrame state={{ kind: "ready" }} title="Attention">chips</PanelFrame>);

    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  test("loading shows a skeleton instead of the content", () => {
    renderFrame(<PanelFrame state={{ kind: "loading" }} title="Market">content</PanelFrame>);

    expect(screen.getByRole("status", { name: "Loading Market" })).toBeInTheDocument();
    expect(screen.queryByText("content")).not.toBeInTheDocument();
  });

  test("an error shows the message and Retry retries", async () => {
    const retry = vi.fn();
    renderFrame(
      <PanelFrame state={{ kind: "error", message: "Market orders request failed.", retry }} title="Market">
        content
      </PanelFrame>,
    );

    expect(screen.getByText("Market orders request failed.")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledOnce();
    expect(screen.queryByText("content")).not.toBeInTheDocument();
  });

  test("empty shows the empty text instead of the content", () => {
    renderFrame(<PanelFrame state={{ kind: "empty", text: "No jobs running." }} title="Industry">content</PanelFrame>);

    expect(screen.getByText("No jobs running.")).toBeInTheDocument();
    expect(screen.queryByText("content")).not.toBeInTheDocument();
  });
});

describe("panelState", () => {
  test("maps a load to loading, error, empty or ready", () => {
    const retry = vi.fn();
    expect(panelState(loaded<number[]>({ loading: true }), () => false, "none")).toEqual({ kind: "loading" });
    expect(panelState(loaded<number[]>({ error: new Error("x"), errorMessage: "boom", retry }), () => false, "none")).toEqual({
      kind: "error",
      message: "boom",
      retry,
    });
    expect(panelState(loaded({ data: [] as number[] }), (data) => data.length === 0, "none")).toEqual({ kind: "empty", text: "none" });
    expect(panelState(loaded({ data: [1] }), (data) => data.length === 0, "none")).toEqual({ kind: "ready" });
  });

  test("a waiting load (nothing fetched yet, no data) counts as loading", () => {
    expect(panelState(loaded<number[]>({}), () => false, "none")).toEqual({ kind: "loading" });
  });
});

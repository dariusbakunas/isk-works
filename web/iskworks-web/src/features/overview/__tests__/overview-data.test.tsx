import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vitest";

import { OverviewDataProvider, useOverviewData } from "../overview-data";
import { fetchByUrl, sourceUrls } from "./overview-fixtures";

afterEach(() => {
  vi.unstubAllGlobals();
});

function Consumer({ name }: { name: string }) {
  const { sources } = useOverviewData();
  return <p>{name}: {sources.jobs.data ? `${sources.jobs.data.summary.running} running` : "…"}</p>;
}

function ReloadButton() {
  const { reloadAll } = useOverviewData();
  return <button onClick={reloadAll} type="button">Reload</button>;
}

describe("OverviewDataProvider", () => {
  test("fetches each source once, however many panels read it", async () => {
    const fetchMock = fetchByUrl();
    render(
      <OverviewDataProvider>
        <Consumer name="strip" />
        <Consumer name="panel" />
      </OverviewDataProvider>,
    );

    expect(await screen.findByText("strip: 3 running")).toBeInTheDocument();
    expect(screen.getByText("panel: 3 running")).toBeInTheDocument();
    for (const url of sourceUrls) {
      expect(fetchMock.mock.calls.filter(([input]) => String(input).includes(url))).toHaveLength(1);
    }
  });

  test("a workspace without characters fetches nothing else", async () => {
    const fetchMock = fetchByUrl({ characters: [] });
    render(
      <OverviewDataProvider>
        <Consumer name="strip" />
      </OverviewDataProvider>,
    );

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    expect(String(fetchMock.mock.calls[0]![0])).toContain("/api/characters");
    expect(screen.getByText("strip: …")).toBeInTheDocument();
  });

  test("reloadAll fetches every source again", async () => {
    const fetchMock = fetchByUrl();
    render(
      <OverviewDataProvider>
        <Consumer name="strip" />
        <ReloadButton />
      </OverviewDataProvider>,
    );
    await screen.findByText("strip: 3 running");

    await userEvent.setup().click(screen.getByRole("button", { name: "Reload" }));

    await waitFor(() => {
      for (const url of sourceUrls) {
        expect(fetchMock.mock.calls.filter(([input]) => String(input).includes(url))).toHaveLength(2);
      }
    });
  });
});

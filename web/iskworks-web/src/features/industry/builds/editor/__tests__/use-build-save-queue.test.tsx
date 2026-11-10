import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { Build } from "../../../../../api/industry";
import { useBuildSaveQueue, type BuildDraftInput, type BuildSaveQueueOptions } from "../use-build-save-queue";

const api = vi.hoisted(() => ({ createBuild: vi.fn(), updateBuild: vi.fn() }));

vi.mock("../../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../../api/industry")>("../../../../../api/industry");
  return { ...actual, ...api };
});

const draft = { name: "Rifter build" } as BuildDraftInput;

function savedBuild(revision: number): Build {
  return { id: "build-1", revision } as Build;
}

function options(overrides: Partial<BuildSaveQueueOptions> = {}): BuildSaveQueueOptions {
  return {
    initialBuild: savedBuild(1),
    complete: true,
    draftInput: () => draft,
    snapshot: "",
    onCreated: vi.fn(),
    setError: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe("useBuildSaveQueue", () => {
  test("a save requested mid-flight waits its turn and sends the revision the first one returned", async () => {
    let finishFirst!: (build: Build) => void;
    api.updateBuild
      .mockImplementationOnce(() => new Promise<Build>((resolve) => { finishFirst = resolve; }))
      .mockResolvedValueOnce(savedBuild(3));
    const { result } = renderHook(() => useBuildSaveQueue(options()));

    let first!: Promise<Build | null>;
    let second!: Promise<Build | null>;
    act(() => {
      first = result.current.queueSave();
      second = result.current.queueSave();
    });
    await act(async () => {});
    expect(api.updateBuild).toHaveBeenCalledTimes(1);
    expect(api.updateBuild.mock.calls[0][1]).toMatchObject({ expectedRevision: 1 });

    await act(async () => {
      finishFirst(savedBuild(2));
      await first;
      await second;
    });
    expect(api.updateBuild).toHaveBeenCalledTimes(2);
    expect(api.updateBuild.mock.calls[1][1]).toMatchObject({ expectedRevision: 2 });
  });

  test("an incomplete draft is not sent", async () => {
    const setError = vi.fn();
    const { result } = renderHook(() => useBuildSaveQueue(options({ complete: false, setError })));

    await act(async () => {
      expect(await result.current.queueSave()).toBeNull();
    });
    expect(api.updateBuild).not.toHaveBeenCalled();
    expect(setError).toHaveBeenCalledWith("Name, blueprint, and valid runs are required.");
  });

  test("the first save of a new Build creates it and reports the new id", async () => {
    api.createBuild.mockResolvedValue(savedBuild(1));
    const onCreated = vi.fn();
    const { result } = renderHook(() => useBuildSaveQueue(options({ initialBuild: null, onCreated })));

    await act(async () => {
      await result.current.queueSave();
    });
    expect(api.createBuild).toHaveBeenCalledWith(draft);
    expect(onCreated).toHaveBeenCalledWith(savedBuild(1));
  });

  test("queueSaveIfChanged skips a save when the snapshot is already saved", async () => {
    api.updateBuild.mockResolvedValue(savedBuild(2));
    const { result } = renderHook(() => useBuildSaveQueue(options({ snapshot: "draft-a" })));

    await act(async () => {
      expect(await result.current.queueSaveIfChanged()).toBe("build-1");
    });
    expect(api.updateBuild).not.toHaveBeenCalled();
  });
});

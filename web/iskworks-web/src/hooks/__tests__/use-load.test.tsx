import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { useLoad, type Loaded } from "../use-load";

type Load = ((signal: AbortSignal) => Promise<string>) | null;

function deferred() {
  let resolve!: (value: string) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<string>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}

/** Renders `useLoad` and records the state of every render. */
function renderLoad(initial: { load: Load; loadKey: string }) {
  const renders: Loaded<string>[] = [];
  const hook = renderHook(
    ({ load, loadKey }: { load: Load; loadKey: string }) => {
      const loaded = useLoad(load, loadKey);
      renders.push(loaded);
      return loaded;
    },
    { initialProps: initial },
  );
  return { ...hook, renders };
}

describe("useLoad", () => {
  test("loads, then keeps the data while the next key loads", async () => {
    const first = deferred();
    const second = deferred();
    const { result, rerender } = renderLoad({ load: () => first.promise, loadKey: "a" });
    expect(result.current.loading).toBe(true);

    await act(async () => first.resolve("A"));
    expect(result.current).toMatchObject({ data: "A", loading: false, error: null });

    rerender({ load: () => second.promise, loadKey: "b" });
    expect(result.current).toMatchObject({ data: "A", loading: true });

    await act(async () => second.resolve("B"));
    expect(result.current).toMatchObject({ data: "B", loading: false });
  });

  test("reports loading from the very render the key changes in", async () => {
    const { renders, rerender } = renderLoad({ load: () => Promise.resolve("A"), loadKey: "a" });
    await waitFor(() => expect(renders.at(-1)?.data).toBe("A"));

    const before = renders.length;
    rerender({ load: () => new Promise<string>(() => {}), loadKey: "b" });
    expect(renders[before]).toMatchObject({ loading: true });
  });

  test("an error belongs to its key: the next key and a retry start clean", async () => {
    const failing = vi.fn(() => Promise.reject(new Error("boom")));
    const { result, rerender } = renderLoad({ load: failing, loadKey: "a" });
    await waitFor(() => expect(result.current.error?.message).toBe("boom"));
    expect(result.current.loading).toBe(false);

    act(() => result.current.retry());
    expect(result.current).toMatchObject({ loading: true, error: null });
    await waitFor(() => expect(failing).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.error?.message).toBe("boom"));

    rerender({ load: () => new Promise<string>(() => {}), loadKey: "b" });
    expect(result.current).toMatchObject({ loading: true, error: null });
  });

  test("a null load waits, and runs once it arrives even under the same key", async () => {
    const load = vi.fn(() => Promise.resolve("A"));
    const { result, rerender } = renderLoad({ load: null, loadKey: "a" });
    expect(result.current).toMatchObject({ data: null, loading: false, error: null });
    expect(load).not.toHaveBeenCalled();

    rerender({ load, loadKey: "a" });
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.data).toBe("A"));
    expect(load).toHaveBeenCalledTimes(1);
  });

  test("a superseded request is aborted and its answer ignored", async () => {
    const first = deferred();
    let firstSignal: AbortSignal | undefined;
    const { result, rerender } = renderLoad({
      load: (signal) => {
        firstSignal = signal;
        return first.promise;
      },
      loadKey: "a",
    });

    rerender({ load: () => Promise.resolve("B"), loadKey: "b" });
    await waitFor(() => expect(result.current.data).toBe("B"));
    expect(firstSignal?.aborted).toBe(true);

    await act(async () => first.resolve("A"));
    expect(result.current.data).toBe("B");
  });
});

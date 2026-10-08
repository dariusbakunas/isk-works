import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { MONEY_INPUT_MESSAGES } from "../parse-money-input";
import { MoneyInput } from "../money-input";

function setup(props: Partial<React.ComponentProps<typeof MoneyInput>> = {}) {
  const onCommit = vi.fn();
  const onClear = vi.fn();
  render(
    <MoneyInput
      aria-label="Unit price"
      // Effectively disable the typing debounce by default so parallel-load
      // timer slippage can't let an intermediate valid prefix commit during
      // `userEvent.type`. Tests that exercise the debounce opt back in.
      commitDebounceMs={10_000}
      onClear={onClear}
      onCommit={onCommit}
      value=""
      {...props}
    />,
  );
  return { onCommit, onClear, input: screen.getByLabelText("Unit price") as HTMLInputElement };
}

describe("MoneyInput", () => {
  it("shows the committed value grouped for display", () => {
    const { input } = setup({ value: "1000000" });
    expect(input.value).toBe("1,000,000");
  });

  it("commits a canonical value after the debounce for valid input", async () => {
    const { onCommit, input } = setup({ commitDebounceMs: 20 });
    // Paste as one edit so a single classification -> single debounced
    // commit, regardless of per-keystroke timing under parallel load.
    input.focus();
    await userEvent.paste("1,000,000");
    await waitFor(() => expect(onCommit).toHaveBeenCalledWith("1000000"));
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("commits 1,234.56 as 1234.56", async () => {
    const { onCommit, input } = setup({ commitDebounceMs: 20 });
    await userEvent.type(input, "1,234.56");
    await waitFor(() => expect(onCommit).toHaveBeenCalledWith("1234.56"));
  });

  it("does not commit while the value is invalid and shows an adjacent error", async () => {
    const { onCommit, input } = setup();
    await userEvent.type(input, "1.234567");
    // give any debounce a chance to (wrongly) fire
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(onCommit).not.toHaveBeenCalled();
    const error = await screen.findByRole("alert");
    expect(error).toHaveTextContent(MONEY_INPUT_MESSAGES.decimals);
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input.getAttribute("aria-describedby")).toBe(error.id);
  });

  it("surfaces an unfinished comma group as an error only on blur", async () => {
    const { onCommit, input } = setup();
    await userEvent.type(input, "1,00");
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();

    input.blur();
    expect(await screen.findByRole("alert")).toHaveTextContent(MONEY_INPUT_MESSAGES.grouping);
  });

  it("does not commit incomplete typing states", async () => {
    const { onCommit, input } = setup();
    await userEvent.type(input, "1,000.");
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(onCommit).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("rejects five fractional digits locally", async () => {
    const { onCommit, input } = setup();
    await userEvent.type(input, "1,234.56789");
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(onCommit).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toHaveTextContent(MONEY_INPUT_MESSAGES.decimals);
  });

  it("rejects negatives locally", async () => {
    const { onCommit, input } = setup();
    await userEvent.type(input, "-1");
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(onCommit).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toHaveTextContent(MONEY_INPUT_MESSAGES.negative);
  });

  it("clears the error and commits once the input is corrected", async () => {
    const { onCommit, input } = setup({ commitDebounceMs: 20 });
    await userEvent.type(input, "1.234567");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await userEvent.clear(input);
    await userEvent.type(input, "1000");
    await waitFor(() => expect(onCommit).toHaveBeenCalledWith("1000"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("normalizes a trailing decimal to a whole number on blur", async () => {
    const { onCommit, input } = setup();
    await userEvent.type(input, "1,000.");
    input.blur();
    await waitFor(() => expect(onCommit).toHaveBeenCalledWith("1000"));
  });

  it("keeps invalid text and its error on blur without touching the committed value", async () => {
    const { onCommit, input } = setup({ value: "2460" });
    await userEvent.clear(input);
    await userEvent.type(input, "1,00");
    input.blur();
    expect(await screen.findByRole("alert")).toHaveTextContent(MONEY_INPUT_MESSAGES.grouping);
    expect(input.value).toBe("1,00");
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("restores the committed value when blurred on an unresolvable fragment", async () => {
    const { onCommit, input } = setup({ value: "2460" });
    await userEvent.clear(input);
    await userEvent.type(input, ".");
    input.blur();
    await waitFor(() => expect(input.value).toBe("2,460"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("calls onClear (not onCommit) when an existing value is emptied and blurred", async () => {
    const { onCommit, onClear, input } = setup({ value: "2460" });
    await userEvent.clear(input);
    input.blur();
    await waitFor(() => expect(onClear).toHaveBeenCalledTimes(1));
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("never emits an empty string through onCommit", async () => {
    const { onCommit, input } = setup({ value: "2460" });
    await userEvent.clear(input);
    await new Promise((resolve) => setTimeout(resolve, 60));
    for (const call of onCommit.mock.calls) {
      expect(call[0]).not.toBe("");
    }
  });
});

describe("MoneyInput -- explicit-submit form usage (onValueChange, no onCommit)", () => {
  function formSetup(initial = "") {
    const onValueChange = vi.fn();
    render(
      <MoneyInput aria-label="Unit price (ISK)" onValueChange={onValueChange} value={initial} />,
    );
    return { onValueChange, input: screen.getByLabelText("Unit price (ISK)") as HTMLInputElement };
  }

  it("reports the seeded value's classification on mount", () => {
    const { onValueChange } = formSetup("1234.5600");
    expect(onValueChange).toHaveBeenLastCalledWith({ status: "valid", canonical: "1234.56" });
  });

  it("streams the live canonical value on every keystroke without a debounce", async () => {
    const { onValueChange, input } = formSetup();
    await userEvent.type(input, "1,000,000");
    expect(onValueChange).toHaveBeenLastCalledWith({ status: "valid", canonical: "1000000" });
  });

  it("reports invalid / incomplete without committing anything", async () => {
    const { onValueChange, input } = formSetup();
    await userEvent.type(input, "1,00");
    expect(onValueChange).toHaveBeenLastCalledWith({ status: "incomplete" });
    await userEvent.type(input, ".5");
    // "1,00.5" -> unrepairable grouping
    expect(onValueChange.mock.calls.at(-1)?.[0].status).toBe("invalid");
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("reports the finalized classification on blur", async () => {
    const { onValueChange, input } = formSetup();
    await userEvent.type(input, "1,000.");
    input.blur();
    await waitFor(() =>
      expect(onValueChange).toHaveBeenLastCalledWith({ status: "valid", canonical: "1000" }),
    );
  });

  it("gates a parent Save control purely on the reported status", async () => {
    function Harness() {
      const [result, setResult] = useState<{ status: string; canonical?: string }>({ status: "empty" });
      const [saved, setSaved] = useState<string | null>(null);
      return (
        <>
          <MoneyInput aria-label="Price" onValueChange={setResult} value="" />
          <button disabled={result.status !== "valid"} onClick={() => setSaved(result.canonical ?? null)} type="button">
            Save
          </button>
          <output>{saved ?? "unsaved"}</output>
        </>
      );
    }
    render(<Harness />);
    const input = screen.getByLabelText("Price");
    const save = screen.getByRole("button", { name: "Save" });

    expect(save).toBeDisabled();
    await userEvent.type(input, "1,23");
    expect(save).toBeDisabled();
    await userEvent.type(input, "4.56");
    await waitFor(() => expect(save).toBeEnabled());
    await userEvent.click(save);
    expect(screen.getByRole("status")).toHaveTextContent("1234.56");
  });
});

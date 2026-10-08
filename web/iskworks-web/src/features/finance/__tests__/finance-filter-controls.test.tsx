import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import {
  FinanceCheckbox,
  FinanceDateField,
  FinanceFilterSection,
} from "../finance-filter-controls";

describe("Finance filter controls", () => {
  test("exposes and toggles an expanded filter section", () => {
    const onToggle = vi.fn();
    render(
      <FinanceFilterSection id="characters" onToggle={onToggle} open title="Characters">
        <span>Aura Valex</span>
      </FinanceFilterSection>,
    );

    const button = screen.getByRole("button", { name: /characters/i });
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Aura Valex")).toBeInTheDocument();

    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledOnce();
  });

  test("does not render collapsed section content", () => {
    render(
      <FinanceFilterSection id="more" onToggle={vi.fn()} open={false} title="More filters">
        <span>Additional controls</span>
      </FinanceFilterSection>,
    );

    expect(screen.getByRole("button", { name: /more filters/i })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Additional controls")).not.toBeInTheDocument();
  });

  test("retains native checkbox behavior", () => {
    const onChange = vi.fn();
    render(<FinanceCheckbox checked={false} onChange={onChange}>Aura Valex</FinanceCheckbox>);

    fireEvent.click(screen.getByLabelText("Aura Valex"));
    expect(onChange).toHaveBeenCalledOnce();
  });

  test("renders an accessible date input with a decorative calendar icon", () => {
    const onChange = vi.fn();
    render(<FinanceDateField id="from-date" label="From" onChange={onChange} value="2026-08-01" />);

    const input = screen.getByLabelText("From");
    expect(input).toHaveAttribute("type", "date");
    expect(input).toHaveValue("2026-08-01");
    expect(screen.getByTestId("finance-date-calendar")).toHaveAttribute("aria-hidden", "true");

    fireEvent.change(input, { target: { value: "2026-08-02" } });
    expect(onChange).toHaveBeenCalledWith("2026-08-02");
  });
});

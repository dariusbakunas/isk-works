import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import { CalendarFilters, type CalendarCharacterOption } from "../calendar-filters";
import type { CalendarTypeFilter } from "../calendar-url-state";

const CHARACTERS: CalendarCharacterOption[] = [
  { connectionId: "connection-alix", eveCharacterId: 90000001, characterName: "Alix Morgan" },
  { connectionId: "connection-bryn", eveCharacterId: 90000002, characterName: "Bryn Vale" },
];

function Harness({ initial = [] as string[] }) {
  const [selected, setSelected] = useState(initial);
  const [type, setType] = useState<CalendarTypeFilter>("all");
  const [timezone, setTimezone] = useState<"eve" | "local">("eve");
  return <CalendarFilters characters={CHARACTERS} onCharactersChange={setSelected} onTimezoneChange={setTimezone} onTypeChange={setType} selectedCharacterIds={selected} timezone={timezone} type={type} />;
}

test("exposes compact type and timezone segments as pressed controls", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  expect(screen.getByRole("button", { name: "All milestone types" })).toHaveAttribute("aria-pressed", "true");
  await user.click(screen.getByRole("button", { name: "Skill milestones" }));
  expect(screen.getByRole("button", { name: "Skill milestones" })).toHaveAttribute("aria-pressed", "true");
  await user.click(screen.getByRole("button", { name: "Local time" }));
  expect(screen.getByRole("button", { name: "Local time" })).toHaveAttribute("aria-pressed", "true");
});

test("summarizes All, one, and many characters and exposes checked state", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  const trigger = screen.getByRole("button", { name: "Filter by character: All characters" });
  await user.click(trigger);
  expect(screen.getByRole("menuitemcheckbox", { name: "Alix Morgan" })).toHaveAttribute("aria-checked", "true");
  await user.click(screen.getByRole("menuitemcheckbox", { name: "Alix Morgan" }));
  expect(screen.getByRole("button", { name: "Filter by character: Alix Morgan" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Filter by character: Alix Morgan" }));
  await user.click(screen.getByRole("menuitemcheckbox", { name: "Bryn Vale" }));
  expect(screen.getByRole("button", { name: "Filter by character: 2 characters" })).toBeInTheDocument();
});

test("supports arrow navigation, Escape, outside click, and focus restoration", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  const trigger = screen.getByRole("button", { name: /Filter by character/ });
  await user.click(trigger);
  expect(screen.getByRole("menuitemcheckbox", { name: "Alix Morgan" })).toHaveFocus();
  await user.keyboard("{ArrowDown}");
  expect(screen.getByRole("menuitemcheckbox", { name: "Bryn Vale" })).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(trigger).toHaveFocus();
  await user.click(trigger);
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

test("reports controlled filter changes", async () => {
  const onCharactersChange = vi.fn();
  render(<CalendarFilters characters={CHARACTERS} onCharactersChange={onCharactersChange} onTimezoneChange={vi.fn()} onTypeChange={vi.fn()} selectedCharacterIds={[]} timezone="eve" type="all" />);
  fireEvent.click(screen.getByRole("button", { name: /Filter by character/ }));
  fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Alix Morgan" }));
  expect(onCharactersChange).toHaveBeenCalledWith(["connection-alix"]);
});

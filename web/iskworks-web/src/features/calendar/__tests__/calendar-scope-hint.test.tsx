import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { CalendarScopeHint } from "../calendar-scope-hint";

const character = (id: number, name: string) => ({
  connectionId: `00000000-0000-4000-8000-00000000000${id}`,
  eveCharacterId: 90000000 + id,
  characterName: name,
  calendarScopeMissing: true,
});
const several = [character(1, "Psycer"), character(2, "Totally Not Lit"), character(3, "Scolopendra")];

beforeEach(() => window.localStorage.clear());

test("several characters are counted, named on request, and the button says what EVE will ask", async () => {
  const onGrant = vi.fn();
  render(<CalendarScopeHint characters={several} onGrant={onGrant} />);
  const hint = screen.getByRole("note", { name: "Calendar access" });
  expect(hint).toHaveTextContent("Calendar events are missing for 3 characters.");
  expect(hint).toHaveTextContent("EVE's login page asks which character to authorize, so grant access once per character.");
  expect(hint).not.toHaveTextContent("Psycer");

  await userEvent.click(within(hint).getByRole("button", { name: "Which ones?" }));
  const list = within(hint).getByRole("list", { name: "Characters without calendar access" });
  expect(within(list).getAllByRole("listitem").map((item) => item.textContent)).toEqual(["Psycer", "Totally Not Lit", "Scolopendra"]);

  await userEvent.click(within(hint).getByRole("button", { name: "Grant calendar access" }));
  expect(onGrant).toHaveBeenCalledOnce();
});

test("a single character is named, with who to log in as", () => {
  render(<CalendarScopeHint characters={[character(1, "Psycer")]} onGrant={vi.fn()} />);
  const hint = screen.getByRole("note", { name: "Calendar access" });
  expect(hint).toHaveTextContent("Calendar events are missing for Psycer. Grant calendar access and log in as Psycer on EVE's page.");
  expect(within(hint).queryByRole("button", { name: "Which ones?" })).not.toBeInTheDocument();
});

test("dismissing hides it for the same characters but a new one brings it back", async () => {
  const { unmount } = render(<CalendarScopeHint characters={several} onGrant={vi.fn()} />);
  await userEvent.click(screen.getByRole("button", { name: "Dismiss calendar access note" }));
  expect(screen.queryByRole("note")).not.toBeInTheDocument();
  unmount();

  const { unmount: unmountSame } = render(<CalendarScopeHint characters={[...several].reverse()} onGrant={vi.fn()} />);
  expect(screen.queryByRole("note")).not.toBeInTheDocument();
  unmountSame();

  render(<CalendarScopeHint characters={[...several, character(4, "New Alt")]} onGrant={vi.fn()} />);
  expect(screen.getByRole("note", { name: "Calendar access" })).toHaveTextContent("4 characters");
});

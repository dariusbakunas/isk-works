import { expect, test, type Page, type Route } from "@playwright/test";
import { CALENDAR_MILESTONE_FIXTURES } from "../src/features/calendar/calendar-fixtures";

const workspace = {
  configured: true,
  workspace: { id: "workspace-1", name: "Industry", ownerId: "owner-1", createdAt: "2026-09-27T10:00:00Z", updatedAt: "2026-09-27T10:00:00Z" },
  owner: { id: "owner-1", workspaceId: "workspace-1", kind: "manual", displayName: "Industry", hidden: true },
  version: "calendar-e2e",
};

const characters = [
  { connectionId: "0aa719e8-f94f-4ef1-8f47-cf649d3b2d95", eveCharacterId: 90000001, characterName: "Alix Morgan", trainingQueue: [] },
  { connectionId: "874c445a-67f4-4368-9028-b785770a2e52", eveCharacterId: 90000002, characterName: "Bryn Vale", trainingQueue: [] },
];

async function installCalendarApi(page: Page, milestones = CALENDAR_MILESTONE_FIXTURES, calendarRequests: string[] = []) {
  await page.route("**/api/**", async (route: Route) => {
    const path = new URL(route.request().url()).pathname;
    if (!path.startsWith("/api/")) {
      await route.continue();
      return;
    }
    let body: unknown = [];
    if (path === "/api/auth/session") body = { authenticated: true, characterName: "Aeva Stark", workspaceId: "workspace-1", inviteRequired: false };
    else if (path === "/api/workspace") body = workspace;
    else if (path === "/api/characters") body = characters;
    else if (path === "/api/calendar") {
      calendarRequests.push(route.request().url());
      body = milestones;
    }
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });
  });
}

test("desktop Month exposes dense milestones, filters, and all inspector paths", async ({ page }, testInfo) => {
  await installCalendarApi(page);
  await page.goto("/calendar?view=month&date=2026-10-02&type=all&tz=eve");
  await expect(page.getByRole("heading", { name: "Calendar", exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "Calendar" })).toHaveAttribute("aria-current", "page");
  await page.getByRole("button", { name: /UTC midnight-crossing/ }).click();
  await expect(page.getByRole("heading", { name: "UTC midnight-crossing Muninn batch" })).toBeVisible();
  if ((page.viewportSize()?.width ?? 0) >= 1024) {
    await expect(page.getByRole("button", { name: "Dismiss calendar inspector" })).toBeHidden();
  } else {
    await expect(page.getByRole("button", { name: "Dismiss calendar inspector" })).toBeVisible();
  }
  await page.screenshot({ path: testInfo.outputPath("calendar-industry-inspector.png"), fullPage: true });
  await page.getByRole("button", { name: "Close calendar inspector" }).click();

  await page.getByRole("button", { name: "Show 2 more milestones for Friday, October 2, 2026" }).click();
  await expect(page.getByText("3 Industry · 2 Skill")).toBeVisible();
  await page.getByRole("button", { name: /Open Drones V/ }).click();
  await expect(page.getByText("Current queue projection")).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("calendar-skill-inspector.png"), fullPage: true });
  await page.getByRole("button", { name: "Back to Friday, October 2, 2026" }).click();
  await expect(page.getByText("3 Industry · 2 Skill")).toBeVisible();
  await page.getByRole("button", { name: "Close calendar inspector" }).click();

  await page.getByRole("button", { name: /Filter by character/ }).click();
  await expect(page.getByRole("menuitemcheckbox", { name: "Alix Morgan" })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("calendar-character-menu.png"), fullPage: true });
});

test("September view distinguishes past milestones and Today", async ({ page }, testInfo) => {
  await installCalendarApi(page);
  await page.goto("/calendar?view=month&date=2026-09-27&type=all&tz=eve");
  await expect(page.getByRole("button", { name: /Past Hurricane batch/ })).toHaveAttribute("data-past", "true");
  await expect(page.locator('.iw-calendar-day[data-today="true"]')).toHaveCount(1);
  await page.screenshot({ path: testInfo.outputPath("calendar-past-today.png"), fullPage: true });
});

test("desktop renders the source-empty state", async ({ page }) => {
  await installCalendarApi(page, []);
  await page.goto("/calendar?view=month&date=2026-10-02&type=all&tz=eve");
  await expect(page.getByText("No calendar milestones yet")).toBeVisible();
});

test("desktop renders filtered-empty without claiming source data is absent", async ({ page }) => {
  await installCalendarApi(page, CALENDAR_MILESTONE_FIXTURES.filter((milestone) => milestone.kind === "industry"));
  await page.goto("/calendar?view=month&date=2026-10-02&type=skill&tz=eve");
  await expect(page.getByText("No milestones match these filters")).toBeVisible();
  await expect(page.getByText("No calendar milestones yet")).not.toBeVisible();
});

test("375px Month uses compact cells and a real selected-day agenda", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await installCalendarApi(page);
  await page.goto("/calendar?view=month&date=2026-10-02&type=all&tz=eve");
  const day = page.getByRole("gridcell", { name: /Friday, October 2, 2026/ });
  const box = await day.boundingBox();
  expect(box?.width).toBeGreaterThanOrEqual(36);
  expect(box?.width).toBeLessThanOrEqual(40);
  await page.getByRole("button", { name: /Filter by character/ }).click();
  const mobileMenu = page.getByRole("menu");
  await expect(mobileMenu).toBeVisible();
  const menuBox = await mobileMenu.boundingBox();
  expect(menuBox?.x).toBeGreaterThanOrEqual(0);
  expect((menuBox?.x ?? 0) + (menuBox?.width ?? 0)).toBeLessThanOrEqual(375);
  await page.keyboard.press("Escape");
  await day.getByRole("button", { name: /Show milestones/ }).click();
  const agenda = page.getByRole("region", { name: "Selected day agenda" });
  await expect(agenda.getByText("Scimitar")).toBeVisible();
  await agenda.getByRole("button", { name: "Open Scimitar" }).click();
  await expect(page.getByRole("heading", { name: "Scimitar" })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("calendar-mobile-agenda-inspector.png"), fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test("desktop Week renders seven chronological columns and restores inspector focus", async ({ page }, testInfo) => {
  await installCalendarApi(page);
  await page.goto("/calendar?view=week&date=2026-10-02&type=all&tz=eve");
  await expect(page.getByRole("button", { name: "Week view" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".iw-calendar-week-day")).toHaveCount(7);
  await expect(page.locator(".iw-calendar-week-grid")).toHaveCSS("overflow-y", "auto");
  await expect(page.locator(".iw-calendar-week-day-header").first()).toHaveCSS("position", "sticky");
  const columns = await page.locator(".iw-calendar-week-day").evaluateAll((days) => days.map((day) => day.getBoundingClientRect().x));
  expect(columns).toEqual([...columns].sort((left, right) => left - right));
  const friday = page.getByRole("region", { name: "Friday, October 2, 2026" });
  const cards = friday.locator(".iw-calendar-week-card");
  await expect(cards).toHaveCount(5);
  const times = await cards.locator("time").allTextContents();
  expect(times).toEqual(["12:00 PM UTC", "1:15 PM UTC", "2:30 PM UTC", "5:45 PM UTC", "7:00 PM UTC"]);
  const scimitar = friday.getByRole("button", { name: /Scimitar, Manufacturing/ });
  await scimitar.click();
  await expect(page.getByRole("heading", { name: "Scimitar" })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("calendar-week-desktop-inspector.png"), fullPage: true });
  await page.getByRole("button", { name: "Close calendar inspector" }).click();
  await expect(scimitar).toBeFocused();
});

test("dense desktop Week scrolls milestones while its day headers stay pinned", async ({ page }) => {
  const seed = CALENDAR_MILESTONE_FIXTURES.find((milestone) => milestone.title === "Scimitar")!;
  const denseWeek = Array.from({ length: 18 }, (_, index) => ({
    ...seed,
    id: `${seed.id}:dense:${index}`,
    occursAt: `2026-10-02T${String(index + 1).padStart(2, "0")}:00:00Z`,
    title: `Dense milestone ${index + 1}`,
  }));
  await installCalendarApi(page, denseWeek);
  await page.goto("/calendar?view=week&date=2026-10-02&type=all&tz=eve");
  const grid = page.locator(".iw-calendar-week-grid");
  const header = page.locator(".iw-calendar-week-day-header").first();
  await expect.poll(() => grid.evaluate((node) => node.scrollHeight > node.clientHeight)).toBe(true);
  const before = await header.evaluate((node) => node.getBoundingClientRect().y);
  await grid.evaluate((node) => { node.scrollTop = node.scrollHeight; });
  await expect.poll(() => grid.evaluate((node) => node.scrollTop)).toBeGreaterThan(0);
  const after = await header.evaluate((node) => node.getBoundingClientRect().y);
  expect(after).toBeCloseTo(before, 0);
});

test("Week and Month share the selected date and active-unit navigation", async ({ page }) => {
  await installCalendarApi(page);
  await page.goto("/calendar?view=month&date=2026-10-02&type=all&tz=eve");
  await page.getByRole("button", { name: "Show milestones for Thursday, October 15, 2026" }).click();
  await expect(page).toHaveURL(/view=month&date=2026-10-15/);
  await page.getByRole("button", { name: "Close calendar inspector" }).click();
  await page.getByRole("button", { name: "Week view" }).click();
  await expect(page).toHaveURL(/view=week&date=2026-10-15/);
  await expect(page.getByRole("region", { name: "Monday, October 12, 2026" })).toBeVisible();
  await page.getByRole("button", { name: "Next week" }).click();
  await expect(page).toHaveURL(/date=2026-10-22/);
  await page.getByRole("button", { name: "Previous week" }).click();
  await expect(page).toHaveURL(/date=2026-10-15/);
});

test("Week distinguishes source-empty and filtered-empty states", async ({ page }) => {
  await installCalendarApi(page, []);
  await page.goto("/calendar?view=week&date=2026-10-02&type=all&tz=eve");
  await expect(page.getByText("No calendar milestones yet")).toBeVisible();

  await page.unroute("**/api/**");
  await installCalendarApi(page, CALENDAR_MILESTONE_FIXTURES.filter((milestone) => milestone.kind === "industry"));
  await page.goto("/calendar?view=week&date=2026-10-02&type=skill&tz=eve");
  await expect(page.getByText("No milestones match these filters")).toBeVisible();
  await expect(page.getByText("No calendar milestones yet")).not.toBeVisible();
});

test("375px Week is one vertical seven-day agenda without horizontal overflow", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await installCalendarApi(page);
  await page.goto("/calendar?view=week&date=2026-10-02&type=all&tz=eve");
  const days = page.locator(".iw-calendar-week-day");
  await expect(days).toHaveCount(7);
  await expect(page.getByText("Monday, September 28, 2026")).toBeVisible();
  const boxes = await days.evaluateAll((nodes) => nodes.map((node) => {
    const box = node.getBoundingClientRect();
    return { x: box.x, y: box.y, width: box.width };
  }));
  expect(new Set(boxes.map(({ x }) => Math.round(x))).size).toBe(1);
  expect(boxes.every(({ width }) => width <= 375)).toBe(true);
  expect(boxes.map(({ y }) => y).every((y, index, rows) => index === 0 || y > rows[index - 1])).toBe(true);
  const longCard = page.getByRole("button", { name: /UTC midnight-crossing Muninn batch/ });
  await longCard.click();
  await expect(page.getByRole("button", { name: "Dismiss calendar inspector" })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("calendar-week-375-inspector.png"), fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test("Year restores canonically, requests one full EVE year, and uses responsive month geometry", async ({ page }, testInfo) => {
  const requests: string[] = [];
  await installCalendarApi(page, CALENDAR_MILESTONE_FIXTURES, requests);
  await page.goto("/calendar?view=year&date=2026-10-02&type=all&tz=eve");
  await expect(page.getByRole("button", { name: "Year view" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("group", { name: "Calendar year 2026" })).toBeVisible();
  await expect(page.locator(".iw-calendar-year-month")).toHaveCount(12);
  await expect.poll(() => requests.length).toBe(1);
  const request = new URL(requests[0]);
  expect(request.searchParams.get("from")).toBe("2026-01-01T00:00:00.000Z");
  expect(request.searchParams.get("to")).toBe("2027-01-01T00:00:00.000Z");

  const cards = await page.locator(".iw-calendar-year-month").evaluateAll((nodes) => nodes.map((node) => {
    const box = node.getBoundingClientRect();
    return { x: Math.round(box.x), y: Math.round(box.y) };
  }));
  const columns = new Set(cards.map(({ x }) => x)).size;
  const rows = new Set(cards.map(({ y }) => y)).size;
  if ((page.viewportSize()?.width ?? 0) >= 1000) expect([columns, rows]).toEqual([4, 3]);
  else expect([columns, rows]).toEqual([3, 4]);
  await expect(page.getByRole("button", { name: /^October 2026, 5 Industry, 3 Skill/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "Close calendar inspector" })).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath(`calendar-year-${page.viewportSize()?.width}.png`), fullPage: true });
});

test("Year navigation, direct view switches, and month selection preserve canonical dates", async ({ page }) => {
  await installCalendarApi(page);
  await page.goto("/calendar?view=year&date=2028-02-29&type=all&tz=eve");
  await page.getByRole("button", { name: "Previous year" }).click();
  await expect(page).toHaveURL(/view=year&date=2027-02-28/);
  await page.getByRole("button", { name: "Next year" }).click();
  await expect(page).toHaveURL(/view=year&date=2028-02-28/);
  await page.getByRole("button", { name: "Today" }).click();
  await expect(page).toHaveURL(/view=year&date=2026-09-28/);
  await page.getByRole("button", { name: "Week view" }).click();
  await expect(page).toHaveURL(/view=week&date=2026-09-28/);
  await page.getByRole("button", { name: "Year view" }).click();
  await expect(page).toHaveURL(/view=year&date=2026-09-28/);

  await page.goto("/calendar?view=year&date=2027-01-31&type=all&tz=eve");
  await page.getByRole("button", { name: /^February 2027/ }).click();
  await expect(page).toHaveURL(/view=month&date=2027-02-28/);
  await page.goto("/calendar?view=year&date=2028-01-31&type=all&tz=eve");
  await page.getByRole("button", { name: /^February 2028/ }).click();
  await expect(page).toHaveURL(/view=month&date=2028-02-29/);
});

test("Year filters update totals locally and distinguish empty states", async ({ page }) => {
  const requests: string[] = [];
  await installCalendarApi(page, CALENDAR_MILESTONE_FIXTURES, requests);
  await page.goto("/calendar?view=year&date=2026-10-02&type=all&tz=eve");
  await expect(page.getByRole("button", { name: /^October 2026, 5 Industry, 3 Skill/ })).toBeVisible();
  await page.getByRole("button", { name: "Skill milestones" }).click();
  await expect(page.getByRole("button", { name: /^October 2026, 0 Industry, 3 Skill/ })).toBeVisible();
  await page.getByRole("button", { name: /Filter by character/ }).click();
  await page.getByRole("menuitemcheckbox", { name: "Bryn Vale" }).click();
  await expect(page.getByRole("button", { name: /^October 2026, 0 Industry, 2 Skill/ })).toBeVisible();
  await page.getByRole("button", { name: "All milestone types" }).click();
  await page.getByRole("button", { name: "Local time" }).click();
  await expect(page).toHaveURL(/type=all.*tz=local/);
  expect(requests).toHaveLength(2);

  await page.unroute("**/api/**");
  await installCalendarApi(page, []);
  await page.goto("/calendar?view=year&date=2026-10-02&type=all&tz=eve");
  await expect(page.getByText("No calendar milestones yet")).toBeVisible();
  await page.unroute("**/api/**");
  await installCalendarApi(page, CALENDAR_MILESTONE_FIXTURES.filter((milestone) => milestone.kind === "industry"));
  await page.goto("/calendar?view=year&date=2026-10-02&type=skill&tz=eve");
  await expect(page.getByText("No milestones match these filters")).toBeVisible();
});

test("375px Year is a single-column mini-month agenda without horizontal overflow", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await installCalendarApi(page);
  await page.goto("/calendar?view=year&date=2026-10-02&type=all&tz=eve");
  const cards = page.locator(".iw-calendar-year-month");
  await expect(cards).toHaveCount(12);
  const boxes = await cards.evaluateAll((nodes) => nodes.map((node) => {
    const box = node.getBoundingClientRect();
    return { x: Math.round(box.x), y: Math.round(box.y), width: box.width };
  }));
  expect(new Set(boxes.map(({ x }) => x)).size).toBe(1);
  expect(boxes.map(({ y }) => y).every((y, index, rows) => index === 0 || y > rows[index - 1])).toBe(true);
  expect(boxes.every(({ width }) => width <= 375)).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath("calendar-year-375.png"), fullPage: true });
});

test("dense Year stays below the payload gate and settles with twelve tab stops", async ({ page }) => {
  const industry = CALENDAR_MILESTONE_FIXTURES.find((milestone) => milestone.kind === "industry")!;
  const skill = CALENDAR_MILESTONE_FIXTURES.find((milestone) => milestone.kind === "skill")!;
  const denseYear = Array.from({ length: 1200 }, (_, index) => {
    const seed = index % 2 === 0 ? industry : skill;
    const month = (index % 12) + 1;
    const day = (index % 28) + 1;
    return {
      ...seed,
      id: `${seed.id}:dense:${index}`,
      occursAt: `2026-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}T12:00:00Z`,
      title: `Dense milestone ${index + 1}`,
    };
  });
  expect(new TextEncoder().encode(JSON.stringify(denseYear)).byteLength).toBeLessThan(1_048_576);
  const requests: string[] = [];
  await installCalendarApi(page, denseYear, requests);
  await page.goto("/calendar?view=year&date=2026-06-15&type=all&tz=eve");
  await expect(page.locator(".iw-calendar-year-month")).toHaveCount(12, { timeout: 5_000 });
  expect(requests).toHaveLength(1);
  await expect(page.locator(".iw-calendar-year button")).toHaveCount(12);
});

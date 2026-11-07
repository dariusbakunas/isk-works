import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { ApiError } from "../../../api/workspace";
import { MaintenanceNoticePage } from "../maintenance-notice-page";

const adminApi = vi.hoisted(() => ({
  setMaintenanceNotice: vi.fn(),
  clearMaintenanceNotice: vi.fn(),
}));
const healthApi = vi.hoisted(() => ({ getMaintenanceNotice: vi.fn() }));

vi.mock("../../../api/admin", () => adminApi);
vi.mock("../../../api/health", () => healthApi);

const notice = { message: "Back soon.", startsAt: "2030-01-02T03:04:00.000Z" };

describe("MaintenanceNoticePage", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    healthApi.getMaintenanceNotice.mockResolvedValue(null);
  });

  test("defaults to a start in about 10 minutes and saves it as an absolute time", async () => {
    adminApi.setMaintenanceNotice.mockImplementation(async (value) => value);
    const before = Date.now();
    render(<MaintenanceNoticePage />);

    await userEvent.click(await screen.findByRole("button", { name: "Show notice" }));

    await screen.findByText(/Showing now/);
    const [sent] = adminApi.setMaintenanceNotice.mock.calls[0];
    expect(sent.message).toBe("Please save your work before then.");
    const lead = new Date(sent.startsAt).getTime() - before;
    expect(lead).toBeGreaterThan(9 * 60_000);
    expect(lead).toBeLessThanOrEqual(10 * 60_000);
  });

  test("shows the current notice and clears it", async () => {
    healthApi.getMaintenanceNotice.mockResolvedValue(notice);
    adminApi.clearMaintenanceNotice.mockResolvedValue(undefined);
    render(<MaintenanceNoticePage />);

    expect(await screen.findByText("Back soon.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update notice" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Clear notice" }));

    await waitFor(() => expect(screen.queryByText("Back soon.")).not.toBeInTheDocument());
    expect(adminApi.clearMaintenanceNotice).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Show notice" })).toBeInTheDocument();
  });

  test("shows the API's message when saving fails", async () => {
    adminApi.setMaintenanceNotice.mockRejectedValue(
      new ApiError(400, { code: "validation_failed", message: "Start time must be in the future." }),
    );
    render(<MaintenanceNoticePage />);

    await userEvent.click(await screen.findByRole("button", { name: "Show notice" }));

    expect(await screen.findByText("Notice not saved")).toBeInTheDocument();
    expect(screen.getByText("Start time must be in the future.")).toBeInTheDocument();
  });
});

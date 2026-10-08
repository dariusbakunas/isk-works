import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { AdminInvite, AdminUser } from "../../../api/admin";
import { ApiError } from "../../../api/workspace";
import { AdminPage } from "../admin-page";

const adminApi = vi.hoisted(() => ({
  listUsers: vi.fn(),
  disableUser: vi.fn(),
  enableUser: vi.fn(),
  deleteUser: vi.fn(),
  countOrphanedWorkspaces: vi.fn(),
  eraseOrphanedWorkspaces: vi.fn(),
  listInvites: vi.fn(),
  createInvite: vi.fn(),
  disableInvite: vi.fn(),
  deleteInvite: vi.fn(),
  revealInviteCode: vi.fn(),
}));

vi.mock("../../../api/admin", () => adminApi);

function invite(overrides: Partial<AdminInvite> = {}): AdminInvite {
  return {
    id: "invite-1",
    status: "active",
    createdAt: "2026-09-30T10:00:00Z",
    expiresAt: null,
    disabledAt: null,
    maxUses: 3,
    useCount: 1,
    note: "for Bob",
    revealable: true,
    ...overrides,
  };
}

function adminUser(overrides: Partial<AdminUser> = {}): AdminUser {
  return {
    id: "user-1",
    characterId: 91000001,
    characterName: "Busy Pilot",
    createdAt: "2026-09-01T10:00:00Z",
    lastLoginAt: new Date().toISOString(),
    characterCount: 3,
    charactersNeedingAttention: 0,
    activeSessions: 2,
    disabledAt: null,
    isAdmin: false,
    isCurrentUser: false,
    ...overrides,
  };
}

function renderAdmin(isAdmin: boolean, path = "/admin/invites") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/admin/*" element={<AdminPage isAdmin={isAdmin} />} />
        <Route path="/" element={<p>home</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  Object.values(adminApi).forEach((mock) => mock.mockReset());
  adminApi.listInvites.mockResolvedValue([invite()]);
  adminApi.listUsers.mockResolvedValue([adminUser()]);
  adminApi.countOrphanedWorkspaces.mockResolvedValue(0);
});

describe("AdminPage", () => {
  test("redirects non-admins away without calling the API", () => {
    renderAdmin(false);
    expect(screen.getByText("home")).toBeInTheDocument();
    expect(adminApi.listInvites).not.toHaveBeenCalled();
  });

  test("admin lands on the invites list", async () => {
    renderAdmin(true);
    expect(await screen.findByText("for Bob")).toBeInTheDocument();
    expect(screen.getByText("1/3")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
  });

  test("creating an invite shows the code once and refreshes the list", async () => {
    adminApi.createInvite.mockResolvedValue({
      code: "ISK-AAAA-BBBB-CCCC-DDDD",
      invite: invite({ id: "invite-2", note: "new" }),
    });
    const user = userEvent.setup();
    renderAdmin(true);
    await screen.findByText("for Bob");

    await user.type(screen.getByLabelText("Note"), "new");
    await user.click(screen.getByRole("button", { name: "Create invite" }));

    expect(await screen.findByText("ISK-AAAA-BBBB-CCCC-DDDD")).toBeInTheDocument();
    expect(adminApi.createInvite).toHaveBeenCalledWith({
      maxUses: 1,
      expiresAt: undefined,
      note: "new",
    });
    expect(adminApi.listInvites).toHaveBeenCalledTimes(2);

    await user.click(screen.getByRole("button", { name: "Done" }));
    expect(screen.queryByText("ISK-AAAA-BBBB-CCCC-DDDD")).not.toBeInTheDocument();
  });

  test("shows a server validation message when creation fails", async () => {
    adminApi.createInvite.mockRejectedValue(
      new ApiError(400, { code: "validation_failed", message: "Validation failed." }),
    );
    const user = userEvent.setup();
    renderAdmin(true);
    await screen.findByText("for Bob");
    await user.click(screen.getByRole("button", { name: "Create invite" }));
    expect(await screen.findByText("Invite not created")).toBeInTheDocument();
  });

  test("disabling asks for confirmation, then reloads", async () => {
    adminApi.disableInvite.mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderAdmin(true);
    await screen.findByText("for Bob");

    await user.click(screen.getByRole("button", { name: /Disable invite for Bob/ }));
    const dialog = screen.getByRole("dialog");
    expect(adminApi.disableInvite).not.toHaveBeenCalled();
    adminApi.listInvites.mockResolvedValue([invite({ status: "disabled" })]);
    await user.click(within(dialog).getByRole("button", { name: "Disable invite" }));

    await waitFor(() => expect(adminApi.disableInvite).toHaveBeenCalledWith("invite-1"));
    expect(await screen.findByText("Disabled")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Disable invite for Bob/ })).not.toBeInTheDocument();
  });

  test("Reveal fetches and shows the code for a revealable invite", async () => {
    adminApi.revealInviteCode.mockResolvedValue({ code: "ISK-1111-2222-3333-4444" });
    const user = userEvent.setup();
    renderAdmin(true);
    await screen.findByText("for Bob");

    await user.click(screen.getByRole("button", { name: /Reveal invite for Bob/ }));

    expect(await screen.findByText("ISK-1111-2222-3333-4444")).toBeInTheDocument();
    expect(adminApi.revealInviteCode).toHaveBeenCalledWith("invite-1");
    await user.click(screen.getByRole("button", { name: "Done" }));
    expect(screen.queryByText("ISK-1111-2222-3333-4444")).not.toBeInTheDocument();
  });

  test("invites without a stored copy have no Reveal button", async () => {
    adminApi.listInvites.mockResolvedValue([invite({ revealable: false })]);
    renderAdmin(true);
    await screen.findByText("for Bob");
    expect(screen.queryByRole("button", { name: /Reveal invite/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Disable invite/ })).toBeInTheDocument();
  });

  test("Delete asks for confirmation, then removes the invite and reloads", async () => {
    adminApi.deleteInvite.mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderAdmin(true);
    await screen.findByText("for Bob");

    await user.click(screen.getByRole("button", { name: /Delete invite for Bob/ }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Delete this invite?")).toBeInTheDocument();
    expect(adminApi.deleteInvite).not.toHaveBeenCalled();
    adminApi.listInvites.mockResolvedValue([]);
    await user.click(within(dialog).getByRole("button", { name: "Delete invite" }));

    await waitFor(() => expect(adminApi.deleteInvite).toHaveBeenCalledWith("invite-1"));
    expect(adminApi.disableInvite).not.toHaveBeenCalled();
    expect(await screen.findByText("No invites yet")).toBeInTheDocument();
  });

  test("cancelling the Delete dialog leaves the invite alone", async () => {
    const user = userEvent.setup();
    renderAdmin(true);
    await screen.findByText("for Bob");
    await user.click(screen.getByRole("button", { name: /Delete invite for Bob/ }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(adminApi.deleteInvite).not.toHaveBeenCalled();
  });

  test("does not offer Disable for invites that are not active", async () => {
    adminApi.listInvites.mockResolvedValue([invite({ status: "exhausted", useCount: 3 })]);
    renderAdmin(true);
    expect(await screen.findByText("Used up")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Disable invite/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Delete invite/ })).toBeInTheDocument();
  });

  test("/admin lands on the users list with summary counts", async () => {
    adminApi.listUsers.mockResolvedValue([
      adminUser(),
      adminUser({
        id: "user-2",
        characterName: "Stale Pilot",
        characterCount: 1,
        charactersNeedingAttention: 1,
        lastLoginAt: "2026-01-01T00:00:00Z",
      }),
    ]);
    renderAdmin(true, "/admin");

    expect(await screen.findByText("Busy Pilot")).toBeInTheDocument();
    expect(screen.getByText("Stale Pilot")).toBeInTheDocument();
    expect(screen.getByText("1 need attention")).toBeInTheDocument();
    const stat = (label: string) => screen.getByText(label, { selector: "dt" }).nextElementSibling;
    expect(stat("Users")).toHaveTextContent("2");
    expect(stat("Active in 7 days")).toHaveTextContent("1");
    expect(stat("Linked characters")).toHaveTextContent("4");
  });

  test("shows an error when users cannot load", async () => {
    adminApi.listUsers.mockRejectedValue(
      new ApiError(403, { code: "forbidden", message: "You do not have access to this area." }),
    );
    renderAdmin(true, "/admin/users");
    expect(await screen.findByText("Users unavailable")).toBeInTheDocument();
  });

  describe("user actions", () => {
    test("Disable asks first, then disables and reloads", async () => {
      adminApi.disableUser.mockResolvedValue(undefined);
      const user = userEvent.setup();
      renderAdmin(true, "/admin/users");
      await screen.findByText("Busy Pilot");

      await user.click(screen.getByRole("button", { name: "Disable Busy Pilot" }));
      const dialog = screen.getByRole("dialog");
      expect(adminApi.disableUser).not.toHaveBeenCalled();
      adminApi.listUsers.mockResolvedValue([adminUser({ disabledAt: "2026-10-01T00:00:00Z" })]);
      await user.click(within(dialog).getByRole("button", { name: "Disable user" }));

      await waitFor(() => expect(adminApi.disableUser).toHaveBeenCalledWith("user-1"));
      expect(await screen.findByText("Disabled")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Enable Busy Pilot" })).toBeInTheDocument();
    });

    test("Enable acts immediately and reloads", async () => {
      adminApi.enableUser.mockResolvedValue(undefined);
      adminApi.listUsers.mockResolvedValue([adminUser({ disabledAt: "2026-10-01T00:00:00Z" })]);
      const user = userEvent.setup();
      renderAdmin(true, "/admin/users");
      await screen.findByText("Busy Pilot");

      adminApi.listUsers.mockResolvedValue([adminUser()]);
      await user.click(screen.getByRole("button", { name: "Enable Busy Pilot" }));

      await waitFor(() => expect(adminApi.enableUser).toHaveBeenCalledWith("user-1"));
      expect(await screen.findByRole("button", { name: "Disable Busy Pilot" })).toBeInTheDocument();
    });

    test("Delete stays disabled until the character name is typed", async () => {
      adminApi.deleteUser.mockResolvedValue(undefined);
      const user = userEvent.setup();
      renderAdmin(true, "/admin/users");
      await screen.findByText("Busy Pilot");

      await user.click(screen.getByRole("button", { name: "Delete Busy Pilot" }));
      const dialog = screen.getByRole("dialog");
      const confirm = within(dialog).getByRole("button", { name: "Delete user" });
      expect(confirm).toBeDisabled();

      await user.type(within(dialog).getByRole("textbox"), "Busy Pil");
      expect(confirm).toBeDisabled();
      await user.type(within(dialog).getByRole("textbox"), "ot");
      expect(confirm).toBeEnabled();

      adminApi.listUsers.mockResolvedValue([]);
      await user.click(confirm);

      await waitFor(() =>
        expect(adminApi.deleteUser).toHaveBeenCalledWith("user-1", "Busy Pilot"),
      );
      expect(await screen.findByText("No users yet")).toBeInTheDocument();
    });

    test("cancelling Delete does nothing", async () => {
      const user = userEvent.setup();
      renderAdmin(true, "/admin/users");
      await screen.findByText("Busy Pilot");
      await user.click(screen.getByRole("button", { name: "Delete Busy Pilot" }));
      await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(adminApi.deleteUser).not.toHaveBeenCalled();
    });

    test("you and other admins get no actions", async () => {
      adminApi.listUsers.mockResolvedValue([
        adminUser({ id: "me", characterName: "Me Admin", isAdmin: true, isCurrentUser: true }),
        adminUser({ id: "other", characterName: "Other Admin", isAdmin: true }),
        adminUser({ id: "plain", characterName: "Plain Pilot" }),
      ]);
      renderAdmin(true, "/admin/users");
      await screen.findByText("Plain Pilot");

      const table = within(screen.getByLabelText("Users"));
      expect(table.getByText("You")).toBeInTheDocument();
      expect(table.getByText("Admin")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Me Admin/ })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Other Admin/ })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Disable Plain Pilot" })).toBeInTheDocument();
    });

    test("a rejected action is reported", async () => {
      adminApi.disableUser.mockRejectedValue(
        new ApiError(409, { code: "conflict", message: "You cannot disable or delete your own account." }),
      );
      const user = userEvent.setup();
      renderAdmin(true, "/admin/users");
      await screen.findByText("Busy Pilot");
      await user.click(screen.getByRole("button", { name: "Disable Busy Pilot" }));
      await user.click(
        within(screen.getByRole("dialog")).getByRole("button", { name: "Disable user" }),
      );
      expect(await screen.findByText("User action failed")).toBeInTheDocument();
    });
  });

  describe("leftover workspaces", () => {
    test("no banner when nothing is left over", async () => {
      renderAdmin(true, "/admin/users");
      await screen.findByText("Busy Pilot");
      expect(screen.queryByText(/leftover/)).not.toBeInTheDocument();
    });

    test("erasing needs the word ERASE, then clears the banner", async () => {
      adminApi.countOrphanedWorkspaces.mockResolvedValue(2);
      adminApi.eraseOrphanedWorkspaces.mockResolvedValue(2);
      const user = userEvent.setup();
      renderAdmin(true, "/admin/users");
      expect(await screen.findByText(/leftover workspaces/)).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Erase leftover data" }));
      const dialog = screen.getByRole("dialog");
      const confirm = within(dialog).getByRole("button", { name: "Erase leftover data" });
      expect(confirm).toBeDisabled();

      adminApi.countOrphanedWorkspaces.mockResolvedValue(0);
      await user.type(within(dialog).getByRole("textbox"), "erase");
      await user.click(confirm);

      await waitFor(() => expect(adminApi.eraseOrphanedWorkspaces).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(screen.queryByText(/leftover workspaces/)).not.toBeInTheDocument());
    });

    test("a failing count does not hide the user list", async () => {
      adminApi.countOrphanedWorkspaces.mockRejectedValue(new Error("boom"));
      renderAdmin(true, "/admin/users");
      expect(await screen.findByText("Busy Pilot")).toBeInTheDocument();
    });
  });
});

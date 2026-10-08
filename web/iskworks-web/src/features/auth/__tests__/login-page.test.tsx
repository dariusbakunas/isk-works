import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { ApiError } from "../../../api/workspace";
import { LoginPage } from "../login-page";

const authApi = vi.hoisted(() => ({ beginLogin: vi.fn() }));

vi.mock("../../../api/auth", async () => {
  const actual = await vi.importActual<typeof import("../../../api/auth")>("../../../api/auth");
  return { ...actual, ...authApi };
});

const assignMock = vi.fn();
const realLocation = window.location;

function setSearch(search: string) {
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: { ...realLocation, search, assign: assignMock },
  });
}

beforeEach(() => {
  assignMock.mockReset();
  authApi.beginLogin.mockReset();
  authApi.beginLogin.mockResolvedValue({ authorizationUrl: "https://login.eveonline.com/authorize?x=1" });
  setSearch("");
});

afterEach(() => {
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: realLocation,
  });
});

describe("LoginPage", () => {
  test("plain sign-in is always available", async () => {
    render(<LoginPage inviteRequired={false} />, { wrapper: MemoryRouter });
    const button = screen.getByRole("button", { name: /continue with eve online/i });
    await userEvent.click(button);
    await waitFor(() => expect(authApi.beginLogin).toHaveBeenCalledWith(undefined));
    expect(assignMock).toHaveBeenCalledWith("https://login.eveonline.com/authorize?x=1");
  });

  test("no invite affordance when invite mode is off", () => {
    render(<LoginPage inviteRequired={false} />, { wrapper: MemoryRouter });
    expect(screen.queryByText(/have an invite/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/invite code/i)).not.toBeInTheDocument();
  });

  test("invite mode reveals the invite field on request, and it is DOM-private", async () => {
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    // Returning-user path still front and centre.
    expect(screen.getByRole("button", { name: /continue with eve online/i })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /have an invite/i }));
    const field = screen.getByLabelText(/invite code/i);
    expect(field).toHaveAttribute("data-private");
  });

  test("blank invite keeps the join button disabled", async () => {
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    await userEvent.click(screen.getByRole("button", { name: /have an invite/i }));
    expect(screen.getByRole("button", { name: /join alpha with invite/i })).toBeDisabled();

    await userEvent.type(screen.getByLabelText(/invite code/i), "ISK-AAAA-BBBB-CCCC-DDDD");
    expect(screen.getByRole("button", { name: /join alpha with invite/i })).toBeEnabled();
  });

  test("join flow sends the invite code and redirects", async () => {
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    await userEvent.click(screen.getByRole("button", { name: /have an invite/i }));
    await userEvent.type(screen.getByLabelText(/invite code/i), "ISK-AAAA-BBBB-CCCC-DDDD");
    await userEvent.click(screen.getByRole("button", { name: /join alpha with invite/i }));

    await waitFor(() =>
      expect(authApi.beginLogin).toHaveBeenCalledWith("ISK-AAAA-BBBB-CCCC-DDDD"),
    );
    expect(assignMock).toHaveBeenCalled();
  });

  test("invalid invite shows an error and keeps the field open", async () => {
    authApi.beginLogin.mockRejectedValueOnce(
      new ApiError(400, { code: "invite_invalid", message: "nope" }),
    );
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    await userEvent.click(screen.getByRole("button", { name: /have an invite/i }));
    await userEvent.type(screen.getByLabelText(/invite code/i), "ISK-BAD");
    await userEvent.click(screen.getByRole("button", { name: /join alpha with invite/i }));

    expect(await screen.findByText(/isn.t valid/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/invite code/i)).toBeInTheDocument();
    expect(assignMock).not.toHaveBeenCalled();
  });

  test("a network failure reads differently from an invalid invite", async () => {
    authApi.beginLogin.mockRejectedValueOnce(new Error("offline"));
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    await userEvent.click(screen.getByRole("button", { name: /continue with eve online/i }));

    expect(await screen.findByText(/could not start eve sso sign-in/i)).toBeInTheDocument();
  });

  test("?status=invite_required opens the field with an explanation", () => {
    setSearch("?status=invite_required");
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    expect(screen.getByLabelText(/invite code/i)).toBeInTheDocument();
    expect(screen.getByText(/invite-only right now/i)).toBeInTheDocument();
  });

  test("?status=account_disabled explains the block without an invite prompt", () => {
    setSearch("?status=account_disabled");
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    expect(screen.getByText(/account has been disabled/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/invite code/i)).not.toBeInTheDocument();
  });

  test("?status=character_transferred explains the refusal without an invite prompt", () => {
    setSearch("?status=character_transferred");
    render(<LoginPage inviteRequired />, { wrapper: MemoryRouter });
    expect(screen.getByText(/transferred to another EVE account/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/invite code/i)).not.toBeInTheDocument();
  });

  test("?status=denied surfaces a cancelled-sign-in notice without an invite prompt", () => {
    setSearch("?status=denied");
    render(<LoginPage inviteRequired={false} />, { wrapper: MemoryRouter });
    expect(screen.getByText(/sign-in was cancelled/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/invite code/i)).not.toBeInTheDocument();
  });
});

describe("community and legal links", () => {
  afterEach(() => {
    delete window.__ISKWORKS_CONFIG__;
  });

  test("shows the configured community link, the donation note, and the legal page link", () => {
    window.__ISKWORKS_CONFIG__ = {
      supportUrl: "https://discord.gg/Abc123",
      supportLabel: "Discord",
      donationCharacter: "Some Pilot",
    };
    render(<LoginPage inviteRequired={false} />, { wrapper: MemoryRouter });

    expect(screen.getByRole("link", { name: "Discord" })).toHaveAttribute("href", "https://discord.gg/Abc123");
    expect(screen.getByText("Some Pilot")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /about, privacy & legal/i })).toHaveAttribute("href", "/legal");
    expect(screen.getByText(/not affiliated with or endorsed by CCP hf/i)).toBeInTheDocument();
  });
});

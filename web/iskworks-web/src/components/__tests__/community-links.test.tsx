import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test } from "vitest";

import { HelpMarkdown } from "../../features/help/markdown";
import { LegalPage } from "../../features/legal/legal-page";
import { AppFooter } from "../app-footer";
import { UPSTREAM_SOURCE_URL, resolveCommunityLinks } from "../community-links";

const CONFIGURED = {
  supportUrl: "https://discord.gg/Abc123",
  supportLabel: "Discord",
  donationCharacter: "Some Pilot",
  sourceUrl: "https://example.com/fork",
};

afterEach(() => {
  delete window.__ISKWORKS_CONFIG__;
});

describe("resolveCommunityLinks", () => {
  test("hides the support link and donation note when nothing is configured", () => {
    expect(resolveCommunityLinks({}, {})).toEqual({
      support: null,
      donationCharacter: null,
      sourceUrl: UPSTREAM_SOURCE_URL,
    });
  });

  test("uses the runtime config when present, ignoring build variables", () => {
    expect(resolveCommunityLinks({ supportUrl: " https://chat.example.com " }, CONFIGURED)).toEqual({
      support: { url: "https://chat.example.com", label: "Community" },
      donationCharacter: null,
      sourceUrl: UPSTREAM_SOURCE_URL,
    });
  });

  test("falls back to build variables without a runtime config", () => {
    expect(resolveCommunityLinks(undefined, CONFIGURED)).toEqual({
      support: { url: "https://discord.gg/Abc123", label: "Discord" },
      donationCharacter: "Some Pilot",
      sourceUrl: "https://example.com/fork",
    });
  });

  test("ignores non-web URLs", () => {
    const links = resolveCommunityLinks({ supportUrl: "javascript:alert(1)", sourceUrl: "data:x" }, {});
    expect(links.support).toBeNull();
    expect(links.sourceUrl).toBe(UPSTREAM_SOURCE_URL);
  });
});

describe("AppFooter", () => {
  test("shows the configured support link, donation note and source link", () => {
    window.__ISKWORKS_CONFIG__ = CONFIGURED;
    render(<AppFooter />, { wrapper: MemoryRouter });

    expect(screen.getByRole("link", { name: "Discord" })).toHaveAttribute("href", "https://discord.gg/Abc123");
    expect(screen.getByText("Some Pilot")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Source code" })).toHaveAttribute("href", "https://example.com/fork");
  });

  test("omits the support link and donation note when unconfigured", () => {
    window.__ISKWORKS_CONFIG__ = {};
    render(<AppFooter />, { wrapper: MemoryRouter });

    expect(screen.queryByText(/donations/i)).not.toBeInTheDocument();
    expect(screen.getAllByRole("link").map((link) => link.textContent)).toEqual([
      "Help",
      "About, privacy & legal",
      "Source code",
    ]);
    expect(screen.getByRole("link", { name: "Source code" })).toHaveAttribute("href", UPSTREAM_SOURCE_URL);
  });
});

describe("LegalPage", () => {
  test("points support and deletion requests at the operator when no channel is configured", () => {
    window.__ISKWORKS_CONFIG__ = {};
    render(<LegalPage />, { wrapper: MemoryRouter });

    expect(screen.getByText(/data requests go to the operator of this instance/i)).toBeInTheDocument();
    expect(screen.getByText(/contact the operator of this instance and we will erase it/i)).toBeInTheDocument();
    expect(screen.getByText("ISK Works is free.")).toBeInTheDocument();
  });

  test("links the configured channel", () => {
    window.__ISKWORKS_CONFIG__ = CONFIGURED;
    render(<LegalPage />, { wrapper: MemoryRouter });

    const supportLinks = screen.getAllByRole("link", { name: "Discord" });
    expect(supportLinks.length).toBeGreaterThanOrEqual(2);
    supportLinks.forEach((link) => expect(link).toHaveAttribute("href", "https://discord.gg/Abc123"));
    expect(screen.getAllByText("Some Pilot").length).toBeGreaterThanOrEqual(1);
  });
});

describe("HelpMarkdown community placeholders", () => {
  const source = "Ask in the [community channel](support:). Read the [source code](source:).";

  test("resolves placeholders to the configured links", () => {
    window.__ISKWORKS_CONFIG__ = CONFIGURED;
    render(<HelpMarkdown source={source} />, { wrapper: MemoryRouter });

    expect(screen.getByRole("link", { name: "community channel" })).toHaveAttribute("href", "https://discord.gg/Abc123");
    expect(screen.getByRole("link", { name: "source code" })).toHaveAttribute("href", "https://example.com/fork");
  });

  test("renders the support placeholder as plain text when unconfigured", () => {
    window.__ISKWORKS_CONFIG__ = {};
    render(<HelpMarkdown source={source} />, { wrapper: MemoryRouter });

    expect(screen.queryByRole("link", { name: "community channel" })).not.toBeInTheDocument();
    expect(screen.getByText(/community channel/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "source code" })).toHaveAttribute("href", UPSTREAM_SOURCE_URL);
  });
});

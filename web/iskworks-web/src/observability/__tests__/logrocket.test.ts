import { describe, expect, it } from "vitest";

import {
  identifyViewer,
  initLogRocket,
  isLogRocketEnabled,
  reportBackendError,
  resolveReplaySettings,
  shouldTagApiRequest,
  trackEvent,
} from "../logrocket";

describe("shouldTagApiRequest", () => {
  it("tags same-origin /api/* paths", () => {
    expect(shouldTagApiRequest("/api/workspace")).toBe(true);
    expect(shouldTagApiRequest("/api/auth/session")).toBe(true);
    expect(shouldTagApiRequest(`${window.location.origin}/api/assets`)).toBe(true);
  });

  it("ignores same-origin paths outside /api/", () => {
    expect(shouldTagApiRequest("/assets/export")).toBe(false);
    expect(shouldTagApiRequest("/")).toBe(false);
    expect(shouldTagApiRequest("/apiary")).toBe(false);
  });

  it("ignores cross-origin requests", () => {
    expect(shouldTagApiRequest("https://cdn.logr-ingest.com/i")).toBe(false);
    expect(shouldTagApiRequest("https://images.evetech.net/types/34/icon")).toBe(false);
  });

  it("ignores unparseable input", () => {
    expect(shouldTagApiRequest("http://[")).toBe(false);
  });
});

describe("enablement + safety", () => {
  it("is disabled under the test build (no app id, no enable flag)", () => {
    expect(isLogRocketEnabled()).toBe(false);
  });

  it("initLogRocket / identify / track / reportBackendError are safe no-ops when disabled", () => {
    expect(() => {
      initLogRocket();
      identifyViewer({ workspaceId: "ws-1" });
      trackEvent("noop", { a: 1 });
      reportBackendError("corr-1", 503);
    }).not.toThrow();
  });
});

describe("resolveReplaySettings", () => {
  const buildOn = { appId: "build/app", enabled: "true" };

  it("uses the container's runtime config when present, ignoring build variables", () => {
    expect(resolveReplaySettings({ logRocketAppId: "", logRocketEnabled: false }, buildOn)).toEqual({
      appId: "",
      enabled: false,
    });
    expect(resolveReplaySettings({ logRocketAppId: "org/app", logRocketEnabled: true }, {})).toEqual({
      appId: "org/app",
      enabled: true,
    });
  });

  it("needs both an app ID and the enable flag", () => {
    expect(resolveReplaySettings({ logRocketAppId: "org/app", logRocketEnabled: false }, {}).enabled).toBe(false);
    expect(resolveReplaySettings({ logRocketAppId: "  ", logRocketEnabled: true }, {}).enabled).toBe(false);
    expect(resolveReplaySettings(undefined, { appId: "build/app" }).enabled).toBe(false);
  });

  it("falls back to build variables only when no runtime config was loaded (local vite dev)", () => {
    expect(resolveReplaySettings(undefined, buildOn)).toEqual({ appId: "build/app", enabled: true });
    expect(resolveReplaySettings(undefined, {})).toEqual({ appId: "", enabled: false });
  });
});

import { describe, expect, it } from "vitest";

import {
  contextualUserLabelKeys,
  isAuthRoute,
  isBulkExportRoute,
  isSensitiveResponseRoute,
  redactJsonBody,
  REDACTED,
  sanitizePageUrl,
  sanitizeRequest,
  sanitizeResponse,
  sanitizeUrl,
  type IRequest,
  type IResponse,
} from "../logrocket-sanitizers";

// Obvious secret markers — no sanitized output may contain any of these.
const SECRETS = [
  "SECRET_TOKEN",
  "SECRET_SESSION",
  "SECRET_REFRESH",
  "SECRET_ACCESS",
  "SECRET_OAUTH_STATE",
  "SECRET_OAUTH_CODE",
  "SECRET_INVITE",
  "PRIVATE USER NOTE",
  "123456789",
];

function assertNoSecrets(serialized: string) {
  for (const secret of SECRETS) {
    expect(serialized).not.toContain(secret);
  }
}

function req(overrides: Partial<IRequest>): IRequest {
  return {
    reqId: "1",
    url: "https://app.example.com/api/thing",
    method: "POST",
    headers: {},
    ...overrides,
  };
}

function res(overrides: Partial<IResponse>): IResponse {
  return {
    reqId: "1",
    method: "GET",
    status: 200,
    headers: {},
    url: "https://app.example.com/api/thing",
    ...overrides,
  };
}

describe("sanitizeUrl", () => {
  it("redacts OAuth and invite query parameters, keeps the rest", () => {
    const out = sanitizeUrl(
      "/api/eve/oauth/callback?state=SECRET_OAUTH_STATE&code=SECRET_OAUTH_CODE&keep=yes",
    );
    expect(out).toContain("keep=yes");
    expect(out).toContain(`state=${encodeURIComponent(REDACTED)}`);
    assertNoSecrets(out);
  });

  it("redacts invite-code-shaped params (forward protection)", () => {
    const out = sanitizeUrl("/onboarding?invite=SECRET_INVITE&inviteCode=SECRET_INVITE");
    assertNoSecrets(out);
  });

  it("leaves a clean URL untouched", () => {
    expect(sanitizeUrl("/api/builds/42?runs=10")).toBe("/api/builds/42?runs=10");
  });

  it("sanitizePageUrl behaves the same for absolute URLs", () => {
    const out = sanitizePageUrl("https://app.example.com/x?code=SECRET_OAUTH_CODE");
    expect(out.startsWith("https://app.example.com/")).toBe(true);
    assertNoSecrets(out);
  });
});

describe("route classification", () => {
  it("identifies auth / OAuth routes", () => {
    expect(isAuthRoute("/api/auth/session")).toBe(true);
    expect(isAuthRoute("/api/auth/eve/login")).toBe(true);
    expect(isAuthRoute("/api/eve/oauth/callback")).toBe(true);
    expect(isAuthRoute("/api/eve/connections/authorize")).toBe(true);
    expect(isAuthRoute("/api/eve/connections/abc-123/refresh")).toBe(true);
    expect(isAuthRoute("/api/admin/invites")).toBe(true);
    expect(isAuthRoute("/api/builds")).toBe(false);
  });

  it("identifies sensitive-response routes", () => {
    expect(isSensitiveResponseRoute("/api/finance/transactions")).toBe(true);
    expect(isSensitiveResponseRoute("/api/eve/connections/x/wallet-transactions")).toBe(true);
    expect(isSensitiveResponseRoute("/api/assets/summary")).toBe(true);
    expect(isSensitiveResponseRoute("/api/assets")).toBe(false);
    expect(isSensitiveResponseRoute("/api/builds/1")).toBe(false);
  });
});

describe("redactJsonBody", () => {
  it("redacts credential / token / oauth / invite / note keys at any depth", () => {
    const body = JSON.stringify({
      accessToken: "SECRET_ACCESS",
      refreshToken: "SECRET_REFRESH",
      nested: { token: "SECRET_TOKEN", state: "SECRET_OAUTH_STATE", code: "SECRET_OAUTH_CODE" },
      inviteCode: "SECRET_INVITE",
      notes: "PRIVATE USER NOTE",
      description: "PRIVATE USER NOTE",
      list: [{ password: "SECRET_TOKEN" }],
      runs: 18431,
      typeName: "Tritanium",
    });
    const out = redactJsonBody(body, "request");
    assertNoSecrets(out ?? "");
    const parsed = JSON.parse(out ?? "{}");
    // Non-sensitive domain data survives.
    expect(parsed.runs).toBe(18431);
    expect(parsed.typeName).toBe("Tritanium");
    expect(parsed.nested.token).toBe(REDACTED);
    expect(parsed.list[0].password).toBe(REDACTED);
  });

  it("redacts user-authored name/title only in request bodies", () => {
    const payload = JSON.stringify({ name: "My Secret Plan", title: "My Secret Plan" });
    const asRequest = JSON.parse(redactJsonBody(payload, "request") ?? "{}");
    expect(asRequest.name).toBe(REDACTED);
    expect(asRequest.title).toBe(REDACTED);

    const asResponse = JSON.parse(redactJsonBody(payload, "response") ?? "{}");
    // Response `name` is often a canonical type name — kept for debugging.
    expect(asResponse.name).toBe("My Secret Plan");
  });

  it("redacts financial aggregate keys only in response bodies", () => {
    const payload = JSON.stringify({ walletBalance: "123456789", income: "123456789" });
    const asResponse = JSON.parse(redactJsonBody(payload, "response") ?? "{}");
    expect(asResponse.walletBalance).toBe(REDACTED);
    expect(asResponse.income).toBe(REDACTED);
  });

  it("passes non-JSON bodies through untouched", () => {
    expect(redactJsonBody("plain text", "request")).toBe("plain text");
    expect(redactJsonBody(undefined, "response")).toBeUndefined();
  });
});

describe("sanitizeRequest", () => {
  it("strips auth-bearing headers but keeps the correlation tag and content-type", () => {
    const out = sanitizeRequest(
      req({
        headers: {
          Authorization: "Bearer SECRET_TOKEN",
          Cookie: "session=SECRET_SESSION",
          "X-Auth-Token": "SECRET_TOKEN",
          "Content-Type": "application/json",
          "X-LogRocket-URL": "https://app.logrocket.com/o/a/s/1",
        },
        body: JSON.stringify({ notes: "PRIVATE USER NOTE" }),
      }),
    );
    assertNoSecrets(JSON.stringify(out));
    expect(out.headers.Authorization).toBe(REDACTED);
    expect(out.headers.Cookie).toBe(REDACTED);
    expect(out.headers["X-Auth-Token"]).toBe(REDACTED);
    expect(out.headers["Content-Type"]).toBe("application/json");
    expect(out.headers["X-LogRocket-URL"]).toBe("https://app.logrocket.com/o/a/s/1");
  });

  it("drops the body entirely for auth routes but keeps method + route", () => {
    const out = sanitizeRequest(
      req({
        url: "https://app.example.com/api/auth/eve/login",
        // The invite-only sign-in flow POSTs { inviteCode } here.
        body: JSON.stringify({ codeVerifier: "SECRET_TOKEN", inviteCode: "SECRET_INVITE" }),
      }),
    );
    expect(out.body).toBeUndefined();
    expect(out.method).toBe("POST");
    expect(out.url).toContain("/api/auth/eve/login");
    assertNoSecrets(JSON.stringify(out));
  });

  it("redacts an inviteCode key even on a non-auth route (defence in depth)", () => {
    const out = sanitizeRequest(
      req({
        url: "https://app.example.com/api/thing",
        body: JSON.stringify({ inviteCode: "SECRET_INVITE", runs: 12 }),
      }),
    );
    const parsed = JSON.parse(out.body ?? "{}");
    expect(parsed.inviteCode).toBe(REDACTED);
    expect(parsed.runs).toBe(12);
  });

  it("redacts sensitive query params on the request URL", () => {
    const out = sanitizeRequest(
      req({ url: "https://app.example.com/api/eve/oauth/callback?code=SECRET_OAUTH_CODE&state=SECRET_OAUTH_STATE" }),
    );
    assertNoSecrets(out.url);
  });
});

describe("sanitizeResponse", () => {
  it("drops the response body for finance routes, keeps status", () => {
    const out = sanitizeResponse(
      res({
        url: "https://app.example.com/api/finance/transactions?page=1",
        body: JSON.stringify({ summary: { walletBalance: "123456789" }, rows: [{ totalPrice: "123456789" }] }),
        status: 200,
      }),
    );
    expect(out.body).toBeUndefined();
    expect(out.status).toBe(200);
  });

  it("drops the response body for the login response (authorization URL / oauth state)", () => {
    const out = sanitizeResponse(
      res({
        url: "https://app.example.com/api/auth/eve/login",
        method: "POST",
        body: JSON.stringify({ authorizationUrl: "https://login.eveonline.com/v2/oauth/authorize?state=SECRET_OAUTH_STATE" }),
      }),
    );
    expect(out.body).toBeUndefined();
  });

  it("key-redacts an ordinary response body while keeping domain fields", () => {
    const out = sanitizeResponse(
      res({
        url: "https://app.example.com/api/builds/42",
        body: JSON.stringify({
          name: "Rifter",
          notes: "PRIVATE USER NOTE",
          runs: 18431,
          required: 372282,
          making: 368620,
        }),
      }),
    );
    assertNoSecrets(out.body ?? "");
    const parsed = JSON.parse(out.body ?? "{}");
    expect(parsed.runs).toBe(18431);
    expect(parsed.required).toBe(372282);
    expect(parsed.making).toBe(368620);
    expect(parsed.notes).toBe(REDACTED);
  });

  it("strips Set-Cookie and token headers from responses", () => {
    const out = sanitizeResponse(
      res({
        headers: { "Set-Cookie": "session=SECRET_SESSION", "X-Session-Token": "SECRET_TOKEN" },
      }),
    );
    assertNoSecrets(JSON.stringify(out.headers));
  });

  it("redacts account-linked character names from an ordinary response", () => {
    const out = sanitizeResponse(
      res({
        url: "https://app.example.com/api/characters",
        body: JSON.stringify([{ characterName: "Jita Local", corporationName: "Test Corp", totalSp: 5_000_000 }]),
      }),
    );
    const parsed = JSON.parse(out.body ?? "[]");
    expect(parsed[0].characterName).toBe(REDACTED);
    // Progression structure and non-identity fields stay visible.
    expect(parsed[0].corporationName).toBe("Test Corp");
    expect(parsed[0].totalSp).toBe(5_000_000);
  });
});

describe("contextualUserLabelKeys", () => {
  it("targets a generic ticket's capturedName, not a structured ticket's", () => {
    expect([...contextualUserLabelKeys({ kind: "generic", capturedName: "x" })]).toEqual(["capturedName"]);
    expect([...contextualUserLabelKeys({ kind: "manufacturing", capturedName: "Tungsten Carbide" })]).toEqual([]);
    expect([...contextualUserLabelKeys({ kind: "build", capturedName: "Tungsten Carbide" })]).toEqual([]);
  });

  it("targets Build / facility / price-source / acquisition-run labels by shape", () => {
    expect(contextualUserLabelKeys({ name: "x", workspaceId: "w", runs: 1, recipe: {} }).has("name")).toBe(true);
    expect(
      contextualUserLabelKeys({ name: "x", materialReductionPercent: "1", rigs: [] }).has("name"),
    ).toBe(true);
    expect(
      contextualUserLabelKeys({ name: "x", description: "y", itemCount: 3, items: [] }).has("name"),
    ).toBe(true);
    expect(
      contextualUserLabelKeys({ name: "x", displayId: "ISK-9", ownerId: "o", status: "open" }).has("name"),
    ).toBe(true);
  });

  it("does NOT target structured domain objects that merely have a name", () => {
    // EVE market category node
    expect(contextualUserLabelKeys({ marketGroupId: 1, name: "Ammunition", itemCount: 5, children: [] }).size).toBe(0);
    // rig target filter
    expect(contextualUserLabelKeys({ filterId: 1, name: "Capital Components", categoryIds: [], groupIds: [] }).size).toBe(0);
    // SDE search hit
    expect(contextualUserLabelKeys({ typeId: 34, typeName: "Tritanium", groupName: "Mineral" }).size).toBe(0);
    // a facility rig row (has materialReductionPercent but no name/rigs)
    expect(contextualUserLabelKeys({ slotNumber: 1, typeId: 1, materialReductionPercent: "1" }).size).toBe(0);
  });
});

describe("non-JSON bodies", () => {
  it("drops a multipart form request body (market-export upload)", () => {
    const out = sanitizeRequest(
      req({
        url: "https://app.example.com/api/industry/market-imports",
        headers: { "Content-Type": "multipart/form-data; boundary=----x" },
        body: "------x\r\nContent-Disposition: form-data; name=\"files\"; filename=\"export.txt\"\r\n\r\nPRIVATE USER NOTE\r\n------x--",
      }),
    );
    expect(out.body).toBeUndefined();
    expect(out.method).toBe("POST");
    expect(out.url).toContain("/api/industry/market-imports");
  });

  it("drops a url-encoded form request body by content-type", () => {
    const out = sanitizeRequest(
      req({
        url: "https://app.example.com/api/whatever",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: "notes=PRIVATE%20USER%20NOTE&token=SECRET_TOKEN",
      }),
    );
    expect(out.body).toBeUndefined();
  });

  it("drops a CSV export response body, keeping status", () => {
    expect(isBulkExportRoute("/api/assets/export")).toBe(true);
    expect(isBulkExportRoute("/api/inventory/export")).toBe(true);
    const out = sanitizeResponse(
      res({
        url: "https://app.example.com/api/assets/export?columns=item,qty",
        headers: { "content-type": "text/csv" },
        body: "item,quantity,character\nRifter,123456789,SecretPilot\n",
      }),
    );
    expect(out.body).toBeUndefined();
    expect(out.status).toBe(200);
  });
});

describe("round-trip regression: private labels absent from request AND response", () => {
  const TITLE = "TITLE-CANARY-123";
  const BUILD = "BUILD-CANARY-9";
  const FAC = "FAC-CANARY-3";
  const PS = "PS-CANARY-7";

  it("generic Ticket capturedName — POST body and GET-list body", () => {
    const request = sanitizeRequest(
      req({
        url: "https://app.example.com/api/tickets",
        body: JSON.stringify({ kind: "generic", capturedName: TITLE, notes: "PRIVATE USER NOTE", orderId: "o1" }),
      }),
    );
    expect(request.body).not.toContain(TITLE);
    expect(request.body).not.toContain("PRIVATE USER NOTE");

    const response = sanitizeResponse(
      res({
        url: "https://app.example.com/api/tickets",
        body: JSON.stringify([
          { id: "t1", kind: "generic", capturedName: TITLE, notes: "PRIVATE USER NOTE", typeId: null },
          { id: "t2", kind: "manufacturing", capturedName: "Tungsten Carbide", typeId: 16673 },
        ]),
      }),
    );
    expect(response.body).not.toContain(TITLE);
    expect(response.body).not.toContain("PRIVATE USER NOTE");
    // A generated ticket's structured item name is intentionally preserved.
    expect(response.body).toContain("Tungsten Carbide");
  });

  it("custom Build name — POST body and GET-list body", () => {
    const request = sanitizeRequest(
      req({
        url: "https://app.example.com/api/builds",
        body: JSON.stringify({ name: BUILD, runs: 10, notes: "PRIVATE USER NOTE", recipe: { blueprintName: "Rifter Blueprint" } }),
      }),
    );
    expect(request.body).not.toContain(BUILD);

    const response = sanitizeResponse(
      res({
        url: "https://app.example.com/api/builds",
        body: JSON.stringify([
          {
            id: "b1",
            workspaceId: "w1",
            name: BUILD,
            runs: 10,
            revision: 2,
            notes: "PRIVATE USER NOTE",
            recipe: { blueprintName: "Rifter Blueprint" },
            productCategoryName: "Ship",
            parentBuildName: BUILD,
          },
        ]),
      }),
    );
    expect(response.body).not.toContain(BUILD);
    expect(response.body).not.toContain("PRIVATE USER NOTE");
    // Structured domain data survives.
    expect(response.body).toContain("Rifter Blueprint");
    expect(response.body).toContain("Ship");
  });

  it("custom facility name/notes — POST body and GET-list body", () => {
    const request = sanitizeRequest(
      req({
        url: "https://app.example.com/api/industry/facilities",
        body: JSON.stringify({ name: FAC, notes: "PRIVATE USER NOTE", structureTypeName: "Raitaru" }),
      }),
    );
    expect(request.body).not.toContain(FAC);
    expect(request.body).not.toContain("PRIVATE USER NOTE");

    const response = sanitizeResponse(
      res({
        url: "https://app.example.com/api/industry/facilities",
        body: JSON.stringify([
          {
            id: "f1",
            workspaceId: "w1",
            name: FAC,
            notes: "PRIVATE USER NOTE",
            structureTypeName: "Raitaru",
            solarSystemName: "Jita",
            materialReductionPercent: "1.0",
            rigs: [],
          },
        ]),
      }),
    );
    expect(response.body).not.toContain(FAC);
    expect(response.body).not.toContain("PRIVATE USER NOTE");
    expect(response.body).toContain("Raitaru");
    expect(response.body).toContain("Jita");
  });

  it("custom price-source name/description — PUT body and GET body", () => {
    const request = sanitizeRequest(
      req({
        url: "https://app.example.com/api/price-sources/ps1",
        method: "PUT",
        body: JSON.stringify({ expectedRevision: 1, name: PS, description: "my secret buy list" }),
      }),
    );
    expect(request.body).not.toContain(PS);
    expect(request.body).not.toContain("secret buy list");

    const response = sanitizeResponse(
      res({
        url: "https://app.example.com/api/price-sources/ps1",
        body: JSON.stringify({
          id: "ps1",
          workspaceId: "w1",
          name: PS,
          description: "my secret buy list",
          kind: "manual",
          itemCount: 3,
          items: [],
        }),
      }),
    );
    expect(response.body).not.toContain(PS);
    expect(response.body).not.toContain("secret buy list");
  });
});

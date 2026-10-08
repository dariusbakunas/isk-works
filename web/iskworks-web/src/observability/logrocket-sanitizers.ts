/**
 * Pure sanitizers for LogRocket network + URL capture.
 *
 * These run in the browser BEFORE any data is sent to LogRocket. The rule
 * (see docs/security/session-replay-privacy.md) is: metadata — method,
 * route, status, timing — is kept so we can still see failed/slow requests;
 * anything that could carry a credential, OAuth secret, EVE token, invite
 * code, free-form user text, or account financial value is stripped.
 *
 * Everything here is a pure function of its input so it can be unit-tested
 * with synthetic secret-bearing payloads.
 */

export const REDACTED = "[redacted]";

/**
 * Shapes LogRocket passes to `network.requestSanitizer` / `responseSanitizer`.
 * The `logrocket` package declares these on an un-exported internal
 * namespace, so they are mirrored here (bodies are always serialized
 * strings).
 */
export interface IRequest {
  reqId: string;
  url: string;
  method: string;
  headers: Record<string, string | null | undefined>;
  body?: string;
  referrer?: string;
}

export interface IResponse {
  reqId: string;
  status?: number;
  method: string;
  url?: string;
  headers: Record<string, string | null | undefined>;
  body?: string;
}

/** Query-string parameters scrubbed from every captured URL (page + network). */
const SENSITIVE_QUERY_PARAMS = new Set([
  "code",
  "state",
  "token",
  "access_token",
  "refresh_token",
  "id_token",
  "session",
  "session_token",
  "secret",
  "sig",
  "signature",
  "code_verifier",
  "code_challenge",
  "invite",
  "invite_code",
  "invitecode",
]);

/** Request/response headers dropped entirely, matched case-insensitively. */
const SENSITIVE_HEADER = (name: string): boolean => {
  const lower = name.toLowerCase();
  if (lower === "x-logrocket-url") return false; // our own backend-correlation tag
  return (
    /^(authorization|proxy-authorization|cookie|set-cookie|x-csrf-token|x-xsrf-token|x-auth-token|x-api-key)$/.test(
      lower,
    ) || /(token|secret|password|api[-_]?key|auth)/.test(lower)
  );
};

/**
 * JSON body keys whose value is always redacted, in requests AND responses,
 * at any depth. Credentials, OAuth/PKCE values, EVE tokens, invite codes,
 * and free-form user-authored prose.
 */
const ALWAYS_REDACT_KEY =
  /^(authorization|authorization_?url|cookie|set_?cookie|token|access_?token|refresh_?token|id_?token|session_?token|secret|client_?secret|password|passwd|api_?key|encryption_?key|token_?encryption_?key|state|code|code_?verifier|code_?challenge|pkce|invite|invite_?code|notes?|reason|source_?reference|description)$/i;

/**
 * Keys redacted only in REQUEST bodies — labels the user typed themselves.
 * `name`/`title` are blanket-redacted on the request side because a write
 * body is, by definition, what the user just entered. On the RESPONSE side
 * these are handled contextually (see contextualUserLabelKeys) so a
 * canonical EVE name echoed back stays visible.
 */
const REQUEST_ONLY_REDACT_KEY = /^(name|title|captured_?name|display_?name)$/i;

/**
 * Keys redacted only in RESPONSE bodies — account financial aggregates and
 * account-linked character identity. (Character names are DOM-masked too;
 * they must not leak back through a captured API response.)
 */
const RESPONSE_ONLY_REDACT_KEY =
  /^(wallet_?balance|available_?balance|income|expenses?|net_?isk|average_?daily_?isk|counterparty|counterparty_?name|character_?name|access_?character_?name)$/i;

const MAX_DEPTH = 12;

/**
 * Extra keys to redact on THIS object because of what the object *is* — a
 * user-authored entity whose label/title is free-form. Evaluated at every
 * object node during the walk, so an entity nested inside another response
 * (a facility inside a ticket's execution snapshot, a build inside a graph
 * projection) is caught too.
 *
 * Deliberately narrow: each rule requires a distinctive combination of
 * sibling keys, so structured domain objects that merely happen to have a
 * `name` — an EVE market-category node, a rig target filter, an SDE search
 * hit — are left untouched.
 */
export function contextualUserLabelKeys(value: unknown): Set<string> {
  const keys = new Set<string>();
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return keys;
  }
  const obj = value as Record<string, unknown>;
  const has = (k: string): boolean => Object.prototype.hasOwnProperty.call(obj, k);
  const str = (k: string): boolean => typeof obj[k] === "string";

  // Ticket / TicketSummary: `capturedName` is the user's free-form title
  // ONLY for a generic ticket. For manufacturing / reaction / acquisition
  // it is a canonical item name (e.g. "Tungsten Carbide") and must stay
  // visible; prerequisite/blocker rows carry a RequirementKind, never
  // "generic".
  if (obj.kind === "generic" && str("capturedName")) {
    keys.add("capturedName");
  }

  // Build: `name` / `parentBuildName` are user labels; `recipe` and the
  // product category names stay.
  if (str("name") && has("workspaceId") && has("runs") && (has("recipe") || has("revision"))) {
    keys.add("name");
    if (has("parentBuildName")) keys.add("parentBuildName");
  }

  // Facility profile: `name` is a user label; `structureTypeName` /
  // `solarSystemName` carry the canonical names and stay. Rig sub-objects
  // also have `materialReductionPercent` but no `name`/`rigs`.
  if (str("name") && has("materialReductionPercent") && has("rigs")) {
    keys.add("name");
  }

  // Price source: `name` + `description` are user text (description is
  // already in ALWAYS_REDACT_KEY; kept here for clarity).
  if (str("name") && has("itemCount") && (has("description") || has("items"))) {
    keys.add("name");
  }

  // Saved finance filter (finance responses are also dropped wholesale).
  if (str("name") && has("filter")) {
    keys.add("name");
  }

  // Acquisition Run: user-nameable shopping batch. `displayId` + `ownerId` +
  // `status` alongside a bare `name` is unique to it (a Ticket has
  // `capturedName`, not `name`; an Epic/Order has `displayName`).
  if (str("name") && has("displayId") && has("ownerId") && has("status")) {
    keys.add("name");
  }

  // Facility import result / preview row.
  if (
    str("name") &&
    (has("classification") || has("status")) &&
    (has("existingId") || has("existingName") || has("message"))
  ) {
    keys.add("name");
    if (has("existingName")) keys.add("existingName");
  }

  return keys;
}

function pathnameOf(rawUrl: string | undefined): string {
  if (!rawUrl) return "";
  try {
    return new URL(rawUrl, "http://iskworks.invalid").pathname;
  } catch {
    return "";
  }
}

/**
 * Auth / OAuth / admin routes. Their request and response bodies are dropped
 * wholesale — none of them carry anything we can safely record.
 */
export function isAuthRoute(pathname: string): boolean {
  return (
    pathname.startsWith("/api/auth/") ||
    // Admin invite endpoints return/accept plaintext invite codes.
    pathname.startsWith("/api/admin/") ||
    pathname.startsWith("/api/eve/oauth/") ||
    /\/api\/eve\/connections\/authorize$/.test(pathname) ||
    /\/api\/eve\/connections\/[^/]+\/refresh$/.test(pathname)
  );
}

/**
 * Routes whose RESPONSE body is dropped: wallet balances, ISK transaction
 * values, counterparties, and value aggregates.
 */
export function isSensitiveResponseRoute(pathname: string): boolean {
  return (
    pathname.startsWith("/api/finance/") ||
    /\/wallet-transactions$/.test(pathname) ||
    pathname.startsWith("/api/eve/sync-runs") ||
    pathname === "/api/assets/summary"
  );
}

/**
 * Bulk data-dump endpoints (`.../export`) — CSV or full JSON snapshots of
 * the user's inventory / assets / facilities / transactions. Low debugging
 * value, high disclosure risk, and CSV bodies would slip past the JSON
 * walker. The whole response body is dropped.
 */
export function isBulkExportRoute(pathname: string): boolean {
  return /\/export$/.test(pathname);
}

/**
 * Whether a request body is a non-JSON form payload (multipart file upload,
 * url-encoded form). The JSON walker can't reach into these, so the body is
 * dropped. Detected from the content-type header; a boundary-carrying
 * `multipart/form-data` is set automatically for `FormData` bodies.
 */
function isFormBody(headers: Record<string, string | null | undefined> | undefined): boolean {
  if (!headers) return false;
  for (const [name, value] of Object.entries(headers)) {
    if (name.toLowerCase() !== "content-type" || typeof value !== "string") continue;
    return /multipart\/form-data|application\/x-www-form-urlencoded/i.test(value);
  }
  return false;
}

/** Routes that accept an uploaded file / form body (belt-and-suspenders). */
function isFileUploadRoute(pathname: string): boolean {
  return /^\/api\/industry\/market-imports(\/preview)?$/.test(pathname);
}

/** Scrub sensitive query-parameter values from a URL string. */
export function sanitizeUrl(rawUrl: string): string {
  try {
    const url = new URL(rawUrl, "http://iskworks.invalid");
    let mutated = false;
    for (const key of [...url.searchParams.keys()]) {
      if (SENSITIVE_QUERY_PARAMS.has(key.toLowerCase())) {
        url.searchParams.set(key, REDACTED);
        mutated = true;
      }
    }
    if (!mutated) return rawUrl;
    // Preserve the original shape (relative vs absolute).
    return /^https?:\/\//i.test(rawUrl)
      ? url.toString()
      : `${url.pathname}${url.search}${url.hash}`;
  } catch {
    return rawUrl;
  }
}

/** LogRocket browser.urlSanitizer hook. */
export function sanitizePageUrl(rawUrl: string): string {
  return sanitizeUrl(rawUrl);
}

function sanitizeHeaders(
  headers: Record<string, string | null | undefined> | undefined,
): Record<string, string | null | undefined> | undefined {
  if (!headers) return headers;
  const out: Record<string, string | null | undefined> = {};
  for (const [name, value] of Object.entries(headers)) {
    out[name] = SENSITIVE_HEADER(name) ? REDACTED : value;
  }
  return out;
}

function redactValue(
  value: unknown,
  isRedactedKey: (key: string) => boolean,
  depth: number,
  seen: WeakSet<object>,
): unknown {
  if (depth > MAX_DEPTH || value === null || typeof value !== "object") {
    return value;
  }
  if (seen.has(value as object)) return REDACTED;
  seen.add(value as object);

  if (Array.isArray(value)) {
    return value.map((item) => redactValue(item, isRedactedKey, depth + 1, seen));
  }

  // Keys to redact because of what this specific object is (a Build, a
  // generic Ticket, a facility profile, ...), on top of the flat key rules.
  const contextual = contextualUserLabelKeys(value);

  const out: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
    out[key] =
      isRedactedKey(key) || contextual.has(key)
        ? REDACTED
        : redactValue(item, isRedactedKey, depth + 1, seen);
  }
  return out;
}

/**
 * Redact a JSON string body by key (flat rules + per-object contextual
 * rules). A body that is not JSON is returned unchanged — callers that know
 * a route carries a non-JSON sensitive body drop it before reaching here.
 */
export function redactJsonBody(
  body: string | undefined,
  side: "request" | "response",
): string | undefined {
  if (body === undefined || body === "") return body;

  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return body;
  }

  const isRedactedKey = (key: string): boolean => {
    if (ALWAYS_REDACT_KEY.test(key)) return true;
    if (side === "request" && REQUEST_ONLY_REDACT_KEY.test(key)) return true;
    if (side === "response" && RESPONSE_ONLY_REDACT_KEY.test(key)) return true;
    return false;
  };

  const redacted = redactValue(parsed, isRedactedKey, 0, new WeakSet());
  return JSON.stringify(redacted);
}

/** LogRocket network.requestSanitizer hook. */
export function sanitizeRequest(request: IRequest): IRequest {
  const pathname = pathnameOf(request.url);
  const base: IRequest = {
    ...request,
    url: sanitizeUrl(request.url),
    headers: sanitizeHeaders(request.headers) ?? request.headers,
  };
  if (
    isAuthRoute(pathname) ||
    isFileUploadRoute(pathname) ||
    isFormBody(request.headers)
  ) {
    return { ...base, body: undefined };
  }
  return { ...base, body: redactJsonBody(request.body, "request") };
}

/** LogRocket network.responseSanitizer hook. */
export function sanitizeResponse(response: IResponse): IResponse {
  const pathname = pathnameOf(response.url);
  const base: IResponse = {
    ...response,
    url: response.url ? sanitizeUrl(response.url) : response.url,
    headers: sanitizeHeaders(response.headers) ?? response.headers,
  };
  if (
    isAuthRoute(pathname) ||
    isSensitiveResponseRoute(pathname) ||
    isBulkExportRoute(pathname)
  ) {
    return { ...base, body: undefined };
  }
  return { ...base, body: redactJsonBody(response.body, "response") };
}

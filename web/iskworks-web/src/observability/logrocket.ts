import LogRocket from "logrocket";
import setupLogRocketReact from "logrocket-react";

import {
  sanitizePageUrl,
  sanitizeRequest,
  sanitizeResponse,
} from "./logrocket-sanitizers";
import { type RuntimeConfig, runtimeConfig } from "../runtime-config";

/**
 * Central LogRocket configuration for the ISK Works alpha.
 *
 * Session replay is retained for debugging, but configured privacy-first:
 * every text input is masked by default, sensitive DOM regions carry
 * `data-private` (see ./private.tsx), and all network + URL capture passes
 * through the sanitizers in ./logrocket-sanitizers.ts before leaving the
 * browser. See docs/security/session-replay-privacy.md.
 */

/**
 * Nothing is recorded unless BOTH a non-empty app ID and an explicit enable
 * flag are configured. When the runtime config exists (every container), it
 * is the only source; the VITE_* build variables are a fallback for local
 * `vite dev` only, where no /config.js is generated.
 */
export function resolveReplaySettings(
  runtime: RuntimeConfig | undefined,
  build: { appId?: string; enabled?: string },
): { appId: string; enabled: boolean } {
  const appId = (runtime ? runtime.logRocketAppId : build.appId)?.trim() ?? "";
  const flag = runtime ? runtime.logRocketEnabled === true : build.enabled === "true";
  return { appId, enabled: flag && appId !== "" };
}

function replaySettings() {
  return resolveReplaySettings(
    runtimeConfig(),
    {
      appId: import.meta.env.VITE_LOGROCKET_APP_ID,
      enabled: import.meta.env.VITE_LOGROCKET_ENABLED,
    },
  );
}

// Release identifier attached to every recording. Deployments should pass
// VITE_APP_RELEASE (a version string or git SHA); falls back to "dev".
const RELEASE = import.meta.env.VITE_APP_RELEASE ?? "dev";

// Request header carrying the current session-replay URL to our own API,
// which lifts it into the request's tracing span (see
// apps/iskworks-api/src/observability.rs) so backend log lines sit next to
// the matching replay.
const SESSION_HEADER = "X-LogRocket-URL";

let started = false;
let sessionUrl: string | null = null;

/** Whether this deployment is configured to record at all. */
export function isLogRocketEnabled(): boolean {
  return (
    typeof window !== "undefined" &&
    replaySettings().enabled &&
    import.meta.env.MODE !== "test"
  );
}

/**
 * Initialise LogRocket. Must run client-side, before the app renders. Safe
 * to call more than once. No-ops entirely unless isLogRocketEnabled().
 */
export function initLogRocket(): void {
  if (started || !isLogRocketEnabled()) return;
  started = true;

  LogRocket.init(replaySettings().appId, {
    release: RELEASE,
    // Do not collect IP / GeoIP for alpha users.
    shouldCaptureIP: false,
    dom: {
      // Privacy by default: never transmit raw text typed into any
      // <input>, <select>, or <textarea>. Non-sensitive domain text
      // (item names, quantities, runs, ME/TE) stays visible because
      // textSanitizer is left off; sensitive rendered regions opt in via
      // the `data-private` attribute (see ./private.tsx).
      inputSanitizer: true,
      textSanitizer: false,
      redactSelectors: [
        // Every multiline field in this app is free-form user text
        // (ticket / facility / inventory / blueprint notes). Redact them
        // unconditionally rather than rely on inputSanitizer covering
        // <textarea>.
        "textarea",
        // Defense in depth for the highest-risk regions, in case a JSX
        // `data-private` marker regresses. Stable selectors already in the
        // Finance UI.
        'section[aria-label="Transaction summary"]',
        ".finance-table tbody",
      ],
    },
    network: {
      requestSanitizer: (request) => sanitizeRequest(request),
      responseSanitizer: (response) => sanitizeResponse(response),
    },
    browser: {
      urlSanitizer: (url) => sanitizePageUrl(url),
    },
    // Console capture stays on (useful for debugging); the rule is to fix
    // unsafe logs, not blanket-disable. There is currently no console.*
    // usage in application code.
  });

  // logrocket-react@7 hooks React's fiber tree directly and takes no
  // arguments; it lets sessions be filtered by React component clicks.
  setupLogRocketReact();

  // Fires once the session registers, and again when a new session starts
  // (e.g. after 30 min idle). Cache the latest URL for synchronous reads.
  LogRocket.getSessionURL((url) => {
    sessionUrl = url;
  });

  installApiRequestTagging();
}

/**
 * Associate the recording with a pseudonymous, opaque identifier only. The
 * workspace UUID is not user-facing PII; no email, real name, or EVE
 * character identity is sent to LogRocket. `alphaUser` is the one trait,
 * so alpha sessions stay filterable without enriching the third-party set.
 */
export function identifyViewer(session: { workspaceId?: string | null }): void {
  if (!started) return;
  const uid = session.workspaceId;
  if (!uid) return;
  LogRocket.identify(String(uid), { alphaUser: true });
}

/**
 * Record a custom product event. No-ops until initLogRocket() has run.
 */
export function trackEvent(
  name: string,
  properties?: Record<string, string | number | boolean>,
): void {
  if (!started) return;
  LogRocket.track(name, properties);
}

/**
 * Bridge a backend correlation id into the LogRocket timeline. Called for
 * API errors that carry one (redacted internal failures). Combined with the
 * X-LogRocket-URL request header, this makes replay <-> frontend error <->
 * backend log a two-way lookup without any shared storage.
 */
export function reportBackendError(correlationId: string, status: number): void {
  if (!started) return;
  LogRocket.track("backend_error", { correlationId, status });
}

/**
 * Whether an outbound request targets our own API and should carry the
 * session header. Same-origin `/api/*` only — never third parties, and
 * cross-origin API setups are left untagged rather than provoke a CORS
 * preflight for the custom header.
 */
export function shouldTagApiRequest(rawUrl: string): boolean {
  if (typeof window === "undefined") return false;
  try {
    const url = new URL(rawUrl, window.location.origin);
    return url.origin === window.location.origin && url.pathname.startsWith("/api/");
  } catch {
    return false;
  }
}

function requestUrl(input: RequestInfo | URL): string {
  if (typeof input === "string") return input;
  if (input instanceof URL) return input.toString();
  return input.url;
}

/**
 * Wrap window.fetch once so same-origin `/api/*` requests carry the
 * X-LogRocket-URL header. A wrapper rather than edits to each of the ~8
 * per-domain fetch helpers in src/api/: one choke point, and nothing to
 * miss when a new API client is added. No-op for every other request.
 */
function installApiRequestTagging(): void {
  const target = window as typeof window & { __lrFetchTagged?: boolean };
  if (target.__lrFetchTagged) return;

  const original = window.fetch.bind(window);
  target.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
    if (!sessionUrl || !shouldTagApiRequest(requestUrl(input))) {
      return original(input, init);
    }
    const headers = new Headers(
      init?.headers ?? (input instanceof Request ? input.headers : undefined),
    );
    if (!headers.has(SESSION_HEADER)) headers.set(SESSION_HEADER, sessionUrl);
    return original(input, { ...init, headers });
  };
  target.__lrFetchTagged = true;
}

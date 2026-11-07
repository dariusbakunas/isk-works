const RELOAD_KEY = "iskworks:maintenance-reload-at";
const RELOAD_GUARD_MS = 60_000;

/**
 * Reload at most once per guard window. Returns without reloading when
 * sessionStorage is unavailable, because nothing could then stop a reload loop.
 */
function reloadOnce() {
  try {
    const lastReload = Number(sessionStorage.getItem(RELOAD_KEY) ?? 0);
    if (Date.now() - lastReload < RELOAD_GUARD_MS) return;
    sessionStorage.setItem(RELOAD_KEY, String(Date.now()));
  } catch {
    return;
  }
  window.location.reload();
}

/**
 * While the Cloudflare maintenance Worker is attached, it answers API calls with
 * `503` and `X-Maintenance: 1`. An app that is already open, or loaded from the
 * browser cache, would show those as ordinary errors, so reload the page and let
 * the Worker serve its maintenance page instead.
 *
 * This wraps `fetch` because the API clients each make their own calls.
 */
export function installMaintenanceReload() {
  const originalFetch = globalThis.fetch.bind(globalThis);

  globalThis.fetch = async (...args: Parameters<typeof fetch>) => {
    const response = await originalFetch(...args);
    if (response.status === 503 && response.headers.get("X-Maintenance") === "1") {
      reloadOnce();
    }
    return response;
  };
}

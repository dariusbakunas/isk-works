/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_BASE_URL?: string;
  /** Local `vite dev` only: LogRocket app ID. Deployed containers configure
   * replay at runtime instead (LOGROCKET_APP_ID, see public/config.js). */
  readonly VITE_LOGROCKET_APP_ID?: string;
  /** Release identifier (version string or git SHA) for LogRocket recordings. */
  readonly VITE_APP_RELEASE?: string;
  /** Local `vite dev` only: "true" turns on LogRocket session replay. */
  readonly VITE_LOGROCKET_ENABLED?: string;
  /** Local `vite dev` only: community links. Deployed containers set
   * ISKWORKS_SUPPORT_URL / _SUPPORT_LABEL / _DONATION_CHARACTER / _SOURCE_URL. */
  readonly VITE_SUPPORT_URL?: string;
  readonly VITE_SUPPORT_LABEL?: string;
  readonly VITE_DONATION_CHARACTER?: string;
  readonly VITE_SOURCE_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

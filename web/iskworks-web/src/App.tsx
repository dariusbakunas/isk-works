import { CircleHelp, LogOut, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { Navigate, Route, Routes, useLocation, useNavigate, useParams } from "react-router";

import { getSession, logout } from "./api/auth";
import { ApiError, createWorkspace, getWorkspaceState, type WorkspaceStateResponse } from "./api/workspace";
import { identifyViewer, trackEvent } from "./observability/logrocket";
import { AppFooter } from "./components/app-footer";
import { HELP_PATH, LEGAL_PATH } from "./components/community-links";
import { AccountMenu, ApiVersionLine } from "./components/navigation/account-menu";
import { CommandPalette } from "./components/navigation/command-palette";
import { MobileNav } from "./components/navigation/mobile-nav";
import { NavigationRail } from "./components/navigation/navigation-rail";
import type { PaletteEntry } from "./components/navigation/palette-search";
import { useNavPinned } from "./components/navigation/use-nav-pinned";
import { useNavRecents } from "./components/navigation/use-nav-recents";
import { AdminPage } from "./features/admin/admin-page";
import { LoginPage } from "./features/auth/login-page";
import {
  ButtonLink,
  EmptyState,
  InlineAlert,
  LoadingState,
  PageHeader,
  Panel,
  TextField,
} from "./components/primitives";
import {
  CreateBuildPage,
} from "./features/industry/builds/build-worksheet-editor";
import { BuildWorkspacePage } from "./features/industry/builds/build-workspace-page";
import { BuildsPage } from "./features/industry/builds/builds-page";
import { BlueprintsPage } from "./features/industry/blueprints/blueprints-page";
import { BoardPage } from "./features/board/board-page";
import { CalendarPage } from "./features/calendar/calendar-page";
import { FacilitiesPage } from "./features/industry/facilities/facilities-page";
import { FacilityDefaultsPage } from "./features/industry/facilities/facility-defaults-page";
import { FinanceWorkspace } from "./features/finance/finance-workspace";
import { JobsPage } from "./features/jobs/jobs-page";
import { MarketOrdersPage } from "./features/market-orders/market-orders-page";
import { PriceSourceDetailPage } from "./features/industry/prices/price-source-detail-page";
import {
  NewPriceSourcePage,
  PricesPage,
} from "./features/industry/prices/prices-page";
import { MarketImportsPage } from "./features/industry/market/market-imports-page";
import { MarketBrowserPage } from "./features/industry/market-browser/market-browser-page";
import { InventoryPage } from "./features/industry/inventory/inventory-page";
import { RunwayPage } from "./features/industry/inventory/runway/runway-page";
// Consumption pulls in the charting library, so it loads on demand.
const ConsumptionPage = lazy(() =>
  import("./features/industry/inventory/consumption/consumption-page").then((module) => ({
    default: module.ConsumptionPage,
  })),
);
import { OpportunitiesPage } from "./features/industry/opportunities/opportunities-page";
import { PlanetaryPage } from "./features/industry/planetary/planetary-page";
import { AssetsWorkspacePage } from "./features/assets/assets-workspace-page";
import { CharactersPage } from "./features/characters/characters-page";
import { FittingsPage } from "./features/fittings/fittings-page";
import { SkillsPage } from "./features/skills/skills-page";
import { EveCallbackPage } from "./features/esi/eve-callback-page";
import { HelpPage } from "./features/help/help-page";
import { LegalPage } from "./features/legal/legal-page";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string; code: string }
  | { status: "ready"; data: WorkspaceStateResponse };

type AuthGateState =
  | { status: "checking" }
  | { status: "unauthenticated"; inviteRequired: boolean }
  | { status: "authenticated"; characterName: string; isAdmin: boolean }
  // The backend EXPLICITLY reported that authentication is intentionally
  // disabled (503 `auth_not_configured`). A public deployment sets
  // `ISKWORKS_AUTH_REQUIRED=true` and can never return this -- it only
  // happens for a local/dev API with no `EVE_SSO_CLIENT_ID`. This is the
  // one path that enters the legacy no-session workspace mode.
  | { status: "auth-disabled" }
  // The session could not be verified: network failure, timeout, a 5xx, or
  // an unexpected response shape. This is NOT "no session" (that's
  // `unauthenticated` -> LoginPage) and NOT "auth disabled". Fail closed:
  // show a recoverable error, never a product surface, never a data call.
  | { status: "unavailable" };

export function App() {
  // Help and About & Legal must be readable without signing in, so they sit
  // outside the auth gate.
  return (
    <Routes>
      <Route path={LEGAL_PATH} element={<LegalPage />} />
      <Route path={HELP_PATH} element={<HelpPage />} />
      <Route path={`${HELP_PATH}/:slug`} element={<HelpPage />} />
      <Route path="*" element={<AuthGatedApp />} />
    </Routes>
  );
}

function AuthGatedApp() {
  const [authState, setAuthState] = useState<AuthGateState>({ status: "checking" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    setAuthState({ status: "checking" });
    getSession()
      .then((session) => {
        if (!active) return;
        if (session.authenticated) {
          identifyViewer({ workspaceId: session.workspaceId });
        }
        setAuthState(
          session.authenticated
            ? {
                status: "authenticated",
                characterName: session.characterName ?? "",
                isAdmin: session.isAdmin === true,
              }
            : { status: "unauthenticated", inviteRequired: session.inviteRequired === true },
        );
      })
      .catch((caught: unknown) => {
        if (!active) return;
        // Only an explicit backend signal that auth is intentionally off
        // (503 `auth_not_configured`) may enter the legacy unauthenticated
        // mode. Every other failure -- rejected request, timeout, network
        // error, arbitrary 500/503 -- is "we could not verify your
        // session" and must fail closed into an error/retry state.
        const authDisabled =
          caught instanceof ApiError &&
          caught.status === 503 &&
          caught.body?.code === "auth_not_configured";
        setAuthState({ status: authDisabled ? "auth-disabled" : "unavailable" });
      });
    return () => {
      active = false;
    };
  }, [attempt]);

  if (authState.status === "checking") {
    return <LoadingState>Checking session...</LoadingState>;
  }

  if (authState.status === "unavailable") {
    return <AuthUnavailable onRetry={() => setAttempt((count) => count + 1)} />;
  }

  if (authState.status === "unauthenticated") {
    return <LoginPage inviteRequired={authState.inviteRequired} />;
  }

  // "authenticated", or "auth-disabled" (local/dev API that explicitly
  // reported auth is off -> the legacy single-workspace path).
  return (
    <WorkspaceApp
      characterName={authState.status === "authenticated" ? authState.characterName : undefined}
      isAdmin={authState.status === "authenticated" && authState.isAdmin}
    />
  );
}

// Session verification failed for a reason that is NOT "you are signed
// out". We never mount the authenticated app (Board/Characters/Builds/...)
// or fire workspace/data requests from here -- the user retries, which
// re-runs the session check.
function AuthUnavailable({ onRetry }: { onRetry: () => void }) {
  return (
    <LoadingState>
      <div className="grid max-w-md gap-3">
        <InlineAlert title="We couldn't verify your session">
          The service may be temporarily unavailable. You have not been signed out --
          try again in a moment.
        </InlineAlert>
        <button className="iw-button-primary justify-self-start" onClick={onRetry} type="button">
          Retry
        </button>
      </div>
    </LoadingState>
  );
}

function WorkspaceApp({ characterName, isAdmin = false }: { characterName?: string; isAdmin?: boolean }) {
  const [loadState, setLoadState] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    let active = true;
    getWorkspaceState()
      .then((data) => {
        if (active) setLoadState({ status: "ready", data });
      })
      .catch((error: ApiError) => {
        if (active) {
          setLoadState({
            status: "error",
            code: error.body?.code ?? "api_unavailable",
            message: error.body?.message ?? "ISK Works API is unavailable.",
          });
        }
      });
    return () => {
      active = false;
    };
  }, []);

  if (loadState.status === "loading") {
    return <LoadingState>Loading workspace state...</LoadingState>;
  }

  if (loadState.status === "error") {
    return (
      <LoadingState>
        <InlineAlert title="Workspace state unavailable">
          {loadState.code === "persistence_unavailable"
            ? "The database is unavailable. Check PostgreSQL and the API logs."
            : loadState.message}
        </InlineAlert>
      </LoadingState>
    );
  }

  return (
    <Routes>
      <Route
        path="/setup"
        element={<SetupPage workspaceState={loadState.data} onConfigured={(data) => setLoadState({ status: "ready", data })} />}
      />
      <Route
        path="/*"
        element={
          <ProtectedShell
            workspaceState={loadState.data}
            characterName={characterName}
            isAdmin={isAdmin}
          />
        }
      />
    </Routes>
  );
}

function SetupPage({
  workspaceState,
  onConfigured,
}: {
  workspaceState: WorkspaceStateResponse;
  onConfigured: (state: WorkspaceStateResponse) => void;
}) {
  const [name, setName] = useState("");
  const [fieldError, setFieldError] = useState("");
  const [formError, setFormError] = useState("");
  const [saving, setSaving] = useState(false);
  const navigate = useNavigate();

  if (workspaceState.configured) {
    return <Navigate to="/characters" replace />;
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setFieldError("");
    setFormError("");
    setSaving(true);
    try {
      const created = await createWorkspace(name);
      trackEvent("Workspace Created");
      onConfigured(created);
      navigate("/characters", { replace: true });
    } catch (error) {
      if (error instanceof ApiError) {
        setFieldError(error.body.fields?.name ?? "");
        if (!error.body.fields?.name) {
          setFormError(
            error.body.code === "workspace_already_configured"
              ? "Workspace is already configured. Return to Characters."
              : "Changes were not saved. Try again before leaving this page.",
          );
        }
      } else {
        setFormError("Changes were not saved. Try again before leaving this page.");
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <main className="grid min-h-[var(--iw-viewport-h)] place-items-center px-4 py-8">
      <section className="iw-panel w-full max-w-3xl p-6">
        <PageHeader eyebrow="Manual-first industry workspace" title="Create your workspace">
          Start an ISK Works workspace for builds, tracked inventory costs, price assumptions, and sale allocation.
        </PageHeader>
        <form className="grid gap-4" onSubmit={submit} noValidate>
          <Panel>
            <h2 className="mb-3 text-base font-semibold">Workspace</h2>
            <TextField id="workspace-name" label="Workspace name" value={name} error={fieldError} onChange={setName} />
          </Panel>
          {formError ? <InlineAlert title="Setup did not finish">{formError}</InlineAlert> : null}
          <button className="iw-button-primary justify-self-start" disabled={saving} type="submit">
            {saving ? "Creating workspace..." : "Start workspace"}
          </button>
        </form>
      </section>
    </main>
  );
}

function ProtectedShell({
  workspaceState,
  characterName,
  isAdmin,
}: {
  workspaceState: WorkspaceStateResponse;
  characterName?: string;
  isAdmin: boolean;
}) {
  if (!workspaceState.configured || !workspaceState.workspace) {
    return <Navigate to="/setup" replace />;
  }

  return (
    <AppShell
      apiVersion={workspaceState.version}
      workspaceName={workspaceState.workspace.name}
      characterName={characterName}
      isAdmin={isAdmin}
    >
      <Routes>
        {/* Characters is the alpha home; there is no Overview route. */}
        <Route path="/" element={<Navigate replace to="/characters" />} />
        <Route path="/characters" element={<CharactersPage />} />
        <Route path="/skills/*" element={<SkillsPage />} />
        <Route path="/fittings/*" element={<FittingsPage />} />
        <Route path="/opportunities" element={<OpportunitiesPage />} />
        <Route path="/builds" element={<BuildsPage />} />
        <Route path="/builds/new" element={<CreateBuildPage />} />
        <Route path="/builds/blueprints" element={<BlueprintsPage />} />
        <Route
          path="/builds/:rootBuildId/producers/:producerBuildId"
          element={<BuildWorkspacePage />}
        />
        <Route path="/builds/:buildId" element={<BuildWorkspacePage />} />
        <Route path="/builds/:buildId/edit" element={<BuildWorkspacePage canonicalize />} />
        <Route path="/orders/:orderId" element={<OrderDeepLinkRedirect />} />
        <Route path="/board" element={<BoardPage />} />
        <Route path="/jobs/*" element={<JobsPage />} />
        <Route path="/market-orders" element={<MarketOrdersPage />} />
        <Route path="/calendar" element={<CalendarPage />} />
        <Route path="/inventory" element={<InventoryPage />} />
        <Route path="/inventory/runway" element={<RunwayPage />} />
        <Route
          path="/inventory/consumption"
          element={<Suspense fallback={<div className="iw-panel p-3 text-sm text-muted">Loading consumption…</div>}><ConsumptionPage /></Suspense>}
        />
        <Route path="/assets" element={<AssetsWorkspacePage />} />
        <Route path="/market" element={<MarketBrowserPage />} />
        <Route path="/prices" element={<PricesPage />} />
        <Route path="/prices/imports" element={<MarketImportsPage />} />
        <Route path="/prices/new" element={<NewPriceSourcePage />} />
        <Route path="/prices/:priceSourceId" element={<PriceSourceDetailPage />} />
        <Route path="/facilities" element={<FacilitiesPage />} />
        <Route path="/facilities/defaults" element={<FacilityDefaultsPage />} />
        <Route path="/planetary" element={<PlanetaryPage />} />
        <Route path="/finance/*" element={<FinanceWorkspace />} />
        {/* EVE character connection management lives on /characters; these
            two are kept as redirects for old bookmarks/links, and the OAuth
            callback path stays exactly as-is since the backend redirects
            here by that literal path (apps/iskworks-api/src/routes/esi.rs). */}
        <Route path="/settings" element={<Navigate replace to="/characters" />} />
        <Route path="/settings/eve" element={<Navigate replace to="/characters" />} />
        <Route path="/settings/eve/callback" element={<EveCallbackPage />} />
        <Route path="/admin/*" element={<AdminPage isAdmin={isAdmin} />} />
        <Route path="*" element={<NotFoundPage />} />
      </Routes>
    </AppShell>
  );
}

// There is no dedicated Order page: an Epic/Order card opens the Epic
// Inspector in place (see order-card.tsx / board-page.tsx). This keeps
// bookmarks and any external `/orders/:id` link alive by landing on the
// Board with that Epic's inspector already open, rather than a dead route.
function OrderDeepLinkRedirect() {
  const { orderId = "" } = useParams();
  return <Navigate replace to={`/board?epic=${orderId}`} />;
}

function AppShell({
  apiVersion,
  workspaceName,
  characterName,
  isAdmin,
  children,
}: {
  apiVersion: string;
  workspaceName: string;
  characterName?: string;
  isAdmin: boolean;
  children: React.ReactNode;
}) {
  const [loggingOut, setLoggingOut] = useState(false);
  const [pinned, togglePinned] = useNavPinned();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const { recents, recordVisit } = useNavRecents();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const footerRef = useFooterHeightVariable();

  useEffect(() => recordVisit(pathname), [pathname, recordVisit]);

  // ⌘K / Ctrl K, from anywhere, inputs included; again to close.
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "k") return;
      event.preventDefault();
      setPaletteOpen((open) => !open);
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  async function signOut() {
    setLoggingOut(true);
    try {
      await logout();
    } finally {
      // A full reload (not client-side navigation) so the top-level session
      // check in App() re-runs from scratch and shows LoginPage.
      window.location.assign("/");
    }
  }

  const paletteActions: PaletteEntry[] = [
    {
      id: "action:pin",
      kind: "action",
      label: pinned ? "Unpin navigation" : "Pin navigation",
      group: "Actions",
      icon: pinned ? PanelLeftClose : PanelLeftOpen,
      keywords: ["sidebar", "rail"],
      run: togglePinned,
    },
    {
      id: "action:help",
      kind: "action",
      label: "Open help",
      group: "Actions",
      icon: CircleHelp,
      keywords: ["docs"],
      run: () => navigate(HELP_PATH),
    },
    ...(characterName
      ? [{ id: "action:sign-out", kind: "action" as const, label: "Sign out", group: "Actions", icon: LogOut, keywords: ["logout"], run: signOut }]
      : []),
  ];
  const openPalette = () => setPaletteOpen(true);

  const renderAccount = (expanded: boolean) =>
    characterName ? (
      <AccountMenu
        apiVersion={apiVersion}
        characterName={characterName}
        expanded={expanded}
        onSignOut={signOut}
        signingOut={loggingOut}
      />
    ) : (
      <ApiVersionLine apiVersion={apiVersion} expanded={expanded} />
    );

  return (
    <div className="flex min-h-[var(--iw-viewport-h)] text-foreground">
      <div className="sticky top-[var(--iw-banner-h,0px)] z-30 hidden h-[var(--iw-viewport-h)] shrink-0 md:block">
        <NavigationRail
          isAdmin={isAdmin}
          onOpenSearch={openPalette}
          onTogglePinned={togglePinned}
          pinned={pinned}
          renderAccount={renderAccount}
          workspaceName={workspaceName}
        />
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <MobileNav isAdmin={isAdmin} onOpenSearch={openPalette} renderAccount={renderAccount} workspaceName={workspaceName} />
        <div className="iw-app-layout flex-1 lg:grid">
          <main className="iw-page-container min-w-0">{children}</main>
          <aside data-testid="app-right-rail" id="app-right-rail" />
        </div>
        <div ref={footerRef}>
          <AppFooter className="border-t border-border" />
        </div>
      </div>
      {paletteOpen ? (
        <CommandPalette actions={paletteActions} isAdmin={isAdmin} onClose={() => setPaletteOpen(false)} recents={recents} />
      ) : null}
    </div>
  );
}

// Publishes the footer's rendered height as --iw-footer-h, so viewport-height
// workspaces (Finance, Assets) can leave room for it instead of pushing it
// below the fold. The footer wraps on narrow screens, hence measuring.
function useFooterHeightVariable() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = ref.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const root = document.documentElement;
    const observer = new ResizeObserver(() => {
      root.style.setProperty("--iw-footer-h", `${element.offsetHeight}px`);
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
      root.style.removeProperty("--iw-footer-h");
    };
  }, []);
  return ref;
}

function NotFoundPage() {
  return (
    <>
      <PageHeader eyebrow="Not found" title="Page not found">
        This route is not part of the current ISK Works foundation.
      </PageHeader>
      <Panel>
        <EmptyState title="Use the application navigation" action={<ButtonLink to="/characters">Back to Characters</ButtonLink>}>
          Use the primary navigation to return to the operational workspace.
        </EmptyState>
      </Panel>
    </>
  );
}

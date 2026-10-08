import {
  BarChart2,
  Boxes,
  Building2,
  CalendarDays,
  CircleHelp,
  Factory,
  Globe2,
  Kanban,
  Landmark,
  Shield,
  Store,
  Tags,
  Users,
  Warehouse,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, Navigate, Route, Routes, useLocation, useNavigate, useParams } from "react-router";

import { getSession, logout } from "./api/auth";
import mascotHead from "./assets/mascot-head.png";
import { ApiError, createWorkspace, getWorkspaceState, type WorkspaceStateResponse } from "./api/workspace";
import { identifyViewer, trackEvent } from "./observability/logrocket";
import { CharacterName } from "./observability/private";
import { AppFooter } from "./components/app-footer";
import { HELP_PATH, LEGAL_PATH } from "./components/community-links";
import { EsiDowntimeBadge } from "./components/esi-downtime-badge";
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
import { BoardPage } from "./features/board/board-page";
import { CalendarPage } from "./features/calendar/calendar-page";
import { FacilitiesPage } from "./features/industry/facilities/facilities-page";
import { FinanceWorkspace } from "./features/finance/finance-workspace";
import { PriceSourceDetailPage } from "./features/industry/prices/price-source-detail-page";
import {
  NewPriceSourcePage,
  PricesPage,
} from "./features/industry/prices/prices-page";
import { MarketImportsPage } from "./features/industry/market/market-imports-page";
import { MarketBrowserPage } from "./features/industry/market-browser/market-browser-page";
import { InventoryPage } from "./features/industry/inventory/inventory-page";
import { OpportunitiesPage } from "./features/industry/opportunities/opportunities-page";
import { PlanetaryPage } from "./features/industry/planetary/planetary-page";
import { AssetsWorkspacePage } from "./features/assets/assets-workspace-page";
import { CharactersPage } from "./features/characters/characters-page";
import { EveCallbackPage } from "./features/esi/eve-callback-page";
import { HelpPage } from "./features/help/help-page";
import { LegalPage } from "./features/legal/legal-page";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string; code: string }
  | { status: "ready"; data: WorkspaceStateResponse };

// Characters is the intentional alpha home surface (`/` redirects here);
// it leads the nav. Board stays first-class but is a work-management
// surface, not Home. Overview (static placeholder) and Sales (unbuilt) are
// deliberately absent for the alpha.
const navItems = [
  { label: "Characters", path: "/characters", icon: Users },
  { label: "Builds", path: "/builds", icon: Factory },
  { label: "Board", path: "/board", icon: Kanban },
  { label: "Calendar", path: "/calendar", icon: CalendarDays },
  { label: "Inventory", path: "/inventory", icon: Boxes },
  { label: "Assets", path: "/assets", icon: Warehouse },
  { label: "Market", path: "/market", icon: Store },
  { label: "Price Overrides", path: "/prices", icon: Tags },
  { label: "Facilities", path: "/facilities", icon: Building2 },
  { label: "Planetary", path: "/planetary", icon: Globe2 },
  { label: "Finance", path: "/finance", icon: Landmark },
  { label: "Opportunities", path: "/opportunities", icon: BarChart2 },
];

const adminNavItem = { label: "Admin", path: "/admin", icon: Shield };

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
        <Route path="/opportunities" element={<OpportunitiesPage />} />
        <Route path="/builds" element={<BuildsPage />} />
        <Route path="/builds/new" element={<CreateBuildPage />} />
        <Route
          path="/builds/:rootBuildId/producers/:producerBuildId"
          element={<BuildWorkspacePage />}
        />
        <Route path="/builds/:buildId" element={<BuildWorkspacePage />} />
        <Route path="/builds/:buildId/edit" element={<BuildWorkspacePage canonicalize />} />
        <Route path="/orders/:orderId" element={<OrderDeepLinkRedirect />} />
        <Route path="/board" element={<BoardPage />} />
        <Route path="/calendar" element={<CalendarPage />} />
        <Route path="/inventory" element={<InventoryPage />} />
        <Route path="/assets" element={<AssetsWorkspacePage />} />
        <Route path="/market" element={<MarketBrowserPage />} />
        <Route path="/prices" element={<PricesPage />} />
        <Route path="/prices/imports" element={<MarketImportsPage />} />
        <Route path="/prices/new" element={<NewPriceSourcePage />} />
        <Route path="/prices/:priceSourceId" element={<PriceSourceDetailPage />} />
        <Route path="/facilities" element={<FacilitiesPage />} />
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
  const location = useLocation();
  const [loggingOut, setLoggingOut] = useState(false);
  const footerRef = useFooterHeightVariable();

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

  return (
    <div className="flex min-h-[var(--iw-viewport-h)] flex-col text-foreground">
      <header className="flex min-h-11 items-center gap-3 border-b border-border bg-panel px-2">
        <Link className="flex shrink-0 items-center gap-2 pr-2" to="/characters">
          <img className="h-7 w-7" src={mascotHead} alt="" aria-hidden="true" />
          <span className="hidden sm:block">
            <strong className="block text-xs leading-tight">ISK Works</strong>
            <small className="block max-w-32 truncate text-[10px] text-muted">{workspaceName}</small>
          </span>
        </Link>
        <nav className="flex min-w-0 flex-1 items-stretch gap-0.5 overflow-x-auto" aria-label="Primary navigation">
          {(isAdmin ? [...navItems, adminNavItem] : navItems).map((item) => {
            const Icon = item.icon;
            const active = item.path === "/" ? location.pathname === "/" : location.pathname.startsWith(item.path);
            return (
              <Link
                key={item.path}
                className={`flex min-h-10 shrink-0 items-center gap-1.5 border-b-2 px-2 text-xs focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary ${
                  active
                    ? "border-primary bg-panel-strong text-foreground"
                    : "border-transparent text-muted hover:bg-panel-strong hover:text-foreground"
                }`}
                to={item.path}
                aria-current={active ? "page" : undefined}
              >
                <Icon className="h-3.5 w-3.5 text-primary" aria-hidden="true" />
                <span>{item.label}</span>
              </Link>
            );
          })}
        </nav>
        <EsiDowntimeBadge />
        <Link
          aria-label="Help"
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted hover:bg-panel-strong hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
          title="Help"
          to={HELP_PATH}
        >
          <CircleHelp className="h-4 w-4" aria-hidden="true" />
        </Link>
        <span className="hidden shrink-0 font-mono text-[10px] text-muted sm:block" title={`API ${apiVersion}`}>
          {apiVersion}
        </span>
        {characterName ? (
          <div className="flex shrink-0 items-center gap-2 pl-2">
            <CharacterName className="hidden text-xs text-muted sm:block" name={characterName} />

            <button
              className="iw-button-secondary shrink-0 px-2 py-1 text-xs"
              disabled={loggingOut}
              onClick={signOut}
              type="button"
            >
              {loggingOut ? "Signing out..." : "Sign out"}
            </button>
          </div>
        ) : null}
      </header>

      <div className="iw-app-layout flex-1 lg:grid">
        <main className="iw-page-container min-w-0">{children}</main>
        <aside data-testid="app-right-rail" id="app-right-rail" />
      </div>
      <div ref={footerRef}>
        <AppFooter className="border-t border-border" />
      </div>
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

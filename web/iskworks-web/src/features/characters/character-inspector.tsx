import { X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { getCharacter, syncCharacter, type CharacterDetail } from "../../api/characters";
import { beginAuthorization, disconnectConnection, getConnection, type ConnectedCharacter } from "../../api/esi";
import { StatusDot, textToneClasses } from "../../components/primitives";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { formatIskSummary } from "../../components/money";
import { CharacterName } from "../../observability/private";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { apiMessage, formatDate } from "../esi/shared";
import {
  formatRelativeAge,
  healthMeta,
  industryCategoryStats,
  missingScopeFrom,
  sourceKindLabel,
  sourceStateMeta,
} from "./characters-formatters";
import { CharacterIndustryTab } from "./character-industry-tab";
import { CharacterSkillQueueTab } from "./skill-queue-tab";
import { LiveTrainingProgress } from "./live-training-progress";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; detail: CharacterDetail };

type TokenDetailsState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; connection: ConnectedCharacter };

type Tab = "overview" | "skills" | "industry" | "sync";

function TabStrip({ active, onSelect }: { active: Tab; onSelect: (tab: Tab) => void }) {
  // Product-direction order is Overview · Skills · Industry · Blueprints ·
  // Assets · Wallet · Sync; only the tabs that exist today are rendered.
  const tabs: { id: Tab; label: string }[] = [
    { id: "overview", label: "Overview" },
    { id: "skills", label: "Skills" },
    { id: "industry", label: "Industry" },
    { id: "sync", label: "Sync" },
  ];
  return (
    <div className="flex flex-shrink-0 border-b border-border" role="tablist">
      {tabs.map((tab) => (
        <button
          aria-selected={active === tab.id}
          className={`flex-shrink-0 border-b-2 px-3 py-1.5 text-[10px] font-medium transition ${
            active === tab.id ? "border-primary text-foreground" : "border-transparent text-muted"
          }`}
          key={tab.id}
          onClick={() => onSelect(tab.id)}
          role="tab"
          type="button"
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

// Structure/spacing follows the design prototype (character detail drawer,
// overview tab): a card is a plain bordered box filled
// with the page's own background (recessed relative to the panel behind
// it), an uppercase tracked-out label with no trailing rule, and label/value
// rows so tight they're a single pixel of vertical padding.
function OverviewCard({ children, title }: { children: ReactNode; title: string }) {
  return (
    <div className="border border-border bg-background px-3 py-2.5">
      <div className="mb-1.5 text-[9px] font-semibold uppercase tracking-wider text-muted">{title}</div>
      {children}
    </div>
  );
}

function OverviewRow({
  label,
  value,
  valueClassName = "text-foreground",
}: {
  label: string;
  value: ReactNode;
  valueClassName?: string;
}) {
  return (
    <div className="flex justify-between py-px text-[10px]">
      <span className="text-muted">{label}</span>
      <span className={valueClassName}>{value}</span>
    </div>
  );
}

const actionRowClass = "w-full border border-border bg-background px-3 py-1.5 text-left text-[10px] text-muted";

export function CharacterInspector({
  connectionId,
  onClose,
  onChanged,
}: {
  connectionId: string;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [tab, setTab] = useState<Tab>("overview");
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState("");
  const [tokenDetails, setTokenDetails] = useState<TokenDetailsState>({ status: "idle" });

  function load() {
    getCharacter(connectionId)
      .then((detail) => setState({ status: "ready", detail }))
      .catch((error: unknown) => setState({ status: "error", message: apiMessage(error) }));
  }

  useEffect(() => {
    setState({ status: "loading" });
    setTokenDetails({ status: "idle" });
    load();
    // `load` only closes over `connectionId`; reset and refetch when a different character is inspected.
  }, [connectionId]);

  // The Industry and Skills tabs derive job/training progress and remaining
  // time from fixed ESI timestamps against this clock; a coarse 30s tick
  // keeps them moving without any refetch or per-second churn, and only
  // while one of those tabs is open.
  useEffect(() => {
    if (tab !== "industry" && tab !== "skills") return undefined;
    setNowMs(Date.now());
    const interval = window.setInterval(() => setNowMs(Date.now()), 30_000);
    return () => window.clearInterval(interval);
  }, [tab, connectionId]);

  async function syncNow() {
    setBusy(true);
    setActionError("");
    try {
      await syncCharacter(connectionId);
      load();
      onChanged();
    } catch (error) {
      setActionError(apiMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function reconnect() {
    setBusy(true);
    setActionError("");
    try {
      const started = await beginAuthorization();
      if (started.fixtureMode) {
        load();
        onChanged();
      } else {
        window.location.assign(started.authorizationUrl);
      }
    } catch (error) {
      setActionError(apiMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function disconnect() {
    setBusy(true);
    setActionError("");
    try {
      await disconnectConnection(connectionId);
      onChanged();
      onClose();
    } catch (error) {
      setActionError(apiMessage(error));
      setBusy(false);
    }
  }

  function toggleTokenDetails() {
    if (tokenDetails.status !== "idle") {
      setTokenDetails({ status: "idle" });
      return;
    }
    setTokenDetails({ status: "loading" });
    getConnection(connectionId)
      .then((connection) => setTokenDetails({ status: "ready", connection }))
      .catch((error: unknown) => setTokenDetails({ status: "error", message: apiMessage(error) }));
  }

  const detail = state.status === "ready" ? state.detail : null;
  const health = detail ? healthMeta[detail.health] : null;
  const needsReconnect = detail
    ? detail.connectionStatus !== "connected" ||
      detail.sources.some((source) => missingScopeFrom(source.lastError) !== null)
    : false;
  const industryStats = detail ? industryCategoryStats(detail) : [];

  return (
    <PlannerInspectorShell hideDefaultHeader onClose={onClose} open title="Character details" width="wide">
      {state.status === "loading" ? <p className="p-4 text-xs text-muted">Loading character…</p> : null}
      {state.status === "error" ? <p className="p-4 text-xs text-danger">{state.message}</p> : null}
      {detail && health ? (
        <>
          <div className="flex flex-shrink-0 items-center gap-3 border-b border-border px-4 py-3">
            <EveCharacterPortrait characterId={detail.eveCharacterId} characterName={detail.characterName} size={40} />
            <div className="min-w-0 flex-1">
              <CharacterName className="block truncate text-[13px] font-semibold text-foreground" name={detail.characterName} />
              <p className="truncate text-[10px] text-muted">{detail.corporationName ?? "Unknown corporation"}</p>
            </div>
            <button
              aria-label="Close character inspector"
              className="shrink-0 text-muted transition hover:text-foreground"
              onClick={onClose}
              type="button"
            >
              <X aria-hidden="true" className="h-3.5 w-3.5" />
            </button>
          </div>

          <TabStrip active={tab} onSelect={setTab} />
          <div className="flex-1 overflow-y-auto p-4">
            {actionError ? (
              <p className="mb-3 border border-danger/50 bg-danger/10 p-2 text-xs text-danger">{actionError}</p>
            ) : null}

            {tab === "overview" ? (
              <div className="space-y-3">
                <OverviewCard title="Location">
                  <OverviewRow label="System" value={detail.solarSystemName ?? (detail.solarSystemId ?? "Unknown")} />
                  <OverviewRow label="Corporation" value={detail.corporationName ?? "Unknown"} />
                </OverviewCard>

                <OverviewCard title="Character">
                  <OverviewRow
                    label="Wallet"
                    value={detail.walletBalance !== null ? formatIskSummary(detail.walletBalance) : "Not synced"}
                    valueClassName="font-mono text-foreground"
                  />
                  <OverviewRow
                    label="Skill Points"
                    value={detail.totalSp !== null ? detail.totalSp.toLocaleString() : "Not synced"}
                    valueClassName="font-mono text-foreground"
                  />
                  <OverviewRow label="Security Status" value={detail.securityStatus ?? "Unknown"} valueClassName="font-mono text-foreground" />
                </OverviewCard>

                <OverviewCard title="Training">
                  <LiveTrainingProgress
                    onNeedsRefresh={load}
                    observedAt={detail.trainingObservedAt}
                    queue={detail.trainingQueue}
                    variant="inspector"
                  />
                </OverviewCard>

                <OverviewCard title="Industry jobs">
                  {industryStats.some((stat) => stat.max !== null) ? (
                    <div className="flex gap-4 text-[10px]">
                      {industryStats.map((stat) => (
                        <div key={stat.key}>
                          <span className={textToneClasses[stat.tone]}>{stat.label}</span>
                          <br />
                          <span className="font-mono text-[11px] text-foreground">
                            {stat.active ?? 0}
                            <span className="text-muted">/{stat.max ?? "—"}</span>
                          </span>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="text-[10px] text-muted">Not synced</p>
                  )}
                </OverviewCard>

                <OverviewCard title="Sync">
                  <div className="flex items-center gap-2">
                    <StatusDot tone={health.tone} />
                    <span className={`text-[10px] ${textToneClasses[health.tone]}`}>
                      {health.label} · {formatRelativeAge(detail.lastSyncedAt)}
                    </span>
                  </div>
                </OverviewCard>
              </div>
            ) : tab === "skills" ? (
              <CharacterSkillQueueTab detail={detail} nowMs={nowMs} />
            ) : tab === "industry" ? (
              <CharacterIndustryTab detail={detail} nowMs={nowMs} />
            ) : (
              <div>
                <div className="mb-3 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <StatusDot tone={health.tone} />
                    <span className="text-[11px] font-semibold text-foreground">
                      {health.label} · {formatRelativeAge(detail.lastSyncedAt)}
                    </span>
                  </div>
                  <button
                    aria-busy={busy}
                    className="border border-border bg-background px-2 py-1 text-[9px] text-muted disabled:opacity-50"
                    disabled={busy}
                    onClick={() => void syncNow()}
                    type="button"
                  >
                    {busy ? "Syncing…" : "Sync now"}
                  </button>
                </div>

                <div className="mb-1.5 text-[9px] font-semibold uppercase tracking-wider text-muted">Data sources</div>
                <table className="w-full border-collapse text-[10px]">
                  <thead>
                    <tr className="border-b border-border">
                      <th className="py-1 text-left text-[9px] font-semibold uppercase tracking-wider text-muted">Source</th>
                      <th className="py-1 text-left text-[9px] font-semibold uppercase tracking-wider text-muted">Last sync</th>
                      <th className="py-1 text-right text-[9px] font-semibold uppercase tracking-wider text-muted">State</th>
                    </tr>
                  </thead>
                  <tbody>
                    {detail.sources.map((source) => {
                      const sourceState = sourceStateMeta[source.refreshState];
                      const missingScope = missingScopeFrom(source.lastError);
                      const note = missingScope ? `Missing: ${missingScope}` : source.lastError;
                      return (
                        <tr className="border-b border-border/40" key={source.sourceKind}>
                          <td className="py-1 text-foreground">
                            {sourceKindLabel[source.sourceKind]}
                            {note ? (
                              <p className={`text-[9px] ${missingScope ? "text-warning" : "text-danger"}`}>{note}</p>
                            ) : null}
                          </td>
                          <td className="py-1 font-mono text-muted">
                            {source.observedAt ? formatRelativeAge(source.observedAt) : "Never"}
                          </td>
                          <td className={`py-1 text-right ${textToneClasses[sourceState.tone]}`}>{sourceState.label}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>

                <div className="mt-3 space-y-1">
                  <button className={actionRowClass} disabled={busy} onClick={() => void reconnect()} type="button">
                    {needsReconnect ? "Reconnect" : "Refresh permissions"}
                  </button>
                  <button className={actionRowClass} onClick={toggleTokenDetails} type="button">
                    View token details
                  </button>
                  {tokenDetails.status !== "idle" ? (
                    <OverviewCard title="Token details">
                      {tokenDetails.status === "loading" ? <p className="text-[10px] text-muted">Loading…</p> : null}
                      {tokenDetails.status === "error" ? (
                        <p className="text-[10px] text-danger">{tokenDetails.message}</p>
                      ) : null}
                      {tokenDetails.status === "ready" ? (
                        <>
                          <OverviewRow
                            label="Character ID"
                            value={tokenDetails.connection.eveCharacterId}
                            valueClassName="font-mono text-foreground"
                          />
                          <OverviewRow
                            label="Token refreshed"
                            value={formatDate(tokenDetails.connection.lastRefreshedAt)}
                            valueClassName="font-mono text-foreground"
                          />
                          <OverviewRow
                            label="Token expires"
                            value={formatDate(tokenDetails.connection.accessTokenExpiresAt)}
                            valueClassName="font-mono text-foreground"
                          />
                          <div className="pt-1.5">
                            <p className="mb-1 text-[9px] font-semibold uppercase tracking-wider text-muted">Granted scopes</p>
                            <ul className="space-y-0.5">
                              {tokenDetails.connection.grantedScopes.map((scope) => (
                                <li className="truncate font-mono text-[10px] text-foreground" key={scope}>
                                  {scope}
                                </li>
                              ))}
                            </ul>
                          </div>
                        </>
                      ) : null}
                    </OverviewCard>
                  ) : null}
                  <button
                    className={`${actionRowClass} text-danger`}
                    disabled={busy}
                    onClick={() => void disconnect()}
                    type="button"
                  >
                    Disconnect character
                  </button>
                </div>
              </div>
            )}
          </div>
        </>
      ) : null}
    </PlannerInspectorShell>
  );
}

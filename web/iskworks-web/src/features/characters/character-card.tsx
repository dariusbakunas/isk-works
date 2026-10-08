import { useCallback } from "react";

import type { CharacterRosterEntry } from "../../api/characters";
import { StatusDot, textToneClasses } from "../../components/primitives";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { formatIskCompact, formatIskSummary } from "../../components/money";
import { CharacterName } from "../../observability/private";
import { formatRelativeAge, healthBorderClass, healthMeta, industryCategoryStats, securityStatusTone } from "./characters-formatters";
import { LiveTrainingProgress } from "./live-training-progress";

export function CharacterCard({
  entry,
  onOpen,
  onTrainingNeedsRefresh,
}: {
  entry: CharacterRosterEntry;
  onOpen?: (connectionId: string) => void;
  onTrainingNeedsRefresh?: (connectionId: string) => void;
}) {
  const health = healthMeta[entry.health];
  const securityStatus = entry.securityStatus !== null ? Number(entry.securityStatus) : null;
  const handleTrainingNeedsRefresh = useCallback(
    () => onTrainingNeedsRefresh?.(entry.connectionId),
    [onTrainingNeedsRefresh, entry.connectionId],
  );
  const activeCategories = industryCategoryStats(entry).filter((stat) => (stat.active ?? 0) > 0);

  return (
    <div
      className={`flex flex-col gap-0 border border-l-[3px] bg-panel px-2.5 py-2 ${healthBorderClass[entry.health]} ${onOpen ? "cursor-pointer" : ""}`}
      onClick={onOpen ? () => onOpen(entry.connectionId) : undefined}
      role={onOpen ? "button" : undefined}
      tabIndex={onOpen ? 0 : undefined}
    >
      <div className="flex items-start gap-2">
        <EveCharacterPortrait characterId={entry.eveCharacterId} characterName={entry.characterName} size={32} />
        <div className="min-w-0 flex-1">
          <CharacterName className="block truncate text-[11px] font-semibold text-foreground" name={entry.characterName} />
          <p className="truncate text-[9px] text-muted">{entry.corporationName ?? "Unknown corporation"}</p>
          {entry.solarSystemName ? <p className="truncate text-[9px] text-muted">{entry.solarSystemName}</p> : null}
        </div>
      </div>

      <div className="mt-1.5 flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-[10px]">
        {entry.walletBalance !== null ? (
          <span className="font-mono font-medium text-foreground" title={formatIskSummary(entry.walletBalance)}>
            {formatIskCompact(entry.walletBalance)}
          </span>
        ) : null}
        {entry.walletBalance !== null && entry.totalSp !== null ? <span className="text-border">·</span> : null}
        {entry.totalSp !== null ? <span className="text-muted">{(entry.totalSp / 1_000_000).toFixed(1)}m SP</span> : null}
        {securityStatus !== null ? (
          <>
            <span className="text-border">·</span>
            <span className={`font-mono ${textToneClasses[securityStatusTone(securityStatus)]}`}>
              {securityStatus >= 0 ? "+" : ""}
              {securityStatus.toFixed(1)}
            </span>
          </>
        ) : null}
      </div>

      <LiveTrainingProgress
        onNeedsRefresh={handleTrainingNeedsRefresh}
        observedAt={entry.trainingObservedAt}
        queue={entry.trainingQueue}
        variant="card"
      />

      <div className="mt-1.5 flex items-center justify-between gap-2">
        {activeCategories.length > 0 ? (
          <div className="flex items-center gap-2 text-[9px]">
            {activeCategories.map((stat) => (
              <span key={stat.key}>
                <span className={textToneClasses[stat.tone]}>{stat.shortLabel}</span>{" "}
                <span className="text-muted">
                  {stat.active}/{stat.max ?? 10}
                </span>
              </span>
            ))}
          </div>
        ) : (
          <span className="text-[9px] text-muted">No active jobs</span>
        )}
        <div className="flex shrink-0 items-center gap-1">
          <StatusDot tone={health.tone} />
          <span className={`whitespace-nowrap text-[9px] ${textToneClasses[health.tone]}`}>
            {health.label}
            {entry.lastSyncedAt !== null ? ` · ${formatRelativeAge(entry.lastSyncedAt)}` : ""}
          </span>
        </div>
      </div>
    </div>
  );
}

import { AlertTriangle, Clock, ExternalLink, MoreHorizontal, Trash2 } from "lucide-react";
import { Link, useNavigate } from "react-router";

import type { Build } from "../../../../api/industry";
import { DropdownMenu } from "../../../../components/dropdown-menu";
import { formatDate, formatRelativeAge } from "../../shared/formatting";
import { BlueprintThumbnail } from "./blueprint-thumbnail";

function outputName(build: Build): string {
  return (
    build.recipe.products[0]?.typeName ??
    (build.recipe.kind === "manufacturing"
      ? build.recipe.blueprintName
      : build.recipe.reactionFormulaName)
  );
}

function contextLine(build: Build): string | null {
  const parts = [build.productCategoryName, build.productGroupName].filter(
    (part): part is string => Boolean(part),
  );
  return parts.length > 0 ? parts.join(" · ") : null;
}

export function BuildLibraryCard({
  build,
  onDelete,
}: {
  build: Build;
  onDelete: (build: Build) => void;
}) {
  const navigate = useNavigate();
  const output = outputName(build);
  const context = contextLine(build);
  const runLabel = `${build.runs.toLocaleString()} ${build.runs === 1 ? "run" : "runs"}`;
  // Reactions have no blueprint concept, so "not on hand" only means
  // something for a manufacturing Build.
  const blueprintMissing = build.recipe.kind === "manufacturing" && !build.hasOwnedBlueprint;

  return (
    <div className="iw-panel relative flex min-w-0 gap-3 p-3 transition hover:border-primary/60">
      <Link
        // Uses the product name, not the user's build label, so the
        // accessible name is not free-form user text captured in replay.
        aria-label={`Open ${output} build`}
        className="absolute inset-0 rounded-lg focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
        to={`/builds/${build.id}`}
      />
      <div className="pointer-events-none relative shrink-0">
        <BlueprintThumbnail build={build} size={48} />
        {blueprintMissing ? (
          <span
            className="pointer-events-auto absolute -left-1.5 -top-1.5 grid h-4 w-4 place-items-center rounded-full border border-warning/50 bg-panel text-warning"
            title="Blueprint not available"
          >
            <AlertTriangle aria-hidden="true" className="h-2.5 w-2.5" />
            <span className="sr-only">Blueprint not available</span>
          </span>
        ) : null}
      </div>
      <div className="pointer-events-none relative min-w-0 flex-1">
        <div className="truncate pr-6 text-sm font-semibold text-foreground" data-private="">
          {build.name}
        </div>
        <div className="truncate text-xs text-muted" title={output}>
          {output}
        </div>
        {context ? (
          <div className="mt-0.5 truncate text-[11px] text-muted/80" title={context}>
            {context}
          </div>
        ) : null}
        <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-muted">
          <span className="whitespace-nowrap">{runLabel}</span>
          <span
            className="inline-flex items-center gap-1 whitespace-nowrap"
            title={`Updated ${formatDate(build.updatedAt)}`}
          >
            <Clock aria-hidden="true" className="h-3 w-3 shrink-0" />
            {formatRelativeAge(build.updatedAt)}
          </span>
        </div>
      </div>
      <div className="relative shrink-0">
        <DropdownMenu
          align="end"
          icon={<MoreHorizontal aria-hidden="true" className="h-4 w-4" />}
          items={[
            {
              key: "open",
              label: "Open build",
              icon: <ExternalLink aria-hidden="true" className="h-3.5 w-3.5" />,
              onSelect: () => navigate(`/builds/${build.id}`),
            },
            {
              key: "delete",
              label: "Delete build",
              icon: <Trash2 aria-hidden="true" className="h-3.5 w-3.5" />,
              onSelect: () => onDelete(build),
            },
          ]}
          label={`${build.name} actions`}
          variant="icon"
        />
      </div>
    </div>
  );
}

import { Plus, Search, X } from "lucide-react";
import { Link } from "react-router";

import { DropdownMenu } from "../../../../components/dropdown-menu";
import {
  BLUEPRINT_OPTIONS,
  SORT_OPTIONS,
  type BlueprintFilter,
  type BuildLibraryCriteria,
  type BuildSort,
} from "../build-library-filters";

export function BuildLibraryToolbar({
  criteria,
  categories,
  matchCount,
  totalCount,
  filtersActive,
  onChange,
  onClear,
}: {
  criteria: BuildLibraryCriteria;
  categories: string[];
  matchCount: number;
  totalCount: number;
  filtersActive: boolean;
  onChange: (patch: Partial<BuildLibraryCriteria>) => void;
  onClear: () => void;
}) {
  const countLabel =
    matchCount === totalCount
      ? `${totalCount} ${totalCount === 1 ? "build" : "builds"}`
      : `${matchCount} of ${totalCount}`;

  return (
    <div className="mb-4 flex flex-wrap items-center gap-2">
      <label className="relative flex min-w-48 flex-1 items-center sm:max-w-xs">
        <Search aria-hidden="true" className="pointer-events-none absolute left-2 h-3.5 w-3.5 text-muted" />
        <input
          aria-label="Search builds"
          className="iw-input pl-7 pr-7"
          onChange={(event) => onChange({ search: event.target.value })}
          placeholder="Filter builds..."
          type="search"
          value={criteria.search}
        />
        {criteria.search ? (
          <button
            aria-label="Clear search"
            className="absolute right-2 text-muted hover:text-foreground"
            onClick={() => onChange({ search: "" })}
            type="button"
          >
            <X aria-hidden="true" className="h-3.5 w-3.5" />
          </button>
        ) : null}
      </label>

      <DropdownMenu
        align="start"
        items={categories.map((name) => ({
          key: name,
          label: name,
          onSelect: () => onChange({ category: name }),
        }))}
        label={criteria.category === "All" ? "Category" : criteria.category}
      />

      <DropdownMenu
        align="start"
        items={BLUEPRINT_OPTIONS.map((option: BlueprintFilter) => ({
          key: option,
          label: option === "On hand" ? "On hand" : "All builds",
          onSelect: () => onChange({ blueprint: option }),
        }))}
        label={criteria.blueprint === "All" ? "Blueprint" : "Blueprint: On hand"}
      />

      <DropdownMenu
        align="start"
        items={SORT_OPTIONS.map((sort: BuildSort) => ({
          key: sort,
          label: sort,
          onSelect: () => onChange({ sort }),
        }))}
        label={`Sort: ${criteria.sort}`}
      />

      <span className="text-xs text-muted" data-testid="build-library-count">
        {countLabel}
      </span>

      {filtersActive ? (
        <button
          className="inline-flex items-center gap-1 text-xs font-semibold text-primary hover:underline"
          onClick={onClear}
          type="button"
        >
          <X aria-hidden="true" className="h-3.5 w-3.5" />
          Clear filters
        </button>
      ) : null}

      <Link className="iw-button-primary ml-auto" to="/builds/new">
        <Plus aria-hidden="true" className="mr-1.5 h-4 w-4" />
        New Build
      </Link>
    </div>
  );
}

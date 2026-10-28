import { ChevronDown, Copy } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link } from "react-router";

import { listCharacters, type CharacterRosterEntry } from "../../api/characters";
import {
  getTypeInfo,
  getTypeUsedIn,
  getTypeUsedWith,
  getTypeVariations,
  type MetaGroupList,
  type TypeInfo,
  type UsedInPage,
  type UsedWithView,
} from "../../api/types";
import { ApiError } from "../../api/workspace";
import { formatIskCompact, formatIskSummary } from "../../components/money";
import type { ToolWindowRenderProps } from "../../components/tool-windows/tool-windows-context";
import { useToolWindows } from "../../components/tool-windows/tool-windows-context";
import {
  TYPE_INFO_KIND,
  currentEntry,
  newTypeInfoState,
  rememberLoaded,
  showType,
  withPilot,
  withTab,
  type TypeInfoWindowState,
} from "../../components/type-info/type-info-state";
import { TypeIcon } from "./meta-tag";
import type { OpenType } from "./rich-text";
import {
  AttributesSection,
  DescriptionSection,
  IndustrySection,
  LoadingRows,
  MetaGroupLists,
  NotInStaticData,
  RequirementsSection,
  TraitsSection,
  hasBasics,
} from "./type-info-sections";

export type TypeInfoTab = "Description" | "Attributes" | "Traits" | "Requirements" | "Variations" | "Used with" | "Industry";

/** Below this width only three tabs fit; the rest go under "More" (spec §9.1). */
const NARROW_WIDTH = 420;
const FIRST_USED_IN_PAGE = 8;
const USED_IN_PAGE = 50;
const SHIP_CATEGORY = 6;

/** The tabs a type has content for, in the client's order. */
export function visibleTabs(info: TypeInfo): TypeInfoTab[] {
  const tabs: [TypeInfoTab, boolean][] = [
    ["Description", info.description.length > 0],
    ["Attributes", info.attributes.length > 0 || hasBasics(info)],
    ["Traits", info.traits.length > 0],
    ["Requirements", info.requirements.length > 0],
    ["Variations", info.variationCount > 1],
    ["Used with", info.usedWith !== null && info.usedWith.count > 0],
    [
      "Industry",
      info.industry.producedBy.length > 0 || info.industry.reprocessesInto.length > 0 || info.industry.usedInCount > 0,
    ],
  ];
  return tabs.filter(([, shown]) => shown).map(([tab]) => tab);
}

type Load<T> = { status: "loading" } | { status: "ready"; value: T } | { status: "missing" } | { status: "failed"; message: string };

function failure(reason: unknown): Load<never> {
  if (reason instanceof ApiError && reason.status === 404) return { status: "missing" };
  return { status: "failed", message: reason instanceof Error ? reason.message : "Couldn't load this item." };
}

/** Loads `fetcher` whenever `key` changes; `null` key loads nothing. */
function useLoad<T>(key: string | null, fetcher: (signal: AbortSignal) => Promise<T>): Load<T> | null {
  const [load, setLoad] = useState<Load<T> | null>(null);
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  useEffect(() => {
    if (key === null) {
      setLoad(null);
      return;
    }
    const controller = new AbortController();
    setLoad({ status: "loading" });
    fetcherRef
      .current(controller.signal)
      .then((value) => setLoad({ status: "ready", value }))
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) setLoad(failure(reason));
      });
    return () => controller.abort();
  }, [key]);
  return load;
}

function Tabs({
  tabs,
  active,
  narrow,
  onTab,
}: {
  tabs: TypeInfoTab[];
  active: TypeInfoTab;
  narrow: boolean;
  onTab: (tab: TypeInfoTab) => void;
}) {
  const [moreOpen, setMoreOpen] = useState(false);
  const shown = narrow ? tabs.slice(0, 3) : tabs;
  const hidden = narrow ? tabs.slice(3) : [];
  const tabClass = (selected: boolean) =>
    `h-8 shrink-0 whitespace-nowrap border-b-2 px-2 text-[9px] focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary ${
      selected ? "border-primary text-foreground" : "border-transparent text-subtle hover:text-foreground"
    }`;
  return (
    <div className="relative flex h-8 shrink-0 items-end border-b border-border px-2" role="tablist">
      {shown.map((tab) => (
        <button aria-selected={tab === active} className={tabClass(tab === active)} key={tab} onClick={() => onTab(tab)} role="tab" type="button">
          {tab}
        </button>
      ))}
      {hidden.length ? (
        <>
          <button
            aria-expanded={moreOpen}
            aria-haspopup="menu"
            className={`${tabClass(hidden.includes(active))} flex items-center gap-1`}
            onClick={() => setMoreOpen((open) => !open)}
            type="button"
          >
            {hidden.includes(active) ? active : "More"}
            <ChevronDown aria-hidden="true" className="h-2.5 w-2.5" />
          </button>
          {moreOpen ? (
            <div aria-label="More tabs" className="absolute right-2 top-8 z-20 w-[130px] border border-border bg-[#151b23] py-1 shadow-xl" role="menu">
              {hidden.map((tab) => (
                <button
                  className={`h-7 w-full px-2 text-left text-[9px] hover:bg-[#1a222c] ${tab === active ? "text-bright" : "text-foreground"}`}
                  key={tab}
                  onClick={() => {
                    setMoreOpen(false);
                    onTab(tab);
                  }}
                  role="menuitem"
                  type="button"
                >
                  {tab}
                </button>
              ))}
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}

const actionClass = "flex h-6 items-center border border-border px-2 text-[8px] text-muted hover:bg-hover hover:text-foreground";

/** New build of this item from its blueprint (or else its reaction formula); `null` when nothing makes it. */
export function planBuildPath(info: TypeInfo): string | null {
  const { producedBy } = info.industry;
  const producer = producedBy.find((entry) => entry.kind === "manufacturing") ?? producedBy[0];
  if (!producer) return null;
  return producer.kind === "manufacturing"
    ? `/builds/new?blueprintTypeId=${producer.typeId}`
    : `/builds/new?reactionFormulaTypeId=${producer.typeId}`;
}

function Header({ info }: { info: TypeInfo }) {
  const [copied, setCopied] = useState(false);
  const classification = [info.group?.name, info.category?.name].filter(Boolean).join(" · ");
  const ship = info.category?.id === SHIP_CATEGORY;
  const plan = planBuildPath(info);
  return (
    <div className="flex shrink-0 gap-3 border-b border-border p-3">
      <TypeIcon metaGroupId={info.metaGroup?.id} name={info.name} size={64} typeId={info.typeId} variation={ship ? "render" : "icon"} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1">
          <h3 className="truncate text-sm font-semibold text-bright">{info.name}</h3>
          <button
            aria-label={copied ? "Name copied" : `Copy name: ${info.name}`}
            className="grid h-6 w-6 shrink-0 place-items-center text-muted hover:bg-[#1a222c] hover:text-foreground"
            onClick={() => {
              void navigator.clipboard?.writeText(info.name).then(() => setCopied(true));
            }}
            title={copied ? "Copied" : "Copy name"}
            type="button"
          >
            <Copy aria-hidden="true" className="h-[11px] w-[11px]" />
          </button>
        </div>
        {classification ? <p className="text-[9px] text-subtle">{classification}</p> : null}
        {info.price ? (
          <p className="mt-1 font-mono text-[10px] text-foreground" title={formatIskSummary(info.price.bestSell)}>
            {formatIskCompact(info.price.bestSell)} ISK
          </p>
        ) : null}
        {info.published || plan ? (
          <div className="mt-2 flex gap-1">
            {info.published ? (
              <Link className={actionClass} to={`/market?${new URLSearchParams({ typeId: String(info.typeId), name: info.name })}`}>
                Open in Market
              </Link>
            ) : null}
            {plan ? (
              <Link className={actionClass} to={plan}>
                Plan a build
              </Link>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** The Type Info window's content (spec §5.2): header, tabs, the tab's section. */
export function TypeInfoWindow({ state, setState, width }: ToolWindowRenderProps<TypeInfoWindowState>) {
  const windows = useToolWindows();
  const entry = currentEntry(state);
  const pilot = state.pilot ?? "allV";
  const latest = useRef(state);
  latest.current = state;

  const info = useLoad(`${entry.typeId}:${pilot}`, (signal) => getTypeInfo(entry.typeId, pilot, signal));
  const loaded = info?.status === "ready" ? info.value : null;

  // Name the window (and its dock entry) once the type has loaded.
  useEffect(() => {
    if (!loaded) return;
    const next = rememberLoaded(latest.current, { typeId: loaded.typeId, name: loaded.name, metaGroupId: loaded.metaGroup?.id ?? null });
    if (next !== latest.current) setState(next);
  }, [loaded, setState]);

  const tabs = loaded ? visibleTabs(loaded) : [];
  const active = (tabs.find((tab) => tab === state.tab) ?? tabs[0]) as TypeInfoTab | undefined;

  const variations = useLoad<MetaGroupList[]>(active === "Variations" ? `v${entry.typeId}` : null, (signal) =>
    getTypeVariations(entry.typeId, signal),
  );
  const usedWith = useLoad<UsedWithView>(active === "Used with" ? `w${entry.typeId}` : null, (signal) =>
    getTypeUsedWith(entry.typeId, signal),
  );
  const firstUsedIn = useLoad<UsedInPage>(
    active === "Industry" && loaded && loaded.industry.usedInCount > 0 ? `i${entry.typeId}` : null,
    (signal) => getTypeUsedIn(entry.typeId, { limit: FIRST_USED_IN_PAGE }, signal),
  );
  const [usedIn, setUsedIn] = useState<UsedInPage | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);
  useEffect(() => setUsedIn(firstUsedIn?.status === "ready" ? firstUsedIn.value : null), [firstUsedIn]);

  const characters = useLoad<CharacterRosterEntry[]>(active === "Requirements" ? "characters" : null, () => listCharacters());

  const onOpen: OpenType = (typeId, name, newWindow) => {
    if (newWindow) windows?.open(TYPE_INFO_KIND, newTypeInfoState(typeId, name), { newWindow: true });
    else setState(showType(latest.current, typeId, name));
  };

  async function loadMore() {
    if (!usedIn || usedIn.nextOffset === null) return;
    setLoadingMore(true);
    try {
      const page = await getTypeUsedIn(entry.typeId, { limit: USED_IN_PAGE, offset: usedIn.items.length });
      setUsedIn((current) => (current ? { ...page, items: [...current.items, ...page.items] } : page));
    } finally {
      setLoadingMore(false);
    }
  }

  if (!info || info.status === "loading") return <LoadingRows />;
  if (info.status === "missing") return <NotInStaticData />;
  if (info.status === "failed") return <p className="p-4 text-[10px] text-warning">{info.message}</p>;
  const value = info.value;

  let section: ReactNode = <p className="p-4 text-[10px] text-subtle">Nothing more is known about this item.</p>;
  switch (active) {
    case "Description":
      section = <DescriptionSection info={value} onOpen={onOpen} />;
      break;
    case "Attributes":
      section = <AttributesSection info={value} onOpen={onOpen} />;
      break;
    case "Traits":
      section = <TraitsSection info={value} onOpen={onOpen} />;
      break;
    case "Requirements":
      section = (
        <RequirementsSection
          characters={characters?.status === "ready" ? characters.value : []}
          info={value}
          onOpen={onOpen}
          onPilot={(next) => setState(withPilot(latest.current, next))}
          pilot={pilot}
        />
      );
      break;
    case "Variations":
      section =
        variations?.status === "ready" ? (
          <MetaGroupLists currentTypeId={value.typeId} groups={variations.value} onOpen={onOpen} />
        ) : (
          <LoadingRows />
        );
      break;
    case "Used with":
      section = usedWith?.status === "ready" ? <MetaGroupLists groups={usedWith.value.groups} onOpen={onOpen} /> : <LoadingRows />;
      break;
    case "Industry":
      section = <IndustrySection info={value} loadingMore={loadingMore} onMore={() => void loadMore()} onOpen={onOpen} usedIn={usedIn} />;
      break;
  }

  return (
    <div className="flex min-h-full flex-col">
      <Header info={value} />
      {tabs.length && active ? (
        <Tabs active={active} narrow={width < NARROW_WIDTH} onTab={(tab) => setState(withTab(latest.current, tab))} tabs={tabs} />
      ) : null}
      <div className="min-h-0 flex-1" role="tabpanel">
        {section}
      </div>
    </div>
  );
}

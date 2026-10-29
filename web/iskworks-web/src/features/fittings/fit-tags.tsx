import { Plus, X } from "lucide-react";
import { useId, useState, type KeyboardEvent } from "react";

import { setFitTags } from "../../api/fittings";
import { PRIVATE_ATTR } from "../../observability/private";

const sameTag = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** The workspace's tags a fit doesn't already hold, by name. */
export function tagSuggestions(all: string[], held: string[]): string[] {
  return all.filter((tag) => !held.some((own) => sameTag(own, tag)));
}

/** Every tag on `fits`, each once (first casing seen), by name. */
export function workspaceTags(fits: Array<{ tags: string[] }>): string[] {
  const tags: string[] = [];
  for (const tag of fits.flatMap((fit) => fit.tags)) {
    if (!tags.some((held) => sameTag(held, tag))) tags.push(tag);
  }
  return tags.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
}

/** A fit's tags as small chips; with `onRemove`, each has a remove button. */
export function TagChips({ tags, onRemove, small = false }: { tags: string[]; onRemove?: (tag: string) => void; small?: boolean }) {
  if (tags.length === 0) return null;
  return (
    <ul aria-label="Tags" className="flex flex-wrap items-center gap-1">
      {tags.map((tag) => (
        <li
          {...PRIVATE_ATTR}
          className={`flex items-center gap-0.5 rounded-[2px] border border-border bg-panel-strong text-muted ${
            small ? "px-1 text-[0.5625rem]" : "px-1.5 py-px text-[0.625rem]"
          }`}
          key={tag}
        >
          {tag}
          {onRemove ? (
            <button aria-label={`Remove tag ${tag}`} className="text-subtle hover:text-foreground" onClick={() => onRemove(tag)} type="button">
              <X aria-hidden="true" className="h-2.5 w-2.5" />
            </button>
          ) : null}
        </li>
      ))}
    </ul>
  );
}

/** A tag field offering `suggestions`: Enter submits, Escape cancels. */
export function TagInput({
  label,
  suggestions,
  onSubmit,
  onCancel,
  placeholder = "Tag",
  autoFocus = true,
}: {
  label: string;
  suggestions: string[];
  onSubmit: (tag: string) => void;
  onCancel?: () => void;
  placeholder?: string;
  autoFocus?: boolean;
}) {
  const listId = useId();
  const [value, setValue] = useState("");
  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      const tag = value.trim().replace(/\s+/g, " ");
      if (!tag) return;
      onSubmit(tag);
      setValue("");
    } else if (event.key === "Escape") {
      event.stopPropagation();
      onCancel?.();
    }
  };
  return (
    <>
      <input
        aria-label={label}
        autoFocus={autoFocus}
        className="iw-input h-6 w-36 text-[0.6875rem]"
        list={listId}
        maxLength={40}
        onBlur={() => {
          if (!value.trim()) onCancel?.();
        }}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        role="combobox"
        value={value}
      />
      <datalist id={listId}>
        {suggestions.map((tag) => (
          <option key={tag} value={tag} />
        ))}
      </datalist>
    </>
  );
}

/** A fit's tags in its header: each change saves at once, editing or not. */
export function FitTagsEditor({
  fitId,
  initial,
  suggestions,
  onChanged,
}: {
  fitId: string;
  initial: string[];
  /** The workspace's tags. */
  suggestions: string[];
  onChanged: () => void;
}) {
  const [tags, setTags] = useState(initial);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async (next: string[]) => {
    setError(null);
    try {
      setTags(await setFitTags(fitId, next));
      onChanged();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Couldn't save the tags.");
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-1">
      <TagChips onRemove={(tag) => void save(tags.filter((held) => held !== tag))} tags={tags} />
      {adding ? (
        <TagInput
          label="New tag"
          onCancel={() => setAdding(false)}
          onSubmit={(tag) => {
            setAdding(false);
            if (!tags.some((held) => sameTag(held, tag))) void save([...tags, tag]);
          }}
          suggestions={tagSuggestions(suggestions, tags)}
        />
      ) : (
        <button
          aria-label="Add tag"
          className="flex items-center gap-0.5 rounded-[2px] border border-dashed border-border px-1.5 py-px text-[0.625rem] text-muted hover:text-foreground"
          onClick={() => setAdding(true)}
          type="button"
        >
          <Plus aria-hidden="true" className="h-2.5 w-2.5" />
          Tag
        </button>
      )}
      {error ? <span className="text-[0.625rem] text-danger" role="alert">{error}</span> : null}
    </div>
  );
}

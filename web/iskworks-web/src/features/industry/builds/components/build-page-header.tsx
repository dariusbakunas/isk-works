import { Check, Pencil, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { EveTypeImage } from "../../../../components/eve-type-image";
import type { EveTypeImageVariation } from "../../../../components/eve-image-url";

export function BuildPageHeader({
  typeName,
  typeId,
  variation,
  children,
  name,
  onNameChange,
}: {
  typeName: string;
  typeId: number;
  variation: EveTypeImageVariation;
  children: ReactNode;
  name: string;
  onNameChange?: (value: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draftName, setDraftName] = useState(name);
  useEffect(() => setDraftName(name), [name]);
  const cancel = () => {
    setDraftName(name);
    setEditing(false);
  };
  const save = () => {
    const nextName = draftName.trim();
    if (!nextName || !onNameChange) return;
    onNameChange(nextName);
    setEditing(false);
  };

  return (
    <header className="mb-3">
      <div className="flex min-w-0 items-center gap-3">
        <EveTypeImage
          size={64}
          typeId={typeId}
          typeName={typeName}
          variation={variation}
        />
        <div className="min-w-0 flex-1">
          {editing ? (
            <div className="flex min-w-0 max-w-xl items-center gap-1">
              <input
                aria-label="Build name"
                autoFocus
                className="iw-input min-w-0 flex-1"
                onChange={(event) => setDraftName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") save();
                  if (event.key === "Escape") cancel();
                }}
                value={draftName}
              />
              <button aria-label="Save Build name" className="iw-icon-button" onClick={save} title="Save name" type="button">
                <Check className="h-4 w-4" aria-hidden="true" />
              </button>
              <button aria-label="Cancel Build name edit" className="iw-icon-button" onClick={cancel} title="Cancel" type="button">
                <X className="h-4 w-4" aria-hidden="true" />
              </button>
            </div>
          ) : (
            <div className="flex min-w-0 items-center gap-1">
              <h1 className="iw-title truncate" title={name}>{name}</h1>
              {onNameChange ? (
                <button
                  aria-label="Edit Build name"
                  className="iw-icon-button h-7 w-7 shrink-0 border-0"
                  onClick={() => setEditing(true)}
                  title="Edit Build name"
                  type="button"
                >
                  <Pencil className="h-3.5 w-3.5" aria-hidden="true" />
                </button>
              ) : null}
            </div>
          )}
          <p className="iw-muted mt-1 max-w-3xl">{children}</p>
        </div>
      </div>
    </header>
  );
}

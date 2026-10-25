import { ClipboardPaste, X } from "lucide-react";
import { useEffect, useId, useState, type ReactNode } from "react";

import {
  createSkillPlan,
  importPlanText,
  previewPlanText,
  type PlanTextPreview,
  type SkillPlanDetail,
} from "../../api/skills";
import { InlineAlert } from "../../components/primitives";
import { skillLevelNumeral } from "../characters/characters-formatters";
import { errorMessage } from "./skill-plans";

const REASONS = {
  unknownSkill: "no matching skill",
  invalidLevel: "level must be 1-5",
  missingLevel: "no level at the end of the line",
} as const;

function Dialog({
  children,
  footer,
  onClose,
  title,
  width = "max-w-md",
}: {
  children: ReactNode;
  footer: ReactNode;
  onClose: () => void;
  title: string;
  width?: string;
}) {
  const titleId = useId();
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className={`iw-dialog iw-dialog-frame w-full ${width} p-4`} role="dialog">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-base font-semibold" id={titleId}>{title}</h2>
          <button aria-label="Close" className="text-muted hover:text-foreground" onClick={onClose} type="button">
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>
        <div className="iw-dialog-body mt-3 space-y-3">{children}</div>
        <div className="iw-dialog-footer">{footer}</div>
      </section>
    </div>
  );
}

export function NewPlanDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (detail: SkillPlanDetail) => void }) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function create() {
    setSaving(true);
    setError(null);
    try {
      onCreated(await createSkillPlan({ name: name.trim(), description: description.trim(), entries: [] }));
    } catch (reason) {
      setError(errorMessage(reason, "Could not create the plan."));
      setSaving(false);
    }
  }

  return (
    <Dialog
      footer={
        <>
          <button className="iw-button-secondary" onClick={onClose} type="button">Cancel</button>
          <button className="iw-button-primary" disabled={saving || name.trim() === ""} onClick={() => void create()} type="button">
            Create plan
          </button>
        </>
      }
      onClose={onClose}
      title="New skill plan"
    >
      <label className="block text-xs">
        <span className="mb-1 block text-muted">Name</span>
        <input autoFocus className="iw-input w-full" onChange={(event) => setName(event.target.value)} value={name} />
      </label>
      <label className="block text-xs">
        <span className="mb-1 block text-muted">Description</span>
        <textarea className="iw-input min-h-16 w-full" onChange={(event) => setDescription(event.target.value)} value={description} />
      </label>
      {error ? <InlineAlert title="Plan not created">{error}</InlineAlert> : null}
    </Dialog>
  );
}

/** Import from the client's plan text, with a live server preview. */
export function ImportPlanDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (detail: SkillPlanDetail) => void }) {
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<PlanTextPreview | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (text.trim() === "") {
      setPreview(null);
      return;
    }
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      previewPlanText(text, controller.signal)
        .then((result) => {
          if (!controller.signal.aborted) setPreview(result);
        })
        .catch(() => undefined);
    }, 250);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [text]);

  async function create() {
    setSaving(true);
    setError(null);
    try {
      onCreated(await importPlanText({ name: name.trim(), description: "", text }));
    } catch (reason) {
      setError(errorMessage(reason, "Could not import the plan."));
      setSaving(false);
    }
  }

  const importable = preview !== null && preview.error === null && preview.entries.length > 0;
  const insertedNames = (preview?.inserted ?? []).map((entry) => {
    const view = preview?.entries.find((candidate) => candidate.skillTypeId === entry.skillTypeId);
    return `${view?.name ?? `Skill ${entry.skillTypeId}`} ${skillLevelNumeral(entry.level)}`;
  });

  return (
    <Dialog
      footer={
        <>
          <span className="mr-auto text-xs text-muted">
            {preview ? `${preview.entries.length} valid · ${preview.unrecognized.length} skipped` : "Paste a plan copied from the game"}
          </span>
          <button className="iw-button-secondary" onClick={onClose} type="button">Cancel</button>
          <button className="iw-button-primary" disabled={saving || !importable || name.trim() === ""} onClick={() => void create()} type="button">
            Create plan
          </button>
        </>
      }
      onClose={onClose}
      title="Import plan from clipboard"
      width="max-w-xl"
    >
      <label className="block text-xs">
        <span className="mb-1 block text-muted">Name</span>
        <input className="iw-input w-full" onChange={(event) => setName(event.target.value)} value={name} />
      </label>
      <label className="block text-xs">
        <span className="mb-1 flex items-center gap-1 text-muted">
          <ClipboardPaste aria-hidden="true" className="h-3 w-3" />
          Plan text
        </span>
        <textarea
          className="iw-input min-h-32 w-full font-mono text-xs"
          onChange={(event) => setText(event.target.value)}
          placeholder={"Mechanics 1\nHull Upgrades 1\n…"}
          value={text}
        />
      </label>
      {preview?.error ? <InlineAlert title="This plan can't be imported as it is">{preview.error}</InlineAlert> : null}
      {preview && preview.entries.length > 0 && !preview.error ? (
        <div>
          <p className="iw-eyebrow mb-1">Recognized skills ({preview.entries.length})</p>
          <ol className="max-h-48 divide-y divide-border overflow-y-auto border border-border">
            {preview.entries.map((entry, index) => (
              <li className="flex justify-between px-2 py-1 text-xs" key={`${entry.skillTypeId}:${entry.level}:${index}`}>
                <span>{entry.name}</span>
                <span className="font-mono text-primary">{skillLevelNumeral(entry.level)}</span>
              </li>
            ))}
          </ol>
        </div>
      ) : null}
      {insertedNames.length > 0 ? (
        <p className="text-xs text-muted">
          {insertedNames.length === 1 ? "1 prerequisite will" : `${insertedNames.length} prerequisites will`} be added
          automatically: {insertedNames.join(", ")}
        </p>
      ) : null}
      {preview && preview.unrecognized.length > 0 ? (
        <div className="rounded-md border border-warning/60 bg-warning/10 px-3 py-2 text-xs" role="status">
          <strong className="block text-warning">
            {preview.unrecognized.length} unrecognized line{preview.unrecognized.length === 1 ? "" : "s"}
          </strong>
          {preview.unrecognized.map((line) => (
            <span className="block font-mono" key={line.line}>
              Line {line.line}: '{line.text}' — {REASONS[line.reason]}
            </span>
          ))}
        </div>
      ) : null}
      {error ? <InlineAlert title="Plan not imported">{error}</InlineAlert> : null}
    </Dialog>
  );
}

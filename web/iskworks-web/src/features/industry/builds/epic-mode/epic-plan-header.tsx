import { useState } from "react";
import { listCharacters, type CharacterRosterEntry } from "../../../../api/characters";
import { listBuilds, listOrders, type Build, type EpicPlanOverlay, type OrderSummary } from "../../../../api/industry";
import { InlineAlert } from "../../../../components/primitives";
import { TicketEditor } from "../../../board/ticket-editor";
import { apiMessage } from "../../shared/api-error";

interface EditorData {
  orders: OrderSummary[];
  characters: CharacterRosterEntry[];
  builds: Build[];
}

/**
 * Above the Plan when an Epic is selected: which Epic, that the view is
 * read-only, whether the Build changed since the freeze, and a Create
 * ticket that starts with this Epic chosen.
 */
export function EpicPlanHeader({
  epic,
  buildRevision,
  onTicketCreated,
}: {
  epic: EpicPlanOverlay;
  buildRevision: number | null;
  onTicketCreated: () => void;
}) {
  const [editorData, setEditorData] = useState<EditorData | null>(null);
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState("");

  async function openEditor() {
    setOpening(true);
    setError("");
    try {
      const [orders, characters, builds] = await Promise.all([listOrders("active"), listCharacters(), listBuilds()]);
      setEditorData({ orders, characters, builds });
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setOpening(false);
    }
  }

  return (
    <div className="mb-3 space-y-2">
      <div className="flex flex-wrap items-start gap-2">
        <div className="min-w-0 flex-1">
          <InlineAlert title={`Epic: ${epic.displayName}`} tone="info">
            Read-only. The Epic&apos;s frozen plan with live reservations and ticket progress. Choose No Epic
            to edit the Build.
          </InlineAlert>
        </div>
        <button className="iw-button-secondary" disabled={opening} onClick={() => void openEditor()} type="button">
          Create ticket
        </button>
      </div>
      {buildRevision !== null && epic.sourceBuildRevision !== buildRevision ? (
        <InlineAlert title="The Build has changed since this Epic was frozen" tone="warning">
          This shows the plan as it was when the Epic was created, not the Build&apos;s current recipe, sourcing
          or runs.
        </InlineAlert>
      ) : null}
      {error ? <InlineAlert title="Ticket was not created">{error}</InlineAlert> : null}
      {editorData ? (
        <TicketEditor
          builds={editorData.builds}
          characters={editorData.characters}
          initialEpicId={epic.orderId}
          onClose={() => setEditorData(null)}
          onCreated={() => {
            setEditorData(null);
            onTicketCreated();
          }}
          orders={editorData.orders}
        />
      ) : null}
    </div>
  );
}

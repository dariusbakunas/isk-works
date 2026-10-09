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
 * Above the Plan when an Epic is selected: a Create ticket that starts with
 * this Epic chosen. (The read-only notice lives with the Epic selector, so
 * it shows on every tab.)
 */
export function EpicPlanHeader({
  epic,
  onTicketCreated,
}: {
  epic: EpicPlanOverlay;
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
      <div className="flex justify-end">
        <button className="iw-button-secondary" disabled={opening} onClick={() => void openEditor()} type="button">
          Create ticket
        </button>
      </div>
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

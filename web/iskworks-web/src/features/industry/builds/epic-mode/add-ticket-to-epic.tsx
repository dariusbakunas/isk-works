import { useState } from "react";
import { listCharacters, type CharacterRosterEntry } from "../../../../api/characters";
import { listBuilds, listOrders, type Build, type OrderSummary } from "../../../../api/industry";
import { TicketEditor } from "../../../board/ticket-editor";
import { apiMessage } from "../../shared/api-error";

interface EditorData {
  orders: OrderSummary[];
  characters: CharacterRosterEntry[];
  builds: Build[];
}

/**
 * "Add ticket to Epic": the Build toolbar's action while an Epic is
 * selected (in place of Create Epic). Opens the ticket editor -- any kind
 * of ticket -- with the Epic already chosen.
 */
export function AddTicketToEpicButton({
  epicId,
  onCreated,
}: {
  epicId: string;
  onCreated: () => void;
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
    <>
      <button
        className="iw-button-secondary"
        disabled={opening}
        onClick={() => void openEditor()}
        title={error || "Create a ticket in this Epic"}
        type="button"
      >
        {opening ? "Opening..." : "Add ticket to Epic"}
      </button>
      {error ? <span className="text-xs text-danger" role="alert">{error}</span> : null}
      {editorData ? (
        <TicketEditor
          builds={editorData.builds}
          characters={editorData.characters}
          initialEpicId={epicId}
          onClose={() => setEditorData(null)}
          onCreated={() => {
            setEditorData(null);
            onCreated();
          }}
          orders={editorData.orders}
        />
      ) : null}
    </>
  );
}

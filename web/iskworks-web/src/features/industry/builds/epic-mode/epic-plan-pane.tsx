import { useState } from "react";
import { listCharacters, type CharacterRosterEntry } from "../../../../api/characters";
import {
  createTicketForRequirement,
  listBuilds,
  listOrders,
  type Build,
  type OrderRequirement,
  type OrderSummary,
} from "../../../../api/industry";
import { InlineAlert } from "../../../../components/primitives";
import { TicketEditor } from "../../../board/ticket-editor";
import { apiMessage } from "../../shared/api-error";
import { EpicPlanView } from "./epic-plan-view";

interface EditorData {
  orders: OrderSummary[];
  characters: CharacterRosterEntry[];
  builds: Build[];
}

/** A Buy requirement nothing is tracking yet. */
function canCreateTicket(requirement: OrderRequirement): boolean {
  return requirement.kind === "buy" && requirement.state === "needsAction";
}

/**
 * The Plan tab in Epic mode, with ticket creation: tickets made here belong
 * to the selected Epic -- a Buy requirement's ticket is linked to that
 * requirement, and the Plan-level editor starts with the Epic chosen.
 */
export function EpicPlanPane({
  epicId,
  buildRevision,
  active,
}: {
  epicId: string;
  buildRevision: number;
  active: boolean;
}) {
  const [reloadKey, setReloadKey] = useState(0);
  const [creatingFor, setCreatingFor] = useState<string | null>(null);
  const [editorData, setEditorData] = useState<EditorData | null>(null);
  const [openingEditor, setOpeningEditor] = useState(false);
  const [error, setError] = useState("");

  async function createForRequirement(requirement: OrderRequirement) {
    setCreatingFor(requirement.id);
    setError("");
    try {
      await createTicketForRequirement(epicId, requirement.id);
      setReloadKey((key) => key + 1);
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setCreatingFor(null);
    }
  }

  async function openEditor() {
    setOpeningEditor(true);
    setError("");
    try {
      const [orders, characters, builds] = await Promise.all([
        listOrders("active"),
        listCharacters(),
        listBuilds(),
      ]);
      setEditorData({ orders, characters, builds });
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setOpeningEditor(false);
    }
  }

  return (
    <>
      {error ? <div className="mb-3"><InlineAlert title="Ticket was not created">{error}</InlineAlert></div> : null}
      <EpicPlanView
        actions={(requirement) => (canCreateTicket(requirement) ? (
          <button
            className="iw-button-secondary px-2 py-0.5 text-xs"
            disabled={creatingFor !== null}
            onClick={() => void createForRequirement(requirement)}
            type="button"
          >
            {creatingFor === requirement.id ? "Creating..." : "Create ticket"}
          </button>
        ) : null)}
        active={active}
        buildRevision={buildRevision}
        epicId={epicId}
        reloadKey={reloadKey}
        toolbar={(
          <button
            className="iw-button-secondary"
            disabled={openingEditor}
            onClick={() => void openEditor()}
            type="button"
          >
            Create ticket
          </button>
        )}
      />
      {editorData ? (
        <TicketEditor
          builds={editorData.builds}
          characters={editorData.characters}
          initialEpicId={epicId}
          onClose={() => setEditorData(null)}
          onCreated={() => {
            setEditorData(null);
            setReloadKey((key) => key + 1);
          }}
          orders={editorData.orders}
        />
      ) : null}
    </>
  );
}

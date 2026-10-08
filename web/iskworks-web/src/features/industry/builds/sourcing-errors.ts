import { ApiError } from "../../../api/workspace";
import { apiMessage } from "../shared/api-error";

/**
 * Sourcing and production-setup write errors in production terms. The
 * backend's codes and messages describe its internal model (producer
 * identities, plan state tokens); a user changing Buy / Build /
 * Reaction only needs to know what happened and what to do. Unknown codes
 * fall back to the generic API message.
 */
const MESSAGES: Record<string, string> = {
  revision_conflict:
    "The Build changed while you were editing it. The Plan has been refreshed -- try the change again.",
  canonical_plan_changed:
    "The Build changed while you were editing it. The Plan has been refreshed -- try the change again.",
  descendant_operation_membership_stale:
    "The Build changed while you were editing it. The Plan has been refreshed -- try the change again.",
  producer_in_use:
    "Another item in this Build still uses this production. Switch that item to Buy first.",
  production_dependency_cycle:
    "That change would make an item depend on itself, so it can't be produced here. Keep it on Buy.",
  canonical_producer_immutable:
    "This item's blueprint or formula can't be changed in place. Switch it to Buy, then choose the other method.",
  canonical_producer_ambiguous:
    "This item has more than one saved production setup. Choose which one to use, then try again.",
  canonical_graph_invalid:
    "This Build's production setup could not be updated. Reload the Build and try again.",
  canonical_write_required:
    "This Build's production setup could not be updated. Reload the Build and try again.",
};

export function sourcingErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    const message = MESSAGES[error.body.code];
    if (message) return message;
  }
  return apiMessage(error);
}


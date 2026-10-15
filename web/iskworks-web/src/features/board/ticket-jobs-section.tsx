import { X } from "lucide-react";
import { useState } from "react";

import type { IndustryJob } from "../../api/characters";
import {
  linkTicketJob,
  listTicketJobChoices,
  unlinkTicketJob,
  type TicketJobChoice,
  type TicketSummary,
} from "../../api/industry";
import { InlineAlert } from "../../components/primitives";
import { IndustryJobRow } from "../characters/character-industry-tab";
import { apiMessage } from "../industry/shared/api-error";

function SectionHeading({ children }: { children: React.ReactNode }) {
  return <h3 className="text-[10px] font-semibold uppercase tracking-wide text-muted">{children}</h3>;
}

type Choices =
  | { status: "closed" }
  | { status: "needsAssignee" }
  | { status: "loading" }
  | { status: "ready"; choices: TicketJobChoice[] };

// A production ticket's ESI industry jobs. Most link automatically; this
// lets the user fix the rare wrong match: unlink a job (automatic matching
// won't link it back), or link one of the assignee's running jobs for this
// exact item, moving it off another ticket if needed.
export function TicketJobsSection({
  ticket,
  jobs,
  nowMs,
  onChanged,
}: {
  ticket: TicketSummary;
  jobs: IndustryJob[];
  nowMs: number;
  onChanged?: () => void;
}) {
  const [choices, setChoices] = useState<Choices>({ status: "closed" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function openChoices() {
    setError("");
    if (!ticket.assigneeCharacterId) {
      setChoices({ status: "needsAssignee" });
      return;
    }
    setChoices({ status: "loading" });
    try {
      setChoices({ status: "ready", choices: await listTicketJobChoices(ticket.id) });
    } catch (requestError) {
      setChoices({ status: "closed" });
      setError(apiMessage(requestError));
    }
  }

  async function change(action: () => Promise<void>) {
    setError("");
    setBusy(true);
    try {
      await action();
      setChoices({ status: "closed" });
      onChanged?.();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <SectionHeading>Industry jobs</SectionHeading>
        <button
          className="text-[10px] font-semibold uppercase text-primary hover:underline disabled:opacity-50"
          disabled={busy || choices.status === "loading"}
          onClick={() => (choices.status === "closed" ? void openChoices() : setChoices({ status: "closed" }))}
          type="button"
        >
          {choices.status === "closed" ? "Link job" : "Done"}
        </button>
      </div>

      {error ? <InlineAlert title="Job links not changed">{error}</InlineAlert> : null}

      {jobs.length > 0 ? (
        <ul className="divide-y divide-border/70 border-y border-border/70">
          {jobs.map((job) => (
            <IndustryJobRow
              job={job}
              key={job.jobId}
              nowMs={nowMs}
              trailing={
                <button
                  aria-label="Unlink job"
                  className="shrink-0 text-muted hover:text-foreground disabled:opacity-50"
                  disabled={busy}
                  onClick={() => void change(() => unlinkTicketJob(ticket.id, job.jobId))}
                  title="Unlink job -- it won't be matched to this ticket again"
                  type="button"
                >
                  <X aria-hidden="true" className="h-3 w-3" />
                </button>
              }
            />
          ))}
        </ul>
      ) : (
        <p className="text-[11px] text-muted">No jobs linked.</p>
      )}

      {choices.status === "needsAssignee" ? (
        <p className="text-[11px] text-muted">Assign the ticket to link its jobs.</p>
      ) : null}
      {choices.status === "loading" ? (
        <p className="text-[11px] text-muted" role="status">
          Loading running jobs…
        </p>
      ) : null}
      {choices.status === "ready" ? (
        choices.choices.length === 0 ? (
          <p className="text-[11px] text-muted">No other running jobs by the assignee make this item.</p>
        ) : (
          <ul
            aria-label="Jobs to link"
            className="divide-y divide-border/70 rounded-[2px] border border-primary/40 bg-panel px-2"
          >
            {choices.choices.map((choice) => (
              <IndustryJobRow
                job={choice.job}
                key={choice.job.jobId}
                nowMs={nowMs}
                trailing={
                  <>
                    {choice.linkedTicketDisplayId ? (
                      <span className="shrink-0 font-mono text-[10px] text-warning">
                        On {choice.linkedTicketDisplayId}
                      </span>
                    ) : null}
                    <button
                      className="iw-button-secondary shrink-0 px-1.5 py-0.5 text-[10px]"
                      disabled={busy}
                      onClick={() => void change(() => linkTicketJob(ticket.id, choice.job.jobId))}
                      type="button"
                    >
                      {choice.linkedTicketId ? "Move here" : "Link"}
                    </button>
                  </>
                }
              />
            ))}
          </ul>
        )
      ) : null}
    </div>
  );
}

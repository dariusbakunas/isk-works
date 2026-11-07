import { useCallback, useEffect, useState } from "react";

import { clearMaintenanceNotice, setMaintenanceNotice } from "../../api/admin";
import { getMaintenanceNotice, type MaintenanceNotice } from "../../api/health";
import { InlineAlert, Panel } from "../../components/primitives";
import { apiMessage } from "../industry/shared/api-error";
import { Field } from "../industry/shared/field";

const DEFAULT_MESSAGE = "Please save your work before then.";
const DEFAULT_LEAD_MINUTES = 10;

/** `datetime-local` values are local wall-clock time with no timezone. */
function toLocalInputValue(date: Date) {
  const offsetMs = date.getTimezoneOffset() * 60_000;
  return new Date(date.getTime() - offsetMs).toISOString().slice(0, 16);
}

function defaultStart() {
  return toLocalInputValue(new Date(Date.now() + DEFAULT_LEAD_MINUTES * 60_000));
}

export function MaintenanceNoticePage() {
  const [current, setCurrent] = useState<MaintenanceNotice | null>(null);
  const [loadError, setLoadError] = useState("");
  const [message, setMessage] = useState(DEFAULT_MESSAGE);
  const [startsAt, setStartsAt] = useState(defaultStart);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const reload = useCallback(async () => {
    try {
      setCurrent(await getMaintenanceNotice());
      setLoadError("");
    } catch (caught) {
      setLoadError(apiMessage(caught));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError("");
    const start = new Date(startsAt);
    if (Number.isNaN(start.getTime())) {
      setError("Choose a start time.");
      return;
    }
    setSaving(true);
    try {
      setCurrent(await setMaintenanceNotice({ message, startsAt: start.toISOString() }));
    } catch (caught) {
      setError(apiMessage(caught));
    } finally {
      setSaving(false);
    }
  }

  async function clear() {
    setError("");
    setSaving(true);
    try {
      await clearMaintenanceNotice();
      setCurrent(null);
      setStartsAt(defaultStart());
    } catch (caught) {
      setError(apiMessage(caught));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="grid gap-4">
      <p className="text-sm text-muted">
        Warns every visitor, including on the sign-in screen, that maintenance is coming. Open tabs
        pick it up within a minute. It disappears at the start time, and does not take the site
        down: turn on the maintenance page separately.
      </p>
      {loadError ? <InlineAlert title="Notice unavailable">{loadError}</InlineAlert> : null}
      {current ? (
        <Panel>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="text-sm font-semibold">
                Showing now, until {new Date(current.startsAt).toLocaleString()}
              </p>
              <p className="text-sm text-muted">{current.message}</p>
            </div>
            <button
              className="iw-button-secondary"
              disabled={saving}
              onClick={() => void clear()}
              type="button"
            >
              Clear notice
            </button>
          </div>
        </Panel>
      ) : null}
      <Panel>
        <form
          className="grid gap-3 md:grid-cols-[minmax(0,1fr)_220px_auto] md:items-end"
          onSubmit={(event) => void submit(event)}
        >
          <Field label="Message" value={message} onChange={setMessage} />
          <label className="block">
            <span className="mb-1 block text-sm font-semibold">Starts at</span>
            <input
              className="iw-input"
              onChange={(event) => setStartsAt(event.target.value)}
              required
              type="datetime-local"
              value={startsAt}
            />
          </label>
          <button className="iw-button-primary" disabled={saving} type="submit">
            {saving ? "Saving..." : current ? "Update notice" : "Show notice"}
          </button>
        </form>
        {error ? (
          <div className="mt-3">
            <InlineAlert title="Notice not saved">{error}</InlineAlert>
          </div>
        ) : null}
      </Panel>
    </div>
  );
}

import { useState } from "react";

import { apiMessage } from "../features/industry/shared/api-error";

export interface AsyncActionState {
  busy: boolean;
  error: string;
  setBusy: (busy: boolean) => void;
  setError: (error: string) => void;
  run: (action: () => Promise<void>, onSettled?: () => void) => Promise<void>;
}

// The shared shape behind the repo's "busy/error" async action buttons:
// clear the error, flip busy on, run the action, and on failure surface
// `apiMessage(error)` -- always flipping busy back off and running any
// caller-supplied cleanup regardless of outcome.
export function useAsyncAction(): AsyncActionState {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function run(action: () => Promise<void>, onSettled?: () => void) {
    setError("");
    setBusy(true);
    try {
      await action();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
      onSettled?.();
    }
  }

  return { busy, error, setBusy, setError, run };
}

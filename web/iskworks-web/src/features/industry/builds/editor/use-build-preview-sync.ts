import { useEffect, useRef, useState } from "react";

import { previewCreateBuildCandidate, type CreateBuildPlanPreview } from "../../../../api/industry";
import { apiMessage } from "../../../../api/api-error";
import { automaticPricingRetryDelay, MAX_MARKET_SYNC_RETRIES } from "../planner/preview-retry";

/**
 * Keeps the worksheet's candidate preview in step with the editor: debounces
 * `previewKey` (the serialized preview request, or "" when there is nothing
 * to preview), fetches it, and re-fetches while market prices are still
 * syncing. A failed preview keeps the last worksheet that did compute.
 */
export function useBuildPreviewSync(previewKey: string, setError: (message: string) => void) {
  const [estimate, setEstimate] = useState<CreateBuildPlanPreview | null>(null);
  const [previewUpdating, setPreviewUpdating] = useState(false);
  const [previewRetryAttempt, setPreviewRetryAttempt] = useState(0);
  const [previewRetryToken, setPreviewRetryToken] = useState(0);
  const [appliedPreviewKey, setAppliedPreviewKey] = useState("");
  const previewRetry = useRef({ key: "", attempts: 0 });
  const previewSequence = useRef(0);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setAppliedPreviewKey(previewKey);
    }, 300);
    return () => window.clearTimeout(timer);
  }, [previewKey]);
  const previewPending = previewKey !== appliedPreviewKey;
  const previewSyncMessage = previewRetryAttempt > 0
    ? `Waiting for fresh market prices to sync (attempt ${previewRetryAttempt} of ${MAX_MARKET_SYNC_RETRIES})...`
    : null;

  useEffect(() => {
    if (!appliedPreviewKey) {
      setEstimate(null);
      return;
    }
    const sequence = ++previewSequence.current;
    const controller = new AbortController();
    if (previewRetry.current.key !== appliedPreviewKey) {
      previewRetry.current = { key: appliedPreviewKey, attempts: 0 };
      setPreviewRetryAttempt(0);
    }
    let retryTimer: number | undefined;
    let retryScheduled = false;
    setPreviewUpdating(true);
    const timer = window.setTimeout(() => {
      void previewCreateBuildCandidate(JSON.parse(appliedPreviewKey), controller.signal)
        .then((plan) => {
          if (sequence !== previewSequence.current) return;
          setEstimate(plan);
          setError("");
          // Every build prices from market scope first (a manual list is
          // only ever an optional fallback), so the "wait for ESI data to
          // sync" retry always applies, regardless of which PriceSource
          // is selected in the (manual-only) dropdown.
          const retryDelay = automaticPricingRetryDelay(
            "esiMarketOrders",
            plan,
            previewRetry.current.attempts,
          );
          if (retryDelay !== null) {
            retryScheduled = true;
            previewRetry.current.attempts += 1;
            setPreviewRetryAttempt(previewRetry.current.attempts);
            retryTimer = window.setTimeout(
              () => setPreviewRetryToken((current) => current + 1),
              retryDelay,
            );
          } else {
            previewRetry.current.attempts = 0;
            setPreviewRetryAttempt(0);
          }
        })
        .catch((requestError) => {
          if (controller.signal.aborted || sequence !== previewSequence.current) return;
          // A transient preview failure (market sync, a rejected edit)
          // surfaces the error but must NOT discard the last worksheet that
          // did compute -- nulling `estimate` here would unmount the whole
          // results view and the Selected Item inspector mid-edit. The
          // estimate is only cleared deliberately (`resetPreview`: product
          // switch) or when there is nothing to preview.
          setError(apiMessage(requestError));
          setPreviewRetryAttempt(0);
        })
        .finally(() => {
          if (sequence === previewSequence.current && !retryScheduled) setPreviewUpdating(false);
        });
    }, 180);
    return () => {
      window.clearTimeout(timer);
      if (retryTimer !== undefined) window.clearTimeout(retryTimer);
      controller.abort();
    };
  }, [appliedPreviewKey, previewRetryToken, setError]);

  /** Drops the current worksheet, for a switch to another product. */
  function resetPreview() {
    setEstimate(null);
    setAppliedPreviewKey("");
  }

  return { estimate, previewUpdating, previewPending, previewSyncMessage, resetPreview };
}

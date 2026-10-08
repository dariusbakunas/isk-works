import { useEffect, useId, useRef, useState, type InputHTMLAttributes } from "react";

import {
  formatMoneyInputDisplay,
  parseMoneyInput,
  type MoneyInputResult,
} from "./parse-money-input";

type PassthroughProps = Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "value" | "onChange" | "onBlur" | "onKeyDown" | "type" | "inputMode"
>;

export interface MoneyInputProps extends PassthroughProps {
  /** Last committed canonical value (`""` = unset). Drives the display. */
  value: string;
  /**
   * Fired with a canonical, unformatted string when a valid value commits
   * (debounced while typing, immediately on blur/Enter). Omit it for an
   * explicit-submit form that only wants the live classification via
   * `onValueChange` and commits on its own action.
   */
  onCommit?: (canonical: string) => void;
  /** Fired when a previously-set field is emptied and left (blur/Enter). */
  onClear?: () => void;
  /**
   * Fired synchronously on every keystroke and on blur/Enter with the full
   * classification (`status` + `canonical`/`message`). Lets an
   * explicit-submit parent track the live canonical value and gate its own
   * Save/Preview control without duplicating the parser or waiting on a
   * debounce.
   */
  onValueChange?: (result: MoneyInputResult) => void;
  /** Debounce before a valid value commits while typing (ms). */
  commitDebounceMs?: number;
  className?: string;
}

/**
 * Controlled ISK text field that owns every presentation concern -- raw
 * text, parser classification, inline validation, blur/Enter finalisation
 * and (when `onCommit` is provided) a debounced commit for valid values.
 * The parent only ever receives a canonical unformatted string via
 * `onCommit` / `onValueChange` (or an `onClear` signal); raw or invalid
 * text never crosses this boundary.
 */
export function MoneyInput({
  value,
  onCommit,
  onClear,
  onValueChange,
  commitDebounceMs = 250,
  className = "",
  ...inputProps
}: MoneyInputProps) {
  const [raw, setRaw] = useState(() => formatMoneyInputDisplay(value));
  const [message, setMessage] = useState<string | null>(null);
  const focusedRef = useRef(false);
  const commitTimerRef = useRef<number | undefined>(undefined);
  // Last value this component pushed upward, so a stale `value` prop (the
  // preview round-trip that echoes the canonical back takes ~1s) does not
  // trigger a duplicate commit.
  const lastCommittedRef = useRef(value);
  const onValueChangeRef = useRef(onValueChange);
  onValueChangeRef.current = onValueChange;
  const fallbackId = useId();
  const errorId = `${inputProps.id ?? fallbackId}-money-error`;

  useEffect(() => {
    lastCommittedRef.current = value;
    onValueChangeRef.current?.(parseMoneyInput(formatMoneyInputDisplay(value)));
    if (!focusedRef.current) {
      setRaw(formatMoneyInputDisplay(value));
      setMessage(null);
    }
  }, [value]);

  useEffect(() => () => window.clearTimeout(commitTimerRef.current), []);

  function clearCommitTimer() {
    window.clearTimeout(commitTimerRef.current);
    commitTimerRef.current = undefined;
  }

  function commit(canonical: string) {
    if (canonical === lastCommittedRef.current) return;
    lastCommittedRef.current = canonical;
    onCommit?.(canonical);
  }

  function handleChange(next: string) {
    setRaw(next);
    const result = parseMoneyInput(next);
    onValueChange?.(result);
    clearCommitTimer();

    if (result.status === "invalid") {
      setMessage(result.message ?? null);
      return;
    }
    setMessage(null);
    if (onCommit && result.status === "valid" && result.canonical !== undefined) {
      const canonical = result.canonical;
      commitTimerRef.current = window.setTimeout(() => commit(canonical), commitDebounceMs);
    }
  }

  function finalize() {
    clearCommitTimer();
    const result = parseMoneyInput(raw, { final: true });
    onValueChange?.(result);

    if (result.status === "valid" && result.canonical !== undefined) {
      setMessage(null);
      commit(result.canonical);
      setRaw(formatMoneyInputDisplay(result.canonical));
      return;
    }
    if (result.status === "empty") {
      setMessage(null);
      setRaw("");
      if (value !== "") {
        lastCommittedRef.current = "";
        onClear?.();
      }
      return;
    }
    if (result.status === "invalid") {
      setMessage(result.message ?? null);
      return;
    }
    // Incomplete and unresolvable (a lone "."): restore the committed value.
    setMessage(null);
    setRaw(formatMoneyInputDisplay(value));
  }

  return (
    <>
      <input
        {...inputProps}
        aria-describedby={message ? errorId : inputProps["aria-describedby"]}
        aria-invalid={message ? true : undefined}
        className={`iw-input font-mono ${className}`.trim()}
        inputMode="decimal"
        onBlur={() => {
          focusedRef.current = false;
          finalize();
        }}
        onChange={(event) => handleChange(event.target.value)}
        onFocus={() => {
          focusedRef.current = true;
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") finalize();
        }}
        type="text"
        value={raw}
      />
      {message ? (
        <p className="mt-1 text-xs text-destructive" id={errorId} role="alert">
          {message}
        </p>
      ) : null}
    </>
  );
}

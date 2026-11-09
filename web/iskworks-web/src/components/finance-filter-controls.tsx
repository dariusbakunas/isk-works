import { CalendarDays, Check, ChevronDown, ChevronRight } from "lucide-react";
import type { ChangeEventHandler, ReactNode } from "react";

export function FinanceFilterSection({
  children,
  id,
  meta,
  onToggle,
  open,
  title,
}: {
  children: ReactNode;
  id: string;
  meta?: ReactNode;
  onToggle: () => void;
  open: boolean;
  title: string;
}) {
  const contentId = `${id}-content`;
  const accessibleTitle = title.toLowerCase().endsWith("filters") ? title : `${title} filters`;
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <section className="space-y-1.5">
      <button
        aria-controls={contentId}
        aria-expanded={open}
        aria-label={accessibleTitle}
        className="flex w-full items-center gap-1.5 rounded-sm py-0.5 text-left text-[0.625rem] font-semibold uppercase text-muted transition hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
        onClick={onToggle}
        type="button"
      >
        <span>{title}</span>
        {meta ? <span className="ml-auto font-mono text-[0.625rem] tabular-nums">{meta}</span> : <span className="ml-auto" />}
        <Chevron aria-hidden="true" className="h-3 w-3 shrink-0" />
      </button>
      {open ? <div id={contentId}>{children}</div> : null}
    </section>
  );
}

export function FinanceCheckbox({
  checked,
  children,
  disabled = false,
  onChange,
}: {
  checked: boolean;
  children: ReactNode;
  disabled?: boolean;
  onChange: ChangeEventHandler<HTMLInputElement>;
}) {
  return (
    <label className={`group flex cursor-pointer items-start gap-2 ${disabled ? "cursor-not-allowed opacity-60" : ""}`}>
      <input
        checked={checked}
        className="peer sr-only"
        disabled={disabled}
        onChange={onChange}
        type="checkbox"
      />
      <span className="mt-0.5 grid h-3.5 w-3.5 shrink-0 place-items-center rounded-[2px] border border-border bg-background text-primary-foreground transition group-hover:border-primary peer-checked:border-primary peer-checked:bg-primary peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-primary">
        {checked ? <Check aria-hidden="true" className="h-3 w-3" strokeWidth={3} /> : null}
      </span>
      <span className="min-w-0 flex-1">{children}</span>
    </label>
  );
}

export function FinanceDateField({
  id,
  label,
  onChange,
  value,
}: {
  id: string;
  label: string;
  onChange: (value: string) => void;
  value: string;
}) {
  return (
    <label className="block text-[0.625rem] text-muted" htmlFor={id}>
      {label}
      <span className="relative mt-1 block">
        <input
          className="finance-date-field iw-input min-h-8 pr-8 font-mono text-xs tabular-nums"
          id={id}
          onChange={(event) => onChange(event.target.value)}
          type="date"
          value={value}
        />
        <CalendarDays
          aria-hidden="true"
          className="pointer-events-none absolute right-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-foreground"
          data-testid="finance-date-calendar"
        />
      </span>
    </label>
  );
}

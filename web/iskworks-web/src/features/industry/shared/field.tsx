interface FieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  autoFocus?: boolean;
  multiline?: boolean;
  inputMode?: "numeric" | "decimal";
  type?: "text" | "number";
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  className?: string;
}

export function Field({
  label,
  value,
  onChange,
  onFocus,
  onBlur,
  autoFocus = false,
  multiline = false,
  inputMode,
  type = "text",
  min,
  max,
  step,
  disabled = false,
  className = "",
}: FieldProps) {
  return (
    <label className={`block ${className}`}>
      <span className="mb-1 block text-balance text-sm font-semibold">{label}</span>
      {multiline ? (
        <textarea
          autoFocus={autoFocus}
          className="iw-input min-h-24 resize-y"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onFocus={onFocus}
          onBlur={onBlur}
        />
      ) : (
        <input
          autoFocus={autoFocus}
          className="iw-input"
          inputMode={inputMode}
          type={type}
          min={min}
          max={max}
          step={step}
          disabled={disabled}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onFocus={onFocus}
          onBlur={onBlur}
        />
      )}
    </label>
  );
}

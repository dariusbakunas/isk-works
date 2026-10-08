interface SparklineProps {
  values: number[];
  /** Any CSS colour, e.g. `var(--color-income)`. */
  color: string;
  width?: number;
  height?: number;
  className?: string;
}

/** A dependency-free trend line, min-max scaled with a 1px inset. */
export function Sparkline({ values, color, width = 64, height = 24, className = "" }: SparklineProps) {
  if (values.length < 2) return null;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min;
  const step = (width - 2) / (values.length - 1);
  const points = values
    .map((value, index) => {
      const y = span === 0 ? height / 2 : height - 1 - ((value - min) / span) * (height - 2);
      return `${(1 + index * step).toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg aria-hidden="true" className={`shrink-0 ${className}`} height={height} viewBox={`0 0 ${width} ${height}`} width={width}>
      <polyline fill="none" points={points} stroke={color} strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.5" />
    </svg>
  );
}

export function CoverageBar({ percentage, hasShortage }: { percentage: number; hasShortage: boolean }) {
  return (
    <div className="mt-1.5 flex items-center gap-2">
      <span className="h-1 flex-1 overflow-hidden bg-border">
        <span
          className={`block h-full ${hasShortage ? "bg-warning" : "bg-positive"}`}
          style={{ width: `${percentage}%` }}
        />
      </span>
      <strong className={`font-mono text-xs ${hasShortage ? "text-warning" : "text-positive"}`}>
        {percentage}%
      </strong>
    </div>
  );
}

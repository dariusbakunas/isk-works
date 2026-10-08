/** Loading placeholder that mirrors the real layout: KPI row, then chart cards. */
export function AnalyticsSkeleton() {
  const block = "animate-pulse rounded-[2px] bg-panel-strong";
  return (
    <div aria-busy="true" aria-label="Loading analytics" className="space-y-2" role="status">
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-5">
        {Array.from({ length: 5 }, (_, index) => (
          <div className="iw-panel space-y-2 px-3 py-2" key={index}>
            <div className={`${block} h-2 w-1/2`} />
            <div className={`${block} h-5 w-3/4`} />
            <div className={`${block} h-2 w-2/5`} />
          </div>
        ))}
      </div>
      <div className="iw-panel space-y-3 p-3">
        <div className={`${block} h-2.5 w-28`} />
        <div className={`${block} h-48 w-full`} />
      </div>
      {[0, 1, 2].map((row) => (
        <div className="grid gap-2 lg:grid-cols-2" key={row}>
          {[0, 1].map((column) => (
            <div className="iw-panel space-y-3 p-3" key={column}>
              <div className={`${block} h-2.5 w-28`} />
              <div className={`${block} h-32 w-full`} />
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

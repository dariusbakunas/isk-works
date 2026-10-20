/** Loading placeholder that mirrors the real layout: KPI row, Epic cards, table rows. */
export function RunwaySkeleton() {
  const block = "animate-pulse rounded-[2px] bg-panel-strong";
  return (
    <div aria-busy="true" aria-label="Loading runway" className="space-y-3" role="status">
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => (
          <div className="iw-panel space-y-2 px-3 py-2" key={index}>
            <div className={`${block} h-2 w-1/2`} />
            <div className={`${block} h-5 w-3/4`} />
            <div className={`${block} h-2 w-2/5`} />
          </div>
        ))}
      </div>
      <div className="flex gap-2">
        {Array.from({ length: 4 }, (_, index) => <div className={`${block} h-16 w-48`} key={index} />)}
      </div>
      <div className="space-y-1">
        {Array.from({ length: 10 }, (_, index) => <div className={`${block} h-6 w-full`} key={index} />)}
      </div>
    </div>
  );
}

# Where it runs

ISK Works doesn't live in a hyperscaler region with a five-nines SLA. It lives in a barn.

## The data center

- **Location:** my barn office. The server shares the building with tools, spiders, and the occasional confused bird.
- **Hardware:** a single home server with an 18-core Xeon and 128 GB of memory, running a small Kubernetes cluster in virtual machines, with the database on a VM of its own.
- **Network:** the internet reaches the barn over a Wi-Fi bridge from the house. Visitors come in through a Cloudflare tunnel, so no ports are open to the outside world.
- **Backups:** the database is dumped every night, and the disks are snapshotted daily.

It's honestly pretty robust. It's just not *redundant*.

## When the lights go out

The one thing the barn can't control is the electricity grid. When the power goes out, the server can ride out a blip, but the internet connection can't, and sooner or later ISK Works drops offline with it. Storms, ice, a squirrel with ambitions: if the grid sneezes, so does the site.

When that happens:

- **Your data is safe.** An outage stops the site; it doesn't touch what's stored. Everything is where you left it when it comes back.
- **ESI catches up.** Character sync runs again once the site is back, so wallet and asset data fill in on the next sync.
- **It's probably the weather.** Ask in the [community channel](support:) if it's been down a while.

> **Uptime guarantee:** none. **Uptime aspiration:** whenever the lights are on.

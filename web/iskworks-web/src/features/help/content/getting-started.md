# Getting started

ISK Works plans EVE Online industry with **your real costs**. Most tools price every input at today's market. ISK Works values the materials you already own at what you **actually paid** for them, and prices only what you still need to buy. Two things make that work: **facilities** and **registered inventory**. Set those up first and your build profits will be accurate.

## 1. Sign in and connect characters

Sign in with EVE Online, then connect your industry characters on **[Characters](/characters)**. ISK Works syncs skills, blueprints, industry jobs, assets, wallet transactions and planets for each one. The [privacy page](/legal) explains what each permission is used for. All access is read-only.

## 2. Set up your facilities

A Build is only as accurate as the place you build it. Create a profile on **[Facilities](/facilities)** for every station or structure you use, with its structure type, rigs, security, tax and surcharges. The system cost index is fetched from ESI for you.

A facility decides:

- **Material quantities**: structure and rig bonuses on top of blueprint ME.
- **Job time**: structure and rig time bonuses on top of blueprint TE.
- **Installation cost**: system cost index, facility tax and SCC surcharge.

If a Build has no facility, materials and time use the blueprint's ME/TE only, and there is **no installation cost**. Its cost and profit stay **Incomplete**, and the Build shows *"An operation has no facility selected"*.

## 3. Register your inventory

This is the most important step. ISK Works keeps an accounting inventory: what you own, and **what you paid for it**, as a weighted average unit cost.

### How a Build is costed

For every material a Build needs:

1. **Inventory covers what it can first.** That quantity is costed at your **average cost** from Inventory, not at today's market price.
2. **Only the shortage is priced.** Missing materials you buy are priced from the market, or from a Price Override when the market has no price. Missing materials you build are costed from their own production.
3. **Profit = expected revenue − total production cost.** It is shown only when every input is priced and every operation has a facility. Otherwise the Build shows **Incomplete**.

Your build profit therefore depends directly on your inventory cost. The same Build can be profitable with materials you bought cheaply last month, and unprofitable if you bought everything at today's prices.

### If you have no inventory

ISK Works shows this rather than guessing. On a Build's worksheet:

- **Covered** shows how much comes from inventory, and **Shortage** shows what's missing (in red).
- **Coverage** is green when inventory covers everything, amber when it covers part, and red when it covers nothing.
- **Sourcing** reads **Inventory** for rows fully covered by stock, and **Buy**, **Build** or **Reaction** for the rest.

With no inventory at all, every material is a shortage and the whole Build is priced at current market. That's a fine estimate, but it isn't what *you* would spend. To deliberately ignore stock for one material, set its scope to **Full (ignores inventory)**.

### Ways to register inventory

- **[Inventory](/inventory) → Record → Add Opening Balance**: start from what you already own, with your best estimate of its cost.
- **[Finance](/finance) → Transactions → + Inventory**: turn a market buy from your wallet into an inventory purchase. You can preview the effect on your average cost first.
- **[Inventory](/inventory) → Record → Purchase / Adjustment**: manual entries, e.g. contracts or loot.
- **Board → Record production**: finished jobs add their output to inventory and use up the inputs. The output's cost is the inputs' cost plus installation, so your own production feeds the next Build's costs.

Inventory is a ledger: entries are never edited, only reversed. **Current value** on the Inventory page compares your stock with today's market. It never changes your recorded cost.

## 4. Plan a build

Create a Build on **[Builds](/builds)**: pick the product, runs, facility and market scopes. Its main views:

- **Worksheet**: every material with Required, Covered, Shortage, Coverage, pricing and cost.
- **Plan**: what to buy and what to build, stage by stage. The **cost warnings** banner lists anything that keeps the cost incomplete, such as a missing price or facility.
- **Graph**: the whole production chain.

For each component, choose **Buy** or **Build**. A component you build becomes its own job in the same plan.

## 5. Run it on the Board

**[Board](/board)** turns plans into work: acquisition and production tickets you move through to done. Recording an acquisition or a finished job on a ticket updates your inventory. Moving a ticket alone never does.

## Where things live

- **Characters**: connected characters, wallets, skills and jobs.
- **Builds**: build plans and their costs.
- **Board**: what to do next.
- **Calendar**: job completions, skill finishes and planetary timers.
- **Inventory**: accounting inventory with historical cost.
- **Assets**: what ESI says your characters actually hold.
- **Market**: browse observed market prices.
- **Price Overrides**: manual prices used when the market has none.
- **Facilities**: your manufacturing and reaction locations.
- **Planetary**: Planetary Interaction overview.
- **Finance**: wallet transactions and analytics.
- **Opportunities**: what's profitable to build right now.

Stuck or found a bug? Ask in the [community channel](support:) or contact the operator of this instance.

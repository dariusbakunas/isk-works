//! The EVE SSO scopes ISK Works requests. A connection records the scopes
//! it granted, so readers of connections (not only the ESI client) need
//! these names; `iskworks-esi` re-exports them.

pub const ASSET_SCOPE: &str = "esi-assets.read_assets.v1";
pub const BLUEPRINT_SCOPE: &str = "esi-characters.read_blueprints.v1";
pub const STRUCTURE_SCOPE: &str = "esi-universe.read_structures.v1";
pub const WALLET_SCOPE: &str = "esi-wallet.read_character_wallet.v1";
pub const LOCATION_SCOPE: &str = "esi-location.read_location.v1";
pub const SKILLS_SCOPE: &str = "esi-skills.read_skills.v1";
pub const SKILL_QUEUE_SCOPE: &str = "esi-skills.read_skillqueue.v1";
pub const INDUSTRY_JOBS_SCOPE: &str = "esi-industry.read_character_jobs.v1";
pub const MARKET_STRUCTURE_SCOPE: &str = "esi-markets.structure_markets.v1";
/// Read access to the character's PI colonies (ESI names it "manage", but
/// the endpoints ISK Works calls are read-only).
pub const PLANETS_SCOPE: &str = "esi-planets.manage_planets.v1";
/// Read access to the character's in-game calendar (no responding).
pub const CALENDAR_SCOPE: &str = "esi-calendar.read_calendar_events.v1";
/// Read access to the character's own active and historical market orders.
pub const MARKET_ORDERS_SCOPE: &str = "esi-markets.read_character_orders.v1";

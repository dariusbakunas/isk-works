import { expect, test } from "vitest";

import * as barrel from "../industry";
import * as builds from "../industry/builds";
import * as facilities from "../industry/facilities";
import * as market from "../industry/market";

test("feature modules and compatibility barrel expose industry clients", () => {
  expect(builds.listBuilds).toBe(barrel.listBuilds);
  expect(facilities.listFacilities).toBe(barrel.listFacilities);
  expect(market.listMarketRegions).toBe(barrel.listMarketRegions);
});

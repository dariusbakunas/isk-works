import { expect, test } from "vitest";

import { apiMessage as esiMessage } from "../../features/esi/shared";
import { apiMessage as inventoryMessage } from "../../features/industry/inventory/shared";
import { apiMessage as marketMessage } from "../../features/industry/market/shared";
import { apiMessage as marketBrowserMessage } from "../../features/industry/market-browser/shared";
import { apiMessage as opportunitiesMessage } from "../../features/industry/opportunities/shared";
import { apiMessage as industryMessage } from "../../features/industry/shared/api-error";
import { ApiError } from "../workspace";

const redacted = new ApiError(503, {
  code: "persistence_unavailable",
  message: "A storage error occurred. Please try again.",
  retryable: true,
  correlationId: "corr-123",
});
const curated = new ApiError(400, { code: "validation_failed", message: "Name is required." });

const messages = {
  esi: esiMessage,
  inventory: inventoryMessage,
  market: marketMessage,
  marketBrowser: marketBrowserMessage,
  opportunities: opportunitiesMessage,
  industry: industryMessage,
};

test.each(Object.entries(messages))("%s apiMessage shows the Error ID of a redacted failure", (_, message) => {
  expect(message(redacted)).toBe("A storage error occurred. Please try again. (Error ID: corr-123)");
});

test.each(Object.entries(messages))("%s apiMessage leaves curated errors unchanged", (_, message) => {
  expect(message(curated)).toBe("Name is required.");
});

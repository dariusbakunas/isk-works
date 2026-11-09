import { describe, expect, it } from "vitest";
import { ApiError } from "../../../../api/workspace";
import { apiMessage } from "../api-error";

describe("apiMessage", () => {
  it("uses the API message for ordinary structured errors", () => {
    expect(apiMessage(new ApiError(400, {
      code: "validation_error",
      message: "Invalid request",
    }))).toBe("Invalid request");
  });

  it("gives revision conflicts actionable copy", () => {
    expect(apiMessage(new ApiError(409, {
      code: "revision_conflict",
      message: "Conflict",
    }))).toBe("This record changed after you loaded it. Reload before trying again.");
  });

  it("preserves Error messages and the unknown fallback", () => {
    expect(apiMessage(new Error("Network failed"))).toBe("Network failed");
    expect(apiMessage(undefined)).toBe("ISK Works could not complete the request.");
  });

  it("appends the Error ID when the backend redacted an internal failure", () => {
    expect(apiMessage(new ApiError(503, {
      code: "persistence_unavailable",
      message: "A storage error occurred. Please try again.",
      retryable: true,
      correlationId: "3f1a9c2e-0b7d-4a11-9e6f-8c2d1a4b5c6d",
    }))).toBe(
      "A storage error occurred. Please try again. (Error ID: 3f1a9c2e-0b7d-4a11-9e6f-8c2d1a4b5c6d)",
    );
  });

  it("does not show an Error ID for ordinary errors without a correlationId", () => {
    expect(apiMessage(new ApiError(404, {
      code: "order_not_found",
      message: "Epic was not found.",
    }))).toBe("Epic was not found.");
  });
});

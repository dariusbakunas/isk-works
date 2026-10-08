import { describe, expect, it } from "vitest";

import type { TicketStatus } from "../../../api/industry";
import { canDropTicket, isTicketDraggable } from "../ticket-drag";
import { ticketFixture } from "./fixtures";

describe("canDropTicket", () => {
  // Lane moves are direction-agnostic workflow writes, not a one-way
  // execution lifecycle. There is no state machine and no dependency gate.
  it("allows forward moves To Do -> In Progress -> Complete", () => {
    expect(canDropTicket("todo", "inProgress")).toBe(true);
    expect(canDropTicket("inProgress", "complete")).toBe(true);
  });

  it("allows backward moves Complete -> In Progress -> To Do", () => {
    expect(canDropTicket("complete", "inProgress")).toBe(true);
    expect(canDropTicket("inProgress", "todo")).toBe(true);
    expect(canDropTicket("complete", "todo")).toBe(true);
    expect(canDropTicket("todo", "complete")).toBe(true);
  });

  it("rejects a no-op move into the card's own lane", () => {
    for (const status of ["todo", "inProgress", "complete"] as TicketStatus[]) {
      expect(canDropTicket(status, status)).toBe(false);
    }
  });

  it("never involves Canceled (organizational status, not a Board lane)", () => {
    expect(canDropTicket("canceled", "todo")).toBe(false);
    expect(canDropTicket("todo", "canceled")).toBe(false);
  });
});

describe("isTicketDraggable", () => {
  it("is true for a To Do, In Progress, or Complete unbatched ticket", () => {
    expect(isTicketDraggable(ticketFixture({ status: "todo" }))).toBe(true);
    expect(isTicketDraggable(ticketFixture({ status: "inProgress" }))).toBe(true);
    expect(isTicketDraggable(ticketFixture({ status: "complete" }))).toBe(true);
  });

  it("is true for a ticket with unmet dependencies -- blockers never gate a lane move", () => {
    expect(
      isTicketDraggable(
        ticketFixture({
          status: "todo",
          blockedBy: [
            {
              prerequisiteId: "pre-1",
              kind: "buy",
              typeId: 34,
              capturedName: "Tritanium",
              outstandingQuantity: 100,
              representativeFulfillingTicketId: null,
              representativeFulfillingTicketDisplayId: null,
              representativeFulfillingTicketStatus: null,
            },
          ],
        }),
      ),
    ).toBe(true);
  });

  it("is false for a Canceled ticket -- not rendered on the Board", () => {
    expect(isTicketDraggable(ticketFixture({ status: "canceled" }))).toBe(false);
  });

  it("is false for a ticket batched into an Acquisition Run, regardless of status", () => {
    expect(isTicketDraggable(ticketFixture({ status: "todo", acquisitionRunId: "run-1" }))).toBe(false);
    expect(isTicketDraggable(ticketFixture({ status: "complete", acquisitionRunId: "run-1" }))).toBe(false);
  });
});

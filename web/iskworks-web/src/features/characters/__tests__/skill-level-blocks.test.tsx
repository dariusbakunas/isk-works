import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { SkillLevelBlocks } from "../skill-level-blocks";
import type { LevelCellState } from "../skill-queue-summary";

function states(container: HTMLElement): string[] {
  return Array.from(container.querySelectorAll("[data-level-state]")).map(
    (el) => el.getAttribute("data-level-state") ?? "",
  );
}

describe("SkillLevelBlocks", () => {
  test("renders one cell per level, carrying its semantic data-level-state", () => {
    const cells: LevelCellState[] = ["trained", "trained", "partial", "queued", "empty"];
    const { container } = render(<SkillLevelBlocks cells={cells} target={4} />);
    expect(states(container)).toEqual(["trained", "trained", "partial", "queued", "empty"]);
  });

  test("the level-indicator group does not shrink (flex-shrink: 0)", () => {
    const { getByRole } = render(
      <SkillLevelBlocks cells={["queued", "empty", "empty", "empty", "empty"]} target={1} />,
    );
    expect(getByRole("img").className).toContain("shrink-0");
  });

  test("the 'training' cell carries the breathe-animation class", () => {
    const { container } = render(
      <SkillLevelBlocks cells={["trained", "trained", "training", "empty", "empty"]} target={3} />,
    );
    const training = container.querySelector('[data-level-state="training"]') as HTMLElement;
    expect(training.className).toContain("iw-skill-cell-training");
  });

  test("aria-label names the currently-training level", () => {
    const { getByRole } = render(
      <SkillLevelBlocks cells={["trained", "trained", "training", "empty", "empty"]} target={3} />,
    );
    expect(getByRole("img").getAttribute("aria-label")).toContain("III currently training");
  });

  test("the partial cell is drawn with a CSS gradient, not a solid fill", () => {
    const { container } = render(
      <SkillLevelBlocks cells={["trained", "trained", "trained", "trained", "partial"]} target={5} />,
    );
    const partial = container.querySelector('[data-level-state="partial"]') as HTMLElement;
    expect(partial.style.backgroundImage).toContain("linear-gradient");
  });

  test("summarises every cell's state in one aria-label (colour is not the only signal)", () => {
    const { getByRole } = render(
      <SkillLevelBlocks cells={["trained", "trained", "queued", "empty", "empty"]} target={3} />,
    );
    expect(getByRole("img")).toHaveAttribute(
      "aria-label",
      "Target level III: I trained, II trained, III queued, IV beyond target, V beyond target",
    );
  });

  test("aria-label names a partially trained level", () => {
    const { getByRole } = render(
      <SkillLevelBlocks cells={["trained", "trained", "trained", "trained", "partial"]} target={5} />,
    );
    expect(getByRole("img").getAttribute("aria-label")).toContain("V partially trained");
  });
});

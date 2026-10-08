import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { PageHeader } from "../../components/primitives";

describe("EveCharacterPortrait privacy", () => {
  it("marks the portrait image data-private and omits the name from alt text", () => {
    const { getByAltText } = render(
      <EveCharacterPortrait characterId={90000001} characterName="Jita Local" size={40} />,
    );
    const img = getByAltText("Character portrait");
    expect(img).toHaveAttribute("data-private", "");
    expect(img.getAttribute("alt")).not.toContain("Jita Local");
  });
});

describe("PageHeader titlePrivate", () => {
  it("marks the title when titlePrivate is set", () => {
    const { getByRole, rerender } = render(
      <PageHeader eyebrow="Price Override" title="My Buy List" titlePrivate>
        body
      </PageHeader>,
    );
    expect(getByRole("heading", { level: 1 })).toHaveAttribute("data-private", "");

    rerender(
      <PageHeader eyebrow="Builds" title="Builds">
        body
      </PageHeader>,
    );
    expect(getByRole("heading", { level: 1 })).not.toHaveAttribute("data-private");
  });
});

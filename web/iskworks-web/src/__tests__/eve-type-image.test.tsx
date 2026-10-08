import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { EveCharacterPortrait } from "../components/eve-character-portrait";
import { eveCharacterPortraitUrl, eveTypeImageUrl } from "../components/eve-image-url";
import { EveTypeImage } from "../components/eve-type-image";

describe("EVE type images", () => {
  it("builds canonical image-service URLs with supported sizes", () => {
    expect(eveTypeImageUrl(587, "render", 256)).toBe(
      "https://images.evetech.net/types/587/render?size=256",
    );
  });

  it("uses CCP's bp variation for original blueprints", () => {
    expect(eveTypeImageUrl(691, "bp", 64)).toBe(
      "https://images.evetech.net/types/691/bp?size=64",
    );
  });

  it("uses fixed dimensions and falls back without layout shift", () => {
    render(<EveTypeImage size={40} typeId={34} typeName="Tritanium" />);
    const image = screen.getByRole("img", { name: "Tritanium" });
    expect(image).toHaveAttribute(
      "src",
      "https://images.evetech.net/types/34/icon?size=64",
    );
    expect(image).toHaveAttribute("width", "40");
    expect(image).toHaveAttribute("height", "40");

    fireEvent.error(image);
    expect(screen.getByRole("img", { name: "Tritanium image unavailable" })).toHaveStyle({
      width: "40px",
      height: "40px",
    });
  });

  it("falls back from a specialized variation to the type icon", () => {
    render(<EveTypeImage size={64} typeId={587} typeName="Rifter" variation="render" />);
    const image = screen.getByRole("img", { name: "Rifter" });
    fireEvent.error(image);
    expect(image).toHaveAttribute(
      "src",
      "https://images.evetech.net/types/587/icon?size=64",
    );
  });

  it("renders fixed-size character portraits with a fallback", () => {
    expect(eveCharacterPortraitUrl(2_119_000_001, 64)).toBe(
      "https://images.evetech.net/characters/2119000001/portrait?size=64",
    );
    render(
      <EveCharacterPortrait
        characterId={2_119_000_001}
        characterName="Fixture Industrialist"
        size={48}
      />,
    );
    // The character name is deliberately absent from the alt/aria text so
    // it is not captured in session replay (see session-replay-privacy.md).
    const portrait = screen.getByRole("img", { name: "Character portrait" });
    expect(portrait).toHaveAttribute("data-private", "");
    expect(portrait).toHaveAttribute(
      "src",
      "https://images.evetech.net/characters/2119000001/portrait?size=64",
    );
    expect(portrait).toHaveAttribute("width", "48");
    expect(portrait).toHaveAttribute("height", "48");

    fireEvent.error(portrait);
    expect(
      screen.getByRole("img", { name: "Character portrait unavailable" }),
    ).toHaveStyle({ width: "48px", height: "48px" });
  });

  it("uses the smallest supported image request for a 16px portrait", () => {
    render(<EveCharacterPortrait characterId={2_119_000_001} characterName="Fixture Industrialist" size={16} />);
    const portrait = screen.getByRole("img", { name: "Character portrait" });
    expect(portrait).toHaveAttribute("src", "https://images.evetech.net/characters/2119000001/portrait?size=32");
    expect(portrait).toHaveStyle({ width: "16px", height: "16px" });
  });
});

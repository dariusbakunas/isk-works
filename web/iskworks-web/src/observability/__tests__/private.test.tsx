import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { CharacterName, Private, PRIVATE_ATTR } from "../private";

describe("Private", () => {
  it("marks its content with data-private", () => {
    const { getByText } = render(<Private>secret note</Private>);
    expect(getByText("secret note")).toHaveAttribute("data-private", "");
  });

  it("supports hard removal and a custom element", () => {
    const { getByText } = render(
      <Private as="p" hard>
        gone
      </Private>,
    );
    const el = getByText("gone");
    expect(el.tagName).toBe("P");
    expect(el).toHaveAttribute("data-private", "delete");
  });

  it("exposes a spreadable attribute pair", () => {
    expect(PRIVATE_ATTR).toEqual({ "data-private": "" });
  });
});

describe("CharacterName", () => {
  it("renders the name inside a data-private span with no leaking title", () => {
    const { getByText } = render(<CharacterName name="Jita Local" />);
    const el = getByText("Jita Local");
    expect(el.tagName).toBe("SPAN");
    expect(el).toHaveAttribute("data-private", "");
    expect(el).not.toHaveAttribute("title");
  });
});

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { BuildEditorHeader } from "../build-editor-header";

function Harness() {
  const [runs, setRuns] = useState("1");
  return <BuildEditorHeader onRunsChange={setRuns} preview={null} runs={runs} />;
}

describe("BuildEditorHeader", () => {
  // Multi-BPC job split: runs above a copy's licensed runs plan as several
  // jobs, so the blueprint never caps or locks the run count.
  it("accepts any run count", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const field = screen.getByLabelText("Runs");
    expect(field).not.toBeDisabled();
    await user.clear(field);
    await user.type(field, "40");

    expect(field).toHaveValue(40);
  });
});

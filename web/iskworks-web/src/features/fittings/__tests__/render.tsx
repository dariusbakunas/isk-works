import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";

import { WithToolWindows } from "../../../test/tool-windows";
import { FittingsPage } from "../fittings-page";


function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}{location.search}</div>;
}

export function renderPage(path = "/fittings", { toolWindows = false } = {}) {
  const routes = (
    <Routes>
      <Route element={<><FittingsPage /><LocationProbe /></>} path="/fittings/*" />
    </Routes>
  );
  return render(
    <MemoryRouter initialEntries={[path]}>
      {toolWindows ? (
        <WithToolWindows>{routes}</WithToolWindows>
      ) : (
        routes
      )}
    </MemoryRouter>,
  );
}

export const location = () => screen.getByTestId("location").textContent ?? "";

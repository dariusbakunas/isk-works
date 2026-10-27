import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";

import { FittingsPage } from "../fittings-page";

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}{location.search}</div>;
}

export function renderPage(path = "/fittings") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<><FittingsPage /><LocationProbe /></>} path="/fittings/*" />
      </Routes>
    </MemoryRouter>,
  );
}

export const location = () => screen.getByTestId("location").textContent ?? "";

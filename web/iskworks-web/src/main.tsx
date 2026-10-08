import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router";

import { App } from "./App";
import { EnvironmentBanner } from "./components/environment-banner";
import { initLogRocket } from "./observability/logrocket";
import "./styles.css";

initLogRocket();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <EnvironmentBanner />
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>,
);

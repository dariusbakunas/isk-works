import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router";

import { installMaintenanceReload } from "./api/maintenance";
import { App } from "./App";
import { SiteBanners } from "./components/site-banners";
import { initLogRocket } from "./observability/logrocket";
import "./styles.css";

initLogRocket();
installMaintenanceReload();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SiteBanners />
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>,
);

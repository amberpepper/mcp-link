import "@mcp_link/tailwind-config/base.css";
import "./renderer/utils/i18n"; // Import i18n initialization first
import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/renderer/components/App";
import { HashRouter } from "react-router-dom";

const root = ReactDOM.createRoot(
  document.getElementById("root") as HTMLElement,
);
root.render(
  <React.StrictMode>
    <HashRouter>
      <App />
    </HashRouter>
  </React.StrictMode>,
);

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { DesktopWindow } from "./features/window/DesktopWindow";
import "./styles.css";
import "./features/window/window.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <DesktopWindow><App /></DesktopWindow>
  </StrictMode>,
);

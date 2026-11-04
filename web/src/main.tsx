import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./site.css";
import { DottieLanding } from "./landing";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <DottieLanding />
  </StrictMode>,
);

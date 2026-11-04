import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../site.css";
import "../live.css";
import { DottieApp } from "./dottie-app";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <DottieApp />
  </StrictMode>,
);

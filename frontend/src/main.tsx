import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

document.addEventListener("contextmenu", (event) => event.preventDefault());
document.addEventListener("selectstart", (event) => {
  const target = event.target;
  if (
    !(target instanceof HTMLElement) ||
    !target.closest("input, textarea, [contenteditable='true']")
  ) {
    event.preventDefault();
  }
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

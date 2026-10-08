import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import { Capsule } from "./capsule/Capsule";
import { App } from "./App";

const isCapsule = window.location.hash.startsWith("#/capsule");
if (isCapsule) document.documentElement.classList.add("capsule-window");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isCapsule ? <Capsule /> : <App />}</React.StrictMode>,
);

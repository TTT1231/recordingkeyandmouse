import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ReplayOverlay } from "./ReplayOverlay";

const isReplayOverlay = new URLSearchParams(window.location.search).get("overlay") === "replay";
if (isReplayOverlay) document.documentElement.classList.add("replay-overlay-window");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>{isReplayOverlay ? <ReplayOverlay /> : <App />}</React.StrictMode>,
);

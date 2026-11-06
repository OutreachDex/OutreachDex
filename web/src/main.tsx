import { createRoot } from "react-dom/client";
import "./app/prefs";
import { App } from "./app/App";
import "./styles/theme.css";
import "./styles/app.css";

createRoot(document.getElementById("root")!).render(<App />);

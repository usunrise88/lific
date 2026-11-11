import { mount } from "svelte";
import "./app.css";
import "./radixThemes.css";
import App from "./App.svelte";
import { init as initTheme } from "./lib/theme";
import { initLocale } from "./lib/i18n";

initTheme();

// The stored or browser language is loaded before the first render, so the
// interface never flashes English first.
const app = initLocale().then(() =>
  mount(App, {
    target: document.getElementById("app")!,
  }),
);

export default app;

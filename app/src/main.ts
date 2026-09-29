import { mount } from "svelte";
import "./app.css";
import { installRipple } from "./lib/ripple";

installRipple();

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");

// Dev only: `?mock` swaps the Rust side for src/dev/mock.ts so the UI runs in a browser. It has
// to be in place before the app's modules load, since they read the Tauri bridge as they start.
if (import.meta.env.DEV && location.search.includes("mock")) (await import("./dev/mock")).installMock();
if (import.meta.env.DEV) (await import("./dev/fps")).installFps();
const { default: App } = await import("./App.svelte");

export default mount(App, { target });

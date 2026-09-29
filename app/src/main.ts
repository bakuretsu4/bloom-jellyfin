import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { installRipple } from "./lib/ripple";

installRipple();

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");

export default mount(App, { target });

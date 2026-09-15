// Interface zoom, stepped like a browser's. Rust applies and remembers the level (zoom.rs);
// this file only turns keys and the wheel into steps and mirrors the level for the page.
import { invoke } from "@tauri-apps/api/core";

/** `flash` counts zoom changes, so a readout can show briefly after each one. */
export const view = $state({ zoom: 1, flash: 0 });

/** One step in (1) or out (-1), or back to 100% (0). */
export async function stepZoom(direction: -1 | 0 | 1) {
  try {
    view.zoom = await invoke<number>("zoom_step", { direction });
    view.flash++;
  } catch {
    // Outside Tauri there is nothing to zoom.
  }
}

function directionFor(e: KeyboardEvent): -1 | 0 | 1 | null {
  if (e.code === "Equal" || e.code === "NumpadAdd" || e.key === "+" || e.key === "=") return 1;
  if (e.code === "Minus" || e.code === "NumpadSubtract" || e.key === "-") return -1;
  if (e.code === "Digit0" || e.code === "Numpad0" || e.key === "0") return 0;
  return null;
}

/** Starts listening; returns a function that stops. */
export function installZoom(): () => void {
  invoke<number>("zoom_level")
    .then((z) => (view.zoom = z))
    .catch(() => {});

  const onKey = (e: KeyboardEvent) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const direction = directionFor(e);
    if (direction === null) return;
    e.preventDefault();
    void stepZoom(direction);
  };

  // Touchpads send a stream of tiny deltas; gather them so one gesture is not ten steps.
  let gathered = 0;
  let lastStep = 0;
  const onWheel = (e: WheelEvent) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    const delta = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY * 40 : e.deltaY;
    if (Math.sign(delta) !== Math.sign(gathered)) gathered = 0;
    gathered += delta;
    const now = performance.now();
    if (Math.abs(gathered) < 60 || now - lastStep < 120) return;
    lastStep = now;
    const direction = gathered < 0 ? 1 : -1;
    gathered = 0;
    void stepZoom(direction);
  };

  window.addEventListener("keydown", onKey);
  window.addEventListener("wheel", onWheel, { passive: false });
  return () => {
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("wheel", onWheel);
  };
}

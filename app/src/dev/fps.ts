// Dev only: a frame-rate readout in the corner, from requestAnimationFrame, which fires once
// per frame the webview actually presents. Ctrl+Shift+F hides or shows it.
export function installFps() {
  const el = document.createElement("div");
  el.style.cssText =
    "position:fixed;right:8px;bottom:8px;z-index:9999;padding:4px 8px;border-radius:6px;background:#000c;color:#fff;font:600 12px/16px monospace;pointer-events:none;white-space:pre";
  document.body.appendChild(el);
  let frames = 0;
  let worst = 0;
  let last = performance.now();
  let windowStart = last;
  const tick = (now: number) => {
    frames++;
    worst = Math.max(worst, now - last);
    last = now;
    if (now - windowStart >= 1000) {
      el.textContent = `${Math.round((frames * 1000) / (now - windowStart))} fps\nworst ${worst.toFixed(1)} ms`;
      frames = 0;
      worst = 0;
      windowStart = now;
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "f") el.hidden = !el.hidden;
  });
}

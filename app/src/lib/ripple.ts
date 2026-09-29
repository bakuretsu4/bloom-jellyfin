// Material 3 ripple: a circle grows from the pointer inside pressable controls (.btn, .iconbtn,
// .chip, or anything marked data-ripple) and fades when the press ends. One delegated listener
// for the whole app, so components don't have to opt in.
const TARGET = ".btn, .iconbtn, .chip, [data-ripple]";

export function installRipple() {
  document.addEventListener(
    "pointerdown",
    (event) => {
      if (event.button !== 0 || document.documentElement.dataset.reduceMotion === "true") return;
      const el = (event.target as Element | null)?.closest<HTMLElement>(TARGET);
      if (!el || (el as HTMLButtonElement).disabled) return;
      const rect = el.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      // Big enough to reach the farthest corner from the press.
      const radius = Math.hypot(Math.max(x, rect.width - x), Math.max(y, rect.height - y));
      const ripple = document.createElement("span");
      ripple.className = "ripple";
      ripple.style.cssText = `left:${x - radius}px;top:${y - radius}px;width:${radius * 2}px;height:${radius * 2}px`;
      el.appendChild(ripple);
      const end = () => {
        ripple.classList.add("is-done");
        ripple.addEventListener("animationend", (e) => e.animationName === "ripple-out" && ripple.remove());
        removeEventListener("pointerup", end);
        removeEventListener("pointercancel", end);
      };
      addEventListener("pointerup", end);
      addEventListener("pointercancel", end);
    },
    { passive: true },
  );
}

// The device settings for the page: read once at startup, edited by the Settings screen, and
// read by whatever they affect (the theme, the player, motion). Rust keeps them (settings.rs).
import * as api from "./api";
import { THEME_PROPERTIES, themeProperties, themeUsable } from "./theme";

export const prefs = $state<{ current: api.Settings | null }>({ current: null });

/** Theme, accent and reduced motion are attributes on the root that the CSS tokens follow. A
 *  custom theme sets the tokens on the root itself, and only if its contrast holds. */
function apply(settings: api.Settings) {
  const root = document.documentElement;
  const custom = (settings.customThemes ?? []).find((t) => t.id === settings.customTheme);
  // Off first, so a property the new theme doesn't set (a font) doesn't stay from the last one.
  for (const property of THEME_PROPERTIES) root.style.removeProperty(property);
  if (custom && themeUsable(custom)) {
    root.dataset.theme = custom.base;
    for (const [property, value] of Object.entries(themeProperties(custom))) root.style.setProperty(property, value);
    root.style.colorScheme = custom.base;
  } else {
    root.style.removeProperty("color-scheme");
    if (settings.theme === "auto") delete root.dataset.theme;
    else root.dataset.theme = settings.theme;
  }
  root.dataset.accent = settings.accent;
  if (settings.reduceMotion) root.dataset.reduceMotion = "true";
  else delete root.dataset.reduceMotion;
}

export async function loadSettings() {
  const loaded = await api.settings();
  prefs.current = loaded;
  apply(loaded);
}

/** Shows a change at once, then keeps what Rust saved, or puts things back if saving failed. */
export async function saveSettings(change: Partial<api.Settings>) {
  const before = prefs.current;
  if (!before) return;
  const next = { ...before, ...change };
  prefs.current = next;
  apply(next);
  try {
    const saved = await api.updateSettings(next);
    prefs.current = saved;
    apply(saved);
  } catch {
    prefs.current = before;
    apply(before);
  }
}

const desktopReduces = matchMedia("(prefers-reduced-motion: reduce)");

/** Either the desktop's setting or Bloom's own switch. */
export function reducedMotion(): boolean {
  return !!prefs.current?.reduceMotion || desktopReduces.matches;
}

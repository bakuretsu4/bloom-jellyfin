// Material 3 colour: a full tonal scheme worked out from one seed colour with Google's HCT
// library, written to the root as `--md-sys-color-*` custom properties. The rest of the UI
// reads those roles (m3.css maps Bloom's older tokens onto them), so changing the seed, the
// theme or a title's artwork re-colours everything at once.
import {
  Hct,
  MaterialDynamicColors,
  SchemeTonalSpot,
  argbFromHex,
  hexFromArgb,
  sourceColorFromImage,
} from "@material/material-color-utilities";

const ROLES = [
  "primary",
  "onPrimary",
  "primaryContainer",
  "onPrimaryContainer",
  "secondary",
  "onSecondary",
  "secondaryContainer",
  "onSecondaryContainer",
  "tertiary",
  "onTertiary",
  "tertiaryContainer",
  "onTertiaryContainer",
  "error",
  "onError",
  "errorContainer",
  "onErrorContainer",
  "background",
  "onBackground",
  "surface",
  "onSurface",
  "surfaceVariant",
  "onSurfaceVariant",
  "surfaceDim",
  "surfaceBright",
  "surfaceContainerLowest",
  "surfaceContainerLow",
  "surfaceContainer",
  "surfaceContainerHigh",
  "surfaceContainerHighest",
  "outline",
  "outlineVariant",
  "inverseSurface",
  "inverseOnSurface",
  "inversePrimary",
  "surfaceTint",
  "scrim",
  "shadow",
] as const;

const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

/** The scheme's colours by role, as `#rrggbb`. */
export function scheme(seed: string, dark: boolean, contrast = 0): Record<string, string> {
  const s = new SchemeTonalSpot(Hct.fromInt(argbFromHex(seed)), dark, contrast);
  const out: Record<string, string> = {};
  for (const role of ROLES) out[role] = hexFromArgb(MaterialDynamicColors[role].getArgb(s));
  return out;
}

/** Colour properties are set on `target`; the root by default. */
export function applyScheme(seed: string, dark: boolean, target: HTMLElement = document.documentElement) {
  for (const [role, hex] of Object.entries(scheme(seed, dark))) {
    target.style.setProperty(`--md-sys-color-${kebab(role)}`, hex);
  }
}

/** The dominant colour of an artwork URL, or null if it can't be read (a cross-origin image
 *  without CORS headers taints the canvas). */
export async function seedFromImage(src: string): Promise<string | null> {
  try {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.decoding = "async";
    img.src = src;
    await img.decode();
    return hexFromArgb(await sourceColorFromImage(img));
  } catch {
    return null;
  }
}

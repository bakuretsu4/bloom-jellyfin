# Custom themes

Besides Light, Dark and four accent colours, Bloom lets you make your own themes: your colours,
your font, checked so they stay readable.

## Ready-made themes

Bloom has 30 of these built in: open **Settings → Appearance → Custom themes** and pick one. It's
added to your own themes and used, so you can edit or delete it like any other. The same themes are
in [`docs/themes/`](themes/) as CSS, which you can also paste into **New theme → Edit as CSS**.

| Theme | Style | File |
|---|---|---|
| Ayu Dark | Dark | [ayu-dark.css](themes/ayu-dark.css) |
| Ayu Mirage | Dark | [ayu-mirage.css](themes/ayu-mirage.css) |
| Catppuccin Frappé | Dark | [catppuccin-frappe.css](themes/catppuccin-frappe.css) |
| Catppuccin Latte | Light | [catppuccin-latte.css](themes/catppuccin-latte.css) |
| Catppuccin Macchiato | Dark | [catppuccin-macchiato.css](themes/catppuccin-macchiato.css) |
| Catppuccin Mocha | Dark | [catppuccin-mocha.css](themes/catppuccin-mocha.css) |
| Dracula | Dark | [dracula.css](themes/dracula.css) |
| Everforest Dark | Dark | [everforest-dark.css](themes/everforest-dark.css) |
| Everforest Light | Light | [everforest-light.css](themes/everforest-light.css) |
| GitHub Dark | Dark | [github-dark.css](themes/github-dark.css) |
| GitHub Light | Light | [github-light.css](themes/github-light.css) |
| Gruvbox Dark | Dark | [gruvbox-dark.css](themes/gruvbox-dark.css) |
| Gruvbox Light | Light | [gruvbox-light.css](themes/gruvbox-light.css) |
| Horizon | Dark | [horizon.css](themes/horizon.css) |
| Kanagawa | Dark | [kanagawa.css](themes/kanagawa.css) |
| Night Owl | Dark | [night-owl.css](themes/night-owl.css) |
| Nightfox | Dark | [nightfox.css](themes/nightfox.css) |
| Nord | Dark | [nord.css](themes/nord.css) |
| One Dark | Dark | [one-dark.css](themes/one-dark.css) |
| One Light | Light | [one-light.css](themes/one-light.css) |
| Oxocarbon | Dark | [oxocarbon.css](themes/oxocarbon.css) |
| Material Palenight | Dark | [palenight.css](themes/palenight.css) |
| Poimandres | Dark | [poimandres.css](themes/poimandres.css) |
| Rosé Pine Dawn | Light | [rose-pine-dawn.css](themes/rose-pine-dawn.css) |
| Rosé Pine Moon | Dark | [rose-pine-moon.css](themes/rose-pine-moon.css) |
| Rosé Pine | Dark | [rose-pine.css](themes/rose-pine.css) |
| Solarized Dark | Dark | [solarized-dark.css](themes/solarized-dark.css) |
| Solarized Light | Light | [solarized-light.css](themes/solarized-light.css) |
| Tokyo Night | Dark | [tokyo-night.css](themes/tokyo-night.css) |
| Tomorrow Night | Dark | [tomorrow-night.css](themes/tomorrow-night.css) |

Each uses its scheme's own colours for the background, panels, text, accent and success and warning
marks. Where a scheme's dimmer text colour doesn't read at 4.5:1, Bloom's worked-out shade stands in.
Everforest Light uses the palette's blue as its accent, since its green is too pale to read on light
panels.

The palettes belong to their authors: [Catppuccin](https://catppuccin.com),
[Gruvbox](https://github.com/morhetz/gruvbox), [Nord](https://www.nordtheme.com),
[Dracula](https://draculatheme.com), [Tokyo Night](https://github.com/folke/tokyonight.nvim),
[Rosé Pine](https://rosepinetheme.com), [Everforest](https://github.com/sainnhe/everforest),
[Kanagawa](https://github.com/rebelot/kanagawa.nvim),
[One Dark](https://github.com/atom/one-dark-syntax) and
[Solarized](https://ethanschoonover.com/solarized).

## Making a theme

1. Open **Settings → Appearance → Custom themes** and choose **New theme**.
2. Give it a **name**, and pick **Dark** or **Light** as its starting point. Each starts from
   Bloom's own colours for that side.
3. Set its four colours with the colour pickers or as hex codes:
   - **Background:** the window behind everything.
   - **Panels:** cards, menus and raised areas.
   - **Text:** all text. Bloom works out the fainter shades from it.
   - **Accent:** buttons, highlights and marks.
4. Optionally pick a **font** installed on your computer. **Roboto (Bloom's)** is the default.
5. Watch the **preview** and the **checks** beside it, then choose **Save and use**.

Bloom works out every shade in between (dividers, hovered panels, secondary text) from your four
colours. The player's own controls keep their amber, so they read over any video.

Your themes appear as chips under Custom themes: click one to use it, or its gear to edit or
**Delete** it. Choosing Auto, Light or Dark leaves your custom theme.

## Readability checks

A theme can only be saved and used when all six checks pass:

| Check | Needs |
|---|---|
| Text on the background | 4.5:1 |
| Text on panels | 4.5:1 |
| Secondary text | 4.5:1 |
| Faint text | 4.5:1 |
| Accent on panels | 3:1 |
| Text on the accent | 4.5:1 |

These are contrast ratios from the WCAG accessibility guidelines. Each check shows its current
ratio, and what it needs when it fails. If one fails, move the colours further apart: usually a
darker background or lighter text on a dark theme, or the reverse on a light one.

## Editing as CSS

**Edit as CSS** shows the whole theme as CSS, which you can edit directly. For example, a dark
purple theme:

```css
:root {
  color-scheme: dark;
  --f-ui: "Inter";
  --ground: #1e1e2e;
  --surface: #313244;
  --surface-2: #3a3b4e;
  --raise: #43445a;
  --line: #5a5b6f;
  --line-soft: #45465a;
  --ink: #cdd6f4;
  --ink-2: #a3aac4;
  --ink-3: #8c92ab;
  --accent: #cba6f7;
  --accent-ink: #111111;
  --good: #5e9b72;
  --alert: #c0736b;
}
```

- `color-scheme` is `dark` or `light`.
- `--f-ui` is the font's name. Leave it out for Bloom's own font.
- The rest are Bloom's colour tokens, as hex:

  | Token | Used for |
  |---|---|
  | `--ground` | The background |
  | `--surface` | Panels |
  | `--surface-2` | Panels on panels, hovered rows |
  | `--raise` | Raised controls |
  | `--line`, `--line-soft` | Dividers and borders |
  | `--ink` | Text |
  | `--ink-2`, `--ink-3` | Secondary and faint text |
  | `--accent` | The accent |
  | `--accent-ink` | Text on the accent |
  | `--good`, `--alert` | Success and warning marks |

`--ground`, `--surface`, `--ink` and `--accent` are the four colours from the editor. Setting any of
the others by hand overrides the shade Bloom would work out; **Back to colours** shows how many you
set, with **Clear** to go back to the worked-out ones. Anything a theme can't set is listed under the
text box instead of being used.

## Sharing a theme

- **To share:** open the theme and choose **Copy CSS**, then paste it wherever you like.
- **To use someone's theme:** choose **New theme**, **Edit as CSS**, replace the text with theirs,
  name it, and **Save and use**. If they used a font you don't have, it shows as "(not installed)"
  and Bloom's own font stands in.

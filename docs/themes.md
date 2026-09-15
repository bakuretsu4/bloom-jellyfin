# Custom themes

Besides Light, Dark and four accent colours, Bloom lets you make your own themes: your colours,
your font, checked so they stay readable.

## Making a theme

1. Open **Settings → Appearance → Custom themes** and choose **New theme**.
2. Give it a **name**, and pick **Dark** or **Light** as its starting point. Each starts from
   Bloom's own colours for that side.
3. Set its four colours with the colour pickers or as hex codes:
   - **Background:** the window behind everything.
   - **Panels:** cards, menus and raised areas.
   - **Text:** all text. Bloom works out the fainter shades from it.
   - **Accent:** buttons, highlights and marks.
4. Optionally pick a **font** installed on your computer. **Archivo (Bloom's)** is the default.
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

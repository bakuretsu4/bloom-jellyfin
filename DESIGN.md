# Design: Bloom

Bloom's interface follows Material 3, drawn with [Slint](https://slint.dev). The pieces live in
`app/ui/` (the screens and the kit) and `app/src/theme.rs` (the colours).

## Colour

A full M3 tonal scheme is generated at run time with Google's HCT (the `material-colors` crate)
from one seed colour, and written into the `Theme` global (`app/ui/theme.slint`) as the M3 roles:
`primary`, `on-primary`, `primary-container`, `secondary-container`, `surface`, the five
`surface-container` steps, `outline`, and the rest. Slint's globals cannot animate, so a change of
scheme is cross-faded over 600 ms from Rust.

- The seed is the chosen accent (amber, green, blue or red) or a custom theme's accent. The scheme
  follows Light and Dark, and Auto follows the desktop, live, through the settings portal.
- Custom themes take four colours (background, panels, text, accent), a font and a base, are
  checked against six contrast rules, and can be written as CSS (`app/src/themes.rs`). Thirty
  ready-made ones are built in from `docs/themes/`.
- A title's page tints the whole window with its poster's dominant colour, faded in and out. A
  custom theme is not tinted over.
- The player's own controls keep their amber so they read over any video.

## Type

Roboto, carried inside the program (Regular, Medium and Bold), or the font a custom theme names.
Sizes follow the M3 scale: 11.5 to 14.5 px for text and facts, 22 for section titles, 32 to 48 for
page and title headings. Numbers that change (the time, the readout) keep a steady width.

## Shape and elevation

Corner radii of 4, 8, 12, 16, 28 and full. Cards and artwork use 16, the title hero 28, buttons
and chips full (or 8 for text buttons). Surfaces are told apart by tone (the surface-container
steps), not shadows; only floating things (the music player, menus) have one.

## Motion

M3 easing and durations: emphasized curves for movement (a page arriving, a menu opening, a card
lifting, a cover growing into its page), standard ones for colour and opacity. Pages fade in.
Reduce motion (the setting, or the desktop's) turns all of it off. The interface has no frame-rate
cap: it draws when something changes, as fast as the display refreshes.

## The kit (`app/ui/m3.slint`)

`Surface` is every pressable thing: a state layer on hover and press, a ripple, and a 2 px focus
ring when reached with Tab (pressed with Space or Enter). On it are built the filled, tonal,
outlined and text buttons, icon buttons, chips, the switch, text fields, the segmented control,
select fields, menu rows, the avatar (a picture or an initial) and tags. Icons are Material
Symbols, drawn as flat SVG in the text colour.

## Layout

An 80 px navigation rail, an 80 px top bar (search at the left; a server pill and an account pill
at the right, each opening a menu), and the page. The window opens at 1280 by 800 and stops at
900 by 600. Pages keep their scroll place when you leave and return. A player for music floats at
the bottom of every page while a song plays.

**Title pages** are a split hero: the words (logo or title, meta line, genres, tagline, synopsis,
the actions in two rows) on a panel at the left, the backdrop at the right melting into it, and
the facts as a strip under it. The synopsis is cut at eight lines with "More details" only when it
is longer. **Home** opens on a spotlight with a page indicator (previous and next around a bar to a
slide, the current one filling as it runs), then rows of cards. **Cards** are a poster, a wide
still or a square cover with its title and a line under it; resting the pointer on one opens a hover
card with the synopsis and the main actions.

## The player

mpv draws the picture into the window's own GL context, under the interface, which is transparent
where the picture is. The controls float over it and fade after 2.5 s of stillness. Three sizes:
default (with the details and an episode list beside it), theater and full screen. `I` shows a
readout of what mpv is doing.

## Content rules

- **No fabricated data.** Never invent play counts, bitrates presented as real, or numbers. Where
  the Jellyfin API would supply a value we don't have, show a real empty state.
- No AI copywriting: "Elevate", "Seamless", "Unleash", "Next-Gen", "Revolutionize".
- No filler chrome: "Scroll to explore", bouncing chevrons, decorative scroll hints. Rail paging
  arrows do the paging; they hide at the ends and never animate on their own.
- No emojis anywhere in the interface.
- Text says what happened and what to do, plainly, with no exclamation marks.

## The mark


`brand/bloom-mark.svg`: **a five-petal blossom in dusk colours with a play button in its
centre.** The petals sweep round like a pinwheel, with a dark outline and a soft shadow. It
replaced the earlier moth in a viewfinder (see below), which is in git history. The files,
colours and sizes are in `brand/README.md`.

**Why a blossom.** *Bloom* is the name, and the flower is the most direct picture of it. The
play button in the throat says what the app does. The dusk gradient (gold at the throat, through
pink, to lavender) sits well on the app's dark grounds and next to the amber accent.

- **Forms**: the icon (blossom on an indigo tile), the mark (blossom alone), the glyph (one
  colour, petals separated by gaps, play button cut out) and the lockup with the wordmark.
- **Colour**: the mark is drawn in its own colours and does not take the accent. Only the glyph
  takes the colour of whatever draws it.
- **Minimum size**: the mark reads down to about 24 px; below that use the glyph.
- **Style**: the mark is an illustration, a little warmer than the interface around it.

### The mark before this one: a moth in viewfinder corners

Chosen from a round of logo concepts and redrawn as a flat vector: a moth (drawn to light,
as bloom is light spreading past its source) inside viewfinder corners. Its rules were
angular straight-edged wings, a visible central body and the frame, to stay a moth and not
Bluesky's butterfly.

### Concepts drawn and rejected before the moth

Kept because simple geometric marks land on existing icons remarkably easily, and
every one of these was rejected *after* rendering it at 26px rather than from the
description. That is the lesson: draw it small before believing it.

| Concept | Collided with |
|---|---|
| Nested concentric rounded squares | A target, or a record button. Static. |
| Jellyfish bell with tentacles | A mushroom, at every size |
| Solid frame dissolving into stepped bars | A pause button beside a barcode |
| Six-petal rosette | An asterisk — at small sizes it *is* the `*` glyph |
| Radial burst of rays around a core | The brightness icon in every OS |
| Concentric arcs offset to one corner | The RSS logo |
| Three overlapping circles, overlaps knocked out | A hazard trefoil |
| Rounded square with a filled circle at one corner | A notification badge |

**Radial symmetry is where all the collisions live.** A lens-flare idea — a bright
circle with smaller artefacts trailing diagonally — was the most promising geometric
direction, but the moth won: it carries the light meaning through what a moth does,
rather than by drawing light.

### The name

A group of jellyfish is a *bloom* (the Jellyfin nod) and *bloom* is the optical
term (the broadcast identity). It deliberately avoids the `-fin` and `Jelly-`
conventions that fifteen-plus other clients already share — joining a crowd that
large buys recognition at the price of anonymity.

# Design System: Bloom

A fast Jellyfin desktop client. Linux first, Tauri 2 + Svelte 5.

**Where this lives:** the system was first demonstrated in a single-page HTML prototype, then
ported. In `app/`, the tokens, buttons, fields, switch, artwork cards with title-card fallback, rails
with paging arrows, the account and server menus, the sign-in screen and the home spotlight
are ported. So are the navigation rail, top-bar search, the library, series, film and search
pages with their tabs, episode list, pager, select and empty states, and the watch screen:
the player box and its controls with chapter ticks, the quality and track menu, theater and
full screen, the buffering level meter, the status readout, the up-next panel, the Up next
list, and the details, Media info and Cast cards under the player. So are the Settings
screen's rows, segmented controls, switches and accent swatches, the Servers screen, banners,
the shortcuts panel, the sign-in wall, the title page, hover cards and search suggestions. So are the motion
system and the Downloads screen.

Generated following the `taste-design` skill (Google Stitch `stitch-utilities` plugin),
with deviations documented in section 10. Deviations are deliberate — that section is
the most important part of this file.

---

## 0. Configuration — Dials

| Dial | Level | Rationale |
|------|-------|-----------|
| **Creativity** | `7` | Expressive where content lives (titles, artwork), restrained in chrome. This is an application operated daily, not a page visited once. |
| **Density** | `7` | A media client is metadata-dense: queues, episode lists, codec readouts. Note: the skill's "density > 7 → all numbers monospace" override is **rejected** — see §10, row 10. |
| **Variance** | `4` | Deliberately lowered from the skill's default of 8. High variance ("no two sections alike") is a landing-page dial and is actively harmful in a tool with daily muscle memory. Navigation must be predictable. |
| **Motion** | `5` | Playback UI must never compete with the video. Two orchestrated moments — the launch roll and the theater-mode transition; everything else is a short functional cue. |

---

## 1. Visual Theme & Atmosphere

A grading suite, not a dashboard. The interface takes its neutrals from the surround of a
reference monitor — a graphite calibrated to sit out of the way so the picture can be
judged against it — and its data typography from broadcast equipment readouts. The
atmosphere is quiet, instrument-like, and slightly severe, with warmth arriving only
through the accent and through artwork.

The organizing idea: **one family, two widths.** There is a single typeface in the entire
product. Presented titles — the watch title, the series hero, the artwork title cards, the
wordmark — are set in its expanded cut; every piece of interface furniture is set in its
normal cut. The hierarchy is carried by width and weight rather than by a second face,
which is quieter and harder to date than a display-font pairing.

---

## 2. Color Palette & Roles

Dark is the primary environment (a media client is used with the lights down). Light is a
full peer, not an inversion.

Surface steps are sized by **CIE L\* delta, not contrast ratio** — contrast ratio compresses
badly between adjacent dark surfaces and will tell you a clearly-visible step is "1.1:1".
Target ΔL\* ≈ 7–8 between ground and surface; below ~5 a panel stops reading as a panel.

### Dark

- **Graphite Ground** (`#141615`) — Page background. Faint green bias, never pure black
- **Graphite Surface** (`#232624`) — Panels, rows, quiet containers. ΔL\* 7.8 from ground
- **Graphite Raised** (`#2D312F`) — Hover fills, inactive track
- **Graphite Lifted** (`#363B39`) — Popovers, the one genuinely elevated layer
- **Signal Line** (`#3F4543`) — Structural 1px rules, used sparingly
- **Readout White** (`#E9ECEB`) — Primary text
- **Readout Steel** (`#A7AFAD`) — Secondary text, descriptions
- **Readout Dim** (`#8C9492`) — Metadata, timestamps, tertiary

### Light

- **Lightbox Ground** (`#DEE2E0`) — Page background, a photographic gray card
- **Lightbox Surface** (`#F4F6F5`) — Panels and rows. ΔL\* 7.2 from ground
- **Lightbox Raised** (`#E7EBE9`) — Hover fills
- **Lightbox Lifted** (`#FFFFFF`) — Popovers
- **Ink** (`#161918`) — Primary text
- **Ink Steel** (`#4A5150`) — Secondary
- **Ink Dim** (`#616967`) — Metadata. Meets AA on both ground and surface

### Accent — exactly one

- **Tally Amber** (`#D98237` dark / `#9C4E14` light) — Named for the tally light on a live
  camera. Spent almost entirely on the playhead, the active nav state, and focus rings. If
  it appears anywhere a user isn't meant to look, it is wrong. The light value is
  deliberately darker than the dark one: the accent is used as *text* on pressed controls
  and the active nav item, and it must clear 4.5:1 on both light surfaces.
- **Tally Amber Media** (`#D98237`, both themes) — The playhead, scrub thumb, and active
  player buttons. Player chrome sits on video, which is dark regardless of theme, so it
  must not follow the theme accent or it goes muddy in light mode.

A user-selectable accent is offered in Settings, and the choices there are curated rather
than a free colour picker — every option stays under 80% saturation, so the constraint is
enforced by the UI instead of by a rule nobody reads. **When implementing this for real, an
accent must be stored as a per-theme pair, not a single value.** The light value has to be
darker to clear 4.5:1 as text; the prototype applies one value to both themes and would
fail contrast in light mode.

### Semantic — separate from the accent, never decorative

- **Direct Green** (`#5E9B72` dark / `#2F6B46` light) — Direct Play, download complete
- **Transcode Rose** (`#C0736B` dark / `#9C4A43` light) — Server is transcoding, error

### Banned

- Purple/violet gradients — the AI aesthetic, and also Jellyfin's own brand, which we are
  deliberately not copying
- Pure black `#000000`
- Any accent above 80% saturation — this rules out YouTube red (`#FF0000`, 100%)
- Mixed warm/cool gray systems in one screen

---

## 3. Typography

Two roles, **one family**: `Archivo` variable, loaded with both the weight and width axes
(`wdth,wght@100..125,400..700`).

- **Display — Archivo at `font-stretch: 118%`, weight 600, tracking `-.015em`**
  Presented titles only: the watch title, the series hero, the generated artwork title
  cards, the wordmark. The expanded cut reads as modern signage and broadcast
  lower-thirds — confident at size without being decorative.

- **Body / UI — Archivo at normal width, weights 400–600**
  All interface furniture: navigation, buttons, labels, descriptions, section headings.
  A signage and newspaper grotesque — sturdy, slightly industrial, quiet at 13–15px.
  Body copy at `65ch` max width, leading `1.65`.

A display *serif* was tried here and rejected as too fancy for a modern UI. Reaching for a
second family to create title hierarchy is the reflex; a width axis does the same job
with less noise and nothing extra to load.

**There is no third face, and specifically no monospace.** Timecode, codecs, resolutions,
bitrates, file sizes, counts, durations, and paths are all set in Archivo with
`font-variant-numeric: tabular-nums`, which gives digits a fixed advance width so a
ticking timecode doesn't jitter — the actual reason mono gets reached for — without the
terminal texture. Tabular figures solve the alignment problem; monospace solves it *and*
announces itself.

### Rules

- The expanded cut appears where a title is *presented*. Where a title is merely *listed*
  (queue rows, episode rows, download rows, at 13–14px) it is normal width at weight 500 —
  expanded type costs horizontal room that dense rows don't have.
- **No serif anywhere**, display or otherwise.
- **No monospace anywhere.** Not for timecode, not for codecs, not for file paths, not for
  "technical" flavor.
- **No small-uppercase-letterspaced labels.** Sentence case at a legible size, with color
  and weight carrying the hierarchy. `text-transform: uppercase` paired with
  `letter-spacing` above `.05em` on sub-12px text is banned outright.
- Hierarchy comes from weight, color, and face, never from size alone.

> Monospace-as-texture and tiny uppercase tracked labels are, together, the single most
> recognizable signature of generated interfaces. They read as "technical" to a machine
> and as "template" to a person. Real media software — Plex, Apple TV, MUBI, Jellyfin's
> own web client — uses essentially none of either.

### Banned fonts

- `Inter` — the original AI tell
- `Geist` — now equally an AI tell; it is Vercel's face and the current default of
  generated interfaces. This overrides the `taste-design` recommendation (see §10)
- `Space Grotesk`, `Poppins`, `Montserrat`
- **Any monospace face** — `IBM Plex Mono`, `JetBrains Mono`, `Geist Mono`, `Space Mono`,
  `Roboto Mono`, and the generic `monospace` stack. Use `tabular-nums` instead
- Generic serif stacks (`Times New Roman`, `Georgia`, `Garamond`, browser default serif)

---

## 4. Component Stylings

- **Radii are role-differentiated.** Controls `6px`, artwork `8px`, containers `12px`.
  One radius stamped on everything flattens hierarchy and reads as generated.
- **Icons are solid, flat-terminal geometry on a 20px grid.** Filled silhouettes built from
  rectangles, triangles, and circles; square linecaps and mitred joins on the rare stroked
  element; never round caps. Transport controls (play, pause, previous, next) are solid
  because broadcast transport controls have always been solid, and the volume control is a
  level-meter rather than the usual speaker-with-arcs.
  **Banned: the 24px uniform-stroke round-cap outline style** — Feather, Lucide, and every
  set that imitates them. It is the default icon language of generated interfaces, and once
  the giveaway fonts are gone it becomes the loudest remaining signal. A set that reads as
  drawn for *this* product is worth the afternoon it costs.
- **Buttons** — Flat. Tactile `translateY(1px)` on active. No outer glow, no custom
  cursors. Primary is accent fill; everything else is a quiet surface or ghost.
- **Containers** — Borders are spent, not defaulted. Prefer a surface value step to a 1px
  outline. A border appears only where two surfaces of near-equal value meet — which, for
  cards sitting on the page ground, is the actual case: the step alone is not enough to
  draw a container edge, so cards carry a `--line-soft` hairline as well.
- **Artwork placeholders** — Real posters are frequently missing in a self-hosted library,
  so the placeholder is a designed component, not a fallback: a flat tinted field, faint
  film grain, and the title set as a title card in the expanded display cut. Tints vary in value,
  not hue, so they never compete with the accent. The grain belongs to the placeholder alone: it
  goes once real artwork has loaded.
- **Status readout** — Playback state is presented as one readout line with hairline
  separators, not a row of identical pills. Only the semantically important segment
  (Direct Play vs Transcoding) carries color — **and it carries it as the text color, with
  no status dot.** A small colored circle next to a status word is filler: the color is
  already on the word, so the dot restates it and adds a shape that has to be drawn,
  aligned, and explained to a screen reader for nothing.
- **Separators** — Two kinds, and they don't mix. *Structural* separation between fields in
  a row (metadata lines, readouts, download stats) is a 1px hairline rule in `--line`, set
  with a `border-left` on every child but the first. *Phrase-internal* enumeration inside a
  single value is plain prose — a comma, or a real word ("decoded locally via VAAPI",
  "1,284 items in Silent Era"). The interpunct `·` is banned as a separator in both cases.
- **Settings rows** — A two-column grid: name plus a hint on the left, the control hard
  right, separated by a `--line-soft` hairline, no card around them. The hint is where the
  consequence goes ("anything below Original forces the server to re-encode"), because a
  setting whose effect is unexplained gets left alone.
- **Segmented control** — For three or four mutually exclusive options that are worth
  seeing at once (theme, hardware decoder). Beyond four, use a select.
- **Select** — Bloom's own listbox (`Select.svelte`), never a native `<select>`: WebKitGTK
  hands the open list to GTK, which draws it in the desktop theme's colours. The trigger is a
  bordered `--surface` control with a CSS triangle that turns over when open, or, as a
  section title (the season picker), plain text in the heading size. The list is the menu
  panel: options with an optional tabular detail on the right ("25 episodes") and an accent
  check on the chosen one. Arrows, Home, End, Enter, Esc and typing a letter all work.
- **Switch** — For a setting that takes effect immediately with no confirmation. Accent
  fill when on, `--surface-2` when off. Never for anything destructive.
- **Rails** — A heading and a horizontally scrolling row with the scrollbar hidden. Paging
  arrows sit at each end on a fade to the page ground, appear on hover or keyboard focus,
  move about 85% of the visible width, and disappear at either end. Card widths are fixed
  per shape: poster 240px (2:3), wide 360px (16:9), square 200px, 20px apart, large enough
  for a hover card to fit its detail over the artwork; grids use the same sizes as minimums. A partly watched item
  carries a 3px `--accent-media` progress bar along the bottom of its artwork. An episode's
  card (Continue watching, Next up, search) is two targets: the artwork plays it, and the
  show's name and episode line below open the show's page, underlining on hover.
- **Spotlight** — The one large feature area, at the top of Home only. A full-bleed backdrop
  under a veil that fades to the page ground from the left and the bottom, so text sits on
  ground rather than on the picture. Content is bottom-left and at most 640px wide: the
  title logo (or the title in the expanded cut), then a metadata line with hairline
  separators and the age rating in a small outlined tag, genres as prose, a synopsis
  clamped to three lines, then Play (primary) and Favorite. The rating is written as
  "7.1/10", never with a star glyph. Clicking the slide anywhere but its buttons opens the
  title's page, growing from the backdrop; the title or logo is that same action as a button,
  with a focus ring, for the keyboard.
- **Slide indicator** — Segmented bars, 30px by 3px, bottom right. Never dots: a row of
  small circles is the carousel equivalent of a status dot. The current segment fills with
  the accent over the dwell time, which is also what advances the slide, so the bar is an
  honest timer and pauses exactly when the slides do. It fills in 30 steps, about a pixel each,
  not continuously: a continuous fill kept the page drawing every frame for as long as Home was
  open. It also waits while the spotlight is scrolled away or Home is hidden under another page.
- **Menus** — A raised panel below its trigger with a `--line` border and the shared shadow.
  The account menu opens with a monogram, the name and "Signed in on {server}" above its
  items. Closes on an outside click or Esc, returning focus to the trigger.
- **Hover cards** — After the mouse rests on a film, show or episode tile for 450ms, a raised
  card lies exactly over its artwork (a poster's art, an episode tile's still and title) and
  never past it; in the watch list, whose rows are too short, it's 300px wide and as tall as
  it needs. In a narrow card Play loses its word and the synopsis fades out where the room
  ends. The card is fixed so no rail clips it, and
  closes 140ms after the pointer leaves both, or on any scroll, resize or Esc. One is open at a
  time, and never on touch. A film or show: title with an accent heart when it's a favorite,
  rating, years, seasons and episodes or runtime, the age rating tag, a six-line synopsis, then
  Play (or Resume) and icon toggles for Favorite and Watched. An episode: the show, the number
  and title, "Aired {date}", Watched, the synopsis, then "Continue E4" (or Play, Watch again) as
  an accent text button and a menu to mark it watched. Clicking the card away from its buttons
  does what clicking the tile does. Everything on it is also reachable without hovering.
- **Transient readouts** — A small raised panel near the top centre for about 1.2s after an
  action with no other visible result, such as a zoom step ("125%"). Never a toast stack.
- **Player** — A 16:9 box with 12px corners (square in theater and full screen) that the page
  never paints: the ground around it is a spread shadow, so the picture fills exactly the
  box. Controls sit on a bottom scrim: a 4px seek bar (6px on hover) with the buffered range,
  an `--accent-media` played range and thumb, dark 2px chapter ticks, and a time tip with
  the chapter's name; then solid transport icons (previous chapter, play, next chapter and
  next episode, which has its own two-step icon), the level-meter volume icon, tabular time,
  and on the right subtitles, the menu, theater and full screen, where a pressed toggle turns
  `--accent-media`. The menu lists Quality (only qualities below the file's own, each saying it
  is transcoded by the server), Audio and Subtitles. Back sits on a top scrim, with the title
  beside it in theater and full screen. All of it hides after 2.5s without movement. The
  status readout sits under the box, never on the picture, and turns `--alert` when
  transcoding.
- **Watch details** — Under the player: the readout, the title, a metadata line, then a row
  with the episode's show (poster, name, season; a link to the show's page) on the left and
  Watched and Favorite on the right. Then `--surface` cards with 14px titles: Media info (the
  synopsis, a hairline, then the specs grid) and Cast (a horizontal row of square portraits).
- **Watch list** — Rows of a 148px 16:9 still with the runtime in a dark corner badge and a
  progress bar when partly watched, beside a title and "S1 E4" or the year, with a `--good`
  Watched check. For an episode it's the whole season, headed by a season picker, the playing
  episode on `--surface` with "Now playing" in `--accent-media` and centred in view; the next
  season's first three follow a divider (the season's name in `--ink-3` and a hairline), then
  a link to all of it. For a film, More like this. A 352px column beside the player in the
  default mode at 1180px and wider, pinned while the page scrolls and scrolling on its own; a
  card of the same rows in a grid otherwise.
- **Watch page scrolling** — The whole screen scrolls, and the details slide up over the
  pinned player like a sheet. The player itself never scrolls: mpv draws the picture under the
  page, and a picture following a moving box trails it by a frame or two, which reads as a
  wobble. Theater and full screen scroll back to the top.
- **Skip button** — While the playhead is in an intro, recap, credits or preview: "Skip intro"
  (or recap, credits, preview) over the bottom right of the picture, above the control bar, in
  the dark translucent fill and light hairline of the other panels on video, with the next
  icon. It stays when the controls hide and slides lower in their place. Credits with an episode
  after them read "Next episode". Never a countdown or an auto-skip.
- **Search suggestions** — Under the top-bar field as soon as two letters are typed: the menu
  panel with up to eight rows of a small poster, the title and "Film, 2016", then "See all
  results" in the accent above a hairline. The highlighted row is `--surface-2`, moved by the
  arrow keys and the pointer alike.
- **Shortcuts panel** — `?` anywhere: a `--raise` panel over a dimmed page, with Close, and two
  groups (Watching, Everywhere) of rows: the action in `--ink-2`, then each key as a small
  `--surface` cap with a heavier bottom border, alternatives joined by "or". Modal while open.
- **Servers screen** — Rows of a monogram, the server's name, then its address, version and
  the accounts saved for it in `--ink-3`, and on the right the state and the moves. States are
  words, never dots: Connected in `--good`, Reachable in `--ink-3`, "No response, last seen 3
  days ago" in `--alert`. Removing is confirmed in place ("Remove and sign out" or Keep), not
  in a modal. A server with downloads says how many and how big under its row, and asks
  "Remove and delete them", "Remove and keep them" or Cancel; kept ones come back if the
  server is added again. Adding by address sits under the list.
- **Banners** — A `--surface` strip above the page with a hairline border and one action on
  the right: the server not answering (Retry, with Browse downloads before it when anything is
  downloaded), or an action that failed (Dismiss, border mixed
  with `--alert`). Never a toast, never over the player.
- **Sign-in wall** — The sign-in screen's right side, as in the prototype: the artwork system at
  full bleed, four columns of 2:3 posters tilted −8° and scaled 1.18 past the edges, under a
  veil from `--surface` on the form's side (100°, gone by 70%). Posters come from the artwork
  cache, portrait files only (an episode's Primary is a landscape still): a random sixteen
  each time the screen opens, repeated to fill the grid when fewer are cached, and only from
  servers still on the Servers list (artwork is cached per server, and removing a server
  deletes it). On a first launch, with nothing cached, it's plain ground: never blank
  placeholder cards, which would read as a load that never finishes. Plain ground until anything is cached. No motion.
- **Desktop notifications** — Drawn by the desktop, so only the words are Bloom's. The summary
  says what arrived and where ("New episode of Frieren", "2 new episodes of Dandadan",
  "New film"), and the body says which, in the app's own codes ("S2 E5 · The Hero's Party",
  "S1 E6 to S1 E7", "Special 42", "Dune: Part Two (2024)"). What arrives within a minute is
  announced together. A big batch is one notification that counts and names the first three;
  past a hundred titles it says "Many new titles added" rather than a count it can't know. No exclamation marks, no emoji, no app name in the
  text (the desktop shows it).
- **Servers on your network** — On the sign-in screen's address step, above the address field:
  "On your network" with a "Search again" link, then each Jellyfin that answered as a bordered
  row on `--ground` (a monogram, the name, the address in `--ink-3`, and Connect in the
  accent). While looking, one quiet line says so; with none, a line says why a server might
  not answer and points at the address field. Choosing one checks it exactly like a typed
  address.
- **Keyboard shortcuts** are reached three ways: `?`, the help button at the foot of the rail
  above Settings, and "Keyboard shortcuts" in the account menu.
- **Custom themes** — In Settings → Appearance, under Theme: saved themes as bordered chips
  (four 10px colour swatches and the name; the one in use bordered in its accent) with an edit
  button each, and "New theme". The editor is a `--surface` panel in two columns: name,
  Dark or Light (each starting from Bloom's own colours), and Background, Panels, Text and
  Accent as a colour well with its hex beside it; then a live preview (a panel with a title,
  secondary and faint text, a primary button and a link, in the theme's own tokens), the three
  contrast checks with their ratios (`--good` passing, `--alert` with what's needed), and
  "Save and use", Copy CSS, Cancel and Delete. The shades between (surface-2, raise, lines,
  secondary text faded only as far as it reads at 4.5:1, the text on the accent) are worked out
  unless set by hand. "Edit as CSS" swaps the wells for a monospace text area holding the theme
  as `:root { … }` (color-scheme, `--f-ui`, every colour token as hex); what can't be used is
  listed under it in `--alert`, never applied. Shades set by hand are counted under the wells
  with a Clear. A Font menu lists the families installed on the computer, Archivo first. The
  checks run on the final tokens: text, secondary and faint text at 4.5:1 on both grounds, the
  accent at 3:1 on panels, and the text on the accent at 4.5:1. A theme failing them can't be
  saved or applied. The player's controls keep `--accent-media`.
- **Command palette** — Ctrl+K anywhere: a `--raise` panel 620px wide in the upper part of the
  window over a dimmed page, a 52px search field with an Esc key cap, and results in quiet
  groups (Go to, Libraries, Settings, Titles). Rows are an icon or a poster thumbnail, the name
  and a `--ink-3` detail; the highlighted row sits on `--surface-2` with its icon in the accent.
  Settings sections match on what they contain ("subtitles" finds Playback). Arrows move, Enter
  goes, Esc closes; it takes those keys from everything under it.
- **Up next** — When the closing credits start (the first credits segment in the last 40% of
  the file), a card in the bottom-right corner, where the skip button sits, while the credits
  keep playing: the next episode's artwork, "Up next in 8", title and episode, a 3px bar
  draining over the 10-second countdown, then Play now (primary) and Watch credits. Pausing
  pauses the countdown, seeking back out of the credits removes the card, and Watch credits
  keeps it away until the file ends. When an episode ends (or has no credits data), the same
  content is a dark panel centred on the player, with Play now focused and Cancel. The bar is
  the countdown itself, so it's a timer, not decoration.
- **Playback speed** — A row of bordered chips in the player menu, 0.5× to 2× with Normal in
  between, the choice in `--accent-media`; `<` and `>` step through them. Any speed but normal
  shows as a small outlined badge after the time, so it's never left on by mistake. Speed
  resets when playback stops. The volume is remembered across launches.
- **Seek thumbnails** — Where the server has made trickplay images, the seek tooltip shows the
  picture at that moment, 176px wide in a 4px dark frame, above the time and chapter, and kept
  inside the bar's ends. Without them the tooltip is the time alone. Each thumbnail is its own
  small image, cut from the server's tile by Bloom, so hovering never decodes a whole tile.
- **Navigation rail** — 64px of solid icons on `--surface`, only for destinations that exist.
  The active one sits on `--raise` in the accent. Collapsing narrows its grid column to zero
  and slides the icons out, never `display: none`, which would let the stage jump into its
  column.
- **Item page** — Laid out like Crunchyroll's title page, drawn in Bloom's system. A full-bleed
  hero like the spotlight (the veil also fades lightly from the top, under Back): the title
  logo or the title in the expanded cut, the age rating tag and a metadata line with hairline
  separators, genres as prose, then Play or Resume (primary), Trailer where the server lists
  one, Mark watched and Download for a film, and Favorite. Under it a two-column band closed by a hairline: the synopsis clamped to four lines
  with an accent "More details" (tagline, studio, status), and a definition list of audio and
  subtitle languages and the age rating. Then the episodes, a Cast rail, Media info, a "Part of
  … Collection" rail of posters oldest first with a bordered "View the collection" under it,
  and a More like this rail of posters. A collection's page is the same page with its titles
  in a poster grid, "In this collection" and the count, in place of the episodes. No star glyphs, vote counts or all-caps labels.
- **Episode grid** — Whole seasons as a grid of 16:9 tiles at least 240px wide. The heading is
  the season picker, the episode count and "Oldest first" or "Newest first". A tile: the still
  with a play disc on hover, the runtime badge, a progress bar, and "Up next" or "Continue" in
  `--accent-media` on the next episode; then the show's name in `--ink-3`, the number in
  `--ink-3` and the title, then the audio languages and "Subtitles" with hairline separators,
  a `--good` Watched (and Downloaded), and a menu button for Mark as watched and Download
  episode. Mark season watched (Mark season unwatched once every episode is) and Download season
  sit beside the sort order. Previous and next season sit
  under the grid above a hairline. Moves like these (previous and next season, sort order, all
  of a season) are bordered buttons, never bare text, which is too easy to miss.
- **Downloads screen** — The page title over where files are saved, then a 10px storage bar in
  three parts (Bloom in the accent, other files, free space) with a legend. Rows in two groups,
  In progress and Ready to watch: 16:9 artwork, the title and episode, then stats split by
  hairlines (size of total, speed, time left, with "about" before an estimated size), and the
  bubbly bar under anything unfinished. Controls sit hard right: the quality, pause or resume,
  cancel. A finished row's artwork plays it and shows its resume bar; its delete asks again in
  place ("Delete 1.4 GB"), never in a modal. A failure is a line in `--alert` with Try again.
  Empty, it's the composed empty state pointing at the title pages.
- **Download button** — Bordered, beside Mark watched and Favorite, and says what's happening:
  Download, Queued, Downloading 42%, Download paused, or Downloaded with a check (pressed).
  Once a title is listed it opens the Downloads screen rather than acting twice. The rail's
  Downloads icon carries a 6px accent dot while a transfer runs.
- **Downloaded, elsewhere** — A card with anything downloaded (the film, or any episode of the
  show) carries a 22px badge in its top-left corner: the download glyph in `--good` on a dark
  plate. Home has a Downloaded row of posters after Next up, a show once with "4 episodes
  downloaded". A library's Downloaded filter is a bordered toggle beside the sort, pressed while
  on. With no server, Home keeps its notice and shows the Downloaded row under it.
- **Pager** — Previous, the first and last pages, the pages either side of the current one,
  and an ellipsis for the rest, then Next. The current page is an accent fill.
- **Loaders** — A skeleton matching the real layout, with a scan line passing down it. Never a diagonal shimmer, never a circular spinner.
- **Empty states** — Composed, with the action that populates them. Never bare "No items".
- **Errors** — Inline, adjacent to what failed. Routine failures (server unreachable) never
  take a modal.

---

## 5. Layout Principles

- CSS Grid for structure. No flexbox percentage math, no `calc()` column hacks.
- No overlapping content. Two deliberate exceptions, both with a scrim or veil so text
  never floats directly on an image: the player control bar, a scrim-backed band over
  video, as in every player; and the Home spotlight, whose text sits where its veil has
  already faded the backdrop to the page ground.
- Horizontal scroll rails for media rows. The "3 equal cards" pattern is banned; a library
  is naturally a rail.
- Fixed app shell with internal scroll: 64px icon rail, 56px top bar, scrolling stage.
- `min-height: 100dvh`, never `100vh`.
- A side gutter of at least 16px at every width.

---

## 6. Responsive Rules

Tested at 375, 768, 1024, and 1440px.

- Below 1180px the up-next rail moves beneath the player.
- Below 760px the icon rail becomes a bottom bar — the Android phase's real pattern.
- Single column below 768px. Horizontal overflow is a critical failure.
- Touch targets minimum 44px.
- Body text never below 14px. Titles scale via `clamp()`.

---

## 7. Motion & Interaction

- **Mechanical easing** for anything instrument-flavoured: `--ease-mech`,
  `cubic-bezier(.16, .84, .24, 1)` — fast in, hard settle, no overshoot. `--ease`
  (`cubic-bezier(.22, 1, .36, 1)`) remains for short interface transitions. Motion
  that repeats mechanically, like film advancing, uses `steps()`. Nothing bounces.
- Animate `transform` and `opacity` only. Never `top`, `left`, `width`, `height`.
- Lists cascade in on first render from a **visible** resting state — never parked at
  `opacity: 0` waiting on an observer.
- Perpetual loops are reserved for genuinely ongoing states: downloading, buffering,
  syncing. Static content is never animated to seem alive. The spotlight's auto-advance
  counts as an ongoing state only because its indicator is a visible timer that stops on
  hover and focus; with reduced motion it doesn't advance at all.
- Spotlight changes: the backdrop cross-fades over 0.7s and the text rises 8px into place
  over 0.42s on `--ease-mech`.
- Two orchestrated moments: the launch roll, and theater mode (chrome dims, player grows).
  The player box itself can't animate: mpv places the picture from the box's measured edges
  and trails the page by a frame or two, so a growing box shows the picture jumping behind it.
  So the layout changes at once (and the window, for full screen), the picture catches up
  within a frame or two, and the top bar and rail fade to or from their theater dimming. The
  picture never goes blank: a shutter of player ground used to close over the box during the
  switch, and while watching, that blank was more jarring than the catch-up (removed
  2026-09-15, by request).
- `prefers-reduced-motion` disables all of it.

### The motion vocabulary — built

Built in the HTML prototype and ported to `app/`. (Settings once had a Motion preview
playing them in place; it was removed on request, so each is seen where it belongs.)

The strongest version of this is **the app behaving like a piece of broadcast
equipment coming up**, where each state maps to a real instrument behaviour. That
gives a vocabulary that stays self-consistent instead of a pile of effects.

| State | Behaviour |
|---|---|
| Launch | Signal acquire: the picture rolls three times, decelerating, then locks with a small jolt and fades to the app. About 1.7s. Plays after signing in, never on a cold page load. Always dark, in both themes, because it is one moment rather than a surface |
| Content loading | A scan head passes down a skeleton that matches the real layout. Only shown for a real fetch — in the prototype, changing season |
| Buffering | A five-bar level meter in the player, shown between pressing play and the first frame. Each bar has its own period so it reads as a meter, not a generic wave. The volume icon's two bars scale with the volume, so icon and meter are the same instrument |
| Downloading | Bubbly, by request, replacing the prototype's sprocket holes: a rounded 12px bar whose fill has a soft highlight, small bubbles rising through it and popping, and a leading edge that swells and settles. Paused or queued, the bar is 5px and still. The one place Bloom's motion is organic rather than mechanical |
| Artwork arriving | Rows cross-fade in where the skeleton was, with a short stagger |
| Opening a title | View Transitions shared-element morph: the poster grows into the player, or a collection into the series artwork. Falls back to a plain view change when unsupported or when motion is reduced |

Two decisions, now made:

- **Mechanical over springy.** Broadcast gear snaps, settles and locks; it does not
  bounce. Recorded as deviation 11 in §10. The download bar is the one deliberate exception,
  chosen in review: a download is a long, pleasant kind of wait, and its bubbles sit nowhere
  near the player or anything else that moves.
- **Loaders must not be charming.** A fast client shows almost no loading states, and a
  delightful loader makes people notice waiting. So each one appears only for real
  waiting — buffering before the first frame, a skeleton for a real fetch — and
  buffering still shows with reduced motion because the wait is real; only its movement
  stops.

Reduced motion is honoured two ways: the operating system setting, and the Settings
switch, which sets `data-reduce-motion` on the root. Either one flattens every animation
and transition, skips the launch roll, and turns the title-opening morph into a plain
view change.

---

## 8. Content Rules

- **No fabricated data.** Never invent play counts, uptime, bitrates presented as real,
  or user numbers. Where the Jellyfin API would supply a value we don't have, show a real
  empty state.
- **Sample content is real content.** Prototypes use actual public-domain films with their
  actual years, runtimes, cast, and roles — Metropolis with Brigitte Helm as Maria, Flash
  Gordon with its real thirteen chapter titles. Never invented-but-plausible titles.
- Any screen showing sample metadata carries a visible prototype marker.
- No AI copywriting: "Elevate", "Seamless", "Unleash", "Next-Gen", "Revolutionize".
- No filler chrome: "Scroll to explore", bouncing chevrons, decorative scroll hints. Rail
  paging arrows are allowed because they do the paging; they hide at the ends and never
  animate on their own.
- No emojis anywhere in the UI.

---

## 9. Anti-Patterns (Banned)

- No emojis
- No `Inter`, `Geist`, `Space Grotesk`, `Poppins`, `Montserrat`
- **No monospace, anywhere, for any reason** — `tabular-nums` instead
- **No small-uppercase-letterspaced labels** as a decorative texture
- **No status dots** — no small colored circle beside a status word; color the word
- **No interpunct `·` separators** — hairline rules between fields, prose within a phrase
- **No Feather/Lucide-style icons** — no 24px uniform-stroke round-cap outlines
- **No screen ships without its empty, error, and loading states** — those are where
  generated UI is laziest, so they are where a reviewer looks first
- No serif, display or otherwise
- No pure black
- No neon or outer-glow shadows
- No accent above 80% saturation
- No gradient text on headers
- No custom mouse cursors
- No overlapping content (player scrim excepted)
- No 3-equal-column card grids
- No one-radius-fits-all; no border on every container
- No uniform pill rows standing in for information design
- No `LABEL // YEAR` formatting
- No generic placeholder names ("John Doe", "Acme", "Sample Movie")
- No fake round numbers or invented metrics
- No circular loading spinners
- No `100vh`
- No `z-index` beyond nav / modal / overlay layers

---

## 10. Deviations from `taste-design`

The skill's ban list is sound and is adopted nearly whole. Its *positive* recommendations
are calibrated for marketing sites and were rejected where they conflict with an
application UI or with the skill's own rules.

| # | Skill says | Bloom does | Why |
|---|-----------|--------------|-----|
| 1 | Accent from `#10B981` / `#3B82F6` / `#E11D48` / `#F59E0B` | Tally Amber `#D98237` | All four are raw Tailwind defaults at 84–92% saturation, violating the skill's own "below 80%" rule three lines earlier |
| 2 | Canvas White `#F9FAFB`, "warm-neutral" ground | Graphite ground, dark-primary | The warm off-white ground is itself a documented AI cluster; also wrong for a room with the lights down |
| 3 | Display font `Geist` / `Outfit` / `Cabinet Grotesk` / `Satoshi` | `Archivo` only, one family in two widths | Geist is now an AI-default face alongside Inter, and Satoshi and Cabinet Grotesk are Fontshare fonts that could not load in the prototype. Archivo's width axis creates title hierarchy without a second family |
| 4 | "Serif is always BANNED in dashboards or software UIs" | Agreed, in the end | A serif for film titles was tried as a deliberate override and rejected as too fancy for a modern UI. The ban stands; Archivo's expanded cut does the job the serif was meant to |
| 5 | Cards: "generously rounded corners (2.5rem)", whisper shadow | Role-differentiated 6/8/12px, near-flat | Equipment UI wants tight geometry. Large uniform radii plus a stamped shadow is the generated-card look |
| 6 | Hero rules: inline image typography, one CTA, no centered hero | A bottom-left spotlight on Home only, with the title's own logo and two actions | The marketing-site rules don't fit. The spotlight exists because browsing a library benefits from a featured title; it is left-aligned (agreeing with "no centered hero"), uses the artwork's logo rather than invented typography, and has Play plus Favorite because both are real actions on the item |
| 7 | Variance dial `8` | Variance `4` | "No two sections alike" is correct for a landing page and wrong for a tool operated daily |
| 8 | Bento grid architecture | Horizontal rails and lists | A media library is a rail. Bento is a feature-grid pattern |
| 9 | `picsum.photos` for missing images | Generated title-card placeholders | External images cannot load in the prototype, and a self-hosted library needs a real designed missing-artwork state anyway |
| 10 | "When density exceeds 7, all numbers must use Monospace"; mono for metadata and timestamps | No monospace at all; `tabular-nums` on Archivo | The largest single correction in this system. Applied literally, this rule put mono on every duration, codec, count, path and label in the UI, and that texture — plus the tiny uppercase tracked labels it encourages — is the most recognizable generated-interface signature there is. Tabular figures deliver the alignment the rule is actually after |
| 11 | "Spring-based exclusively. stiffness: 100, damping: 20" | Mechanical easing, `steps()` for repeating motion, no overshoot, except the bubbly download bar | Broadcast equipment snaps, settles and locks. Springs read as playful consumer software, and a bouncing buffering meter or launch would contradict the instrument identity everything else is built on. The download bar was changed to bubbles in review; it's contained to downloads and never near playback |

---

## 11. The Mark

`brand/bloom-moth.svg`: **a moth inside viewfinder corners.** Chosen from a round of logo
concepts and redrawn as a flat vector for this system.

**Why a moth.** Moths are drawn to light, and bloom is light spreading past its
source — so the mark connects to the optical meaning of the name without drawing a
glow. The viewfinder corners say *screen* and *camera* at a glance, which fits the
broadcast-equipment identity.

**How it was simplified from the concept.** The concept's folded-paper detail
dissolved below about 26px, so the redraw uses fewer, chunkier facets with flat fill
and no gradient fades. The moth is enlarged within the frame and the corner arms are
short, so at 16px the moth carries the mark rather than the frame. Antennae are kept
short; longer ones merged with the head into a "V".

**Not a butterfly.** A moth in this pose can drift toward Bluesky's butterfly. It
was rendered beside Bluesky's symbol at 132, 56, 26 and 16px and stays distinct at
every size, because of three things that must survive any future revision:
**angular straight-edged wings** (Bluesky's are smooth and rounded), **a visible
central body** (Bluesky has none), and **the frame**.

- **Grid**: 24-unit viewBox, `currentColor`, single colour.
- **Colour**: the accent on any ground; knocks out to `--accent-ink` on accent fills.
- **No gradients, glows, blur or shadows** — §9. Facet separation is negative space.
- **Minimum size**: 16px, tested. Antennae are allowed to disappear below that.
- **Clear space**: a quarter of the mark's width on every side; the frame corners
  already provide internal breathing room.
- **Do not** put it inside a rounded-square tile, round the wing corners, or drop the
  body — each one moves it toward a butterfly or a badge.

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

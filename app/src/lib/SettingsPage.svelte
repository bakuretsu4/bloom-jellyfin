<script lang="ts">
  import * as api from "./api";
  import { languageName } from "./language";
  import Select from "./Select.svelte";
  import ThemeEditor from "./ThemeEditor.svelte";
  import { prefs, saveSettings } from "./settings.svelte";
  import { stepZoom, view } from "./zoom.svelte";

  let { account }: { account: api.Account } = $props();

  // Offered as defaults; the player can still pick any track in any file.
  const LANGUAGES = ["ja", "en", "es", "fr", "de", "it", "pt", "ko", "zh", "ru"];
  const languageOptions = (unset: string) => [
    { value: "", label: unset },
    ...LANGUAGES.map((code) => ({ value: code, label: languageName(code) ?? code })),
  ];
  const QUALITY_OPTIONS: { value: api.Quality; label: string }[] = [
    { value: "original", label: "Original" },
    { value: "1080p", label: "1080p" },
    { value: "720p", label: "720p" },
  ];
  // Mirrors the pairs in src/app.css.
  const ACCENTS: { id: api.Accent; name: string; light: string; dark: string }[] = [
    { id: "amber", name: "Amber", light: "#e0862b", dark: "#e0862b" },
    { id: "green", name: "Green", light: "#2e8b6a", dark: "#2e8b6a" },
    { id: "blue", name: "Blue", light: "#3e6fb0", dark: "#3e6fb0" },
    { id: "red", name: "Red", light: "#c0463d", dark: "#c0463d" },
  ];

  let info = $state<api.AppInfo | null>(null);
  $effect(() => {
    api
      .appInfo()
      .then((loaded) => (info = loaded))
      .catch(() => {});
  });

  let s = $derived(prefs.current);
  const save = (change: Partial<api.Settings>) => void saveSettings(change);

  // The download folder is typed, then checked (and created) before it's saved.
  let locationDraft = $state(prefs.current?.downloadLocation ?? "");
  let locationNote = $state("");
  let locationError = $state("");
  let checkingLocation = $state(false);

  async function useLocation(e: SubmitEvent) {
    e.preventDefault();
    checkingLocation = true;
    locationError = "";
    locationNote = "";
    try {
      const resolved = await api.checkDownloadLocation(locationDraft);
      await saveSettings({ downloadLocation: locationDraft.trim() || null });
      locationNote = `New downloads go to ${resolved}.`;
    } catch (err) {
      locationError = api.message(err);
    } finally {
      checkingLocation = false;
    }
  }
</script>

{#snippet segmented(label: string, options: { value: string; label: string }[], value: string, pick: (value: string) => void)}
  <div class="seg" role="group" aria-label={label}>
    {#each options as option (option.value)}
      <button type="button" aria-pressed={option.value === value} onclick={() => pick(option.value)}>{option.label}</button>
    {/each}
  </div>
{/snippet}

{#snippet toggle(label: string, on: boolean, flip: () => void)}
  <button class="switch" role="switch" aria-checked={on} aria-label={label} onclick={flip}><span></span></button>
{/snippet}

<div class="page">
  <header class="page-head">
    <div class="page-heading">
      <h1 class="page-title">Settings</h1>
      <span class="eyebrow">These apply to this computer.</span>
    </div>
  </header>

  {#if !s}
    <div class="page-notice" role="alert">
      <h2>Couldn't read the settings</h2>
      <p>Bloom is using its defaults. Restarting it usually helps.</p>
    </div>
  {:else}
    <div class="settings">
      <section class="set-section" aria-labelledby="set-playback">
        <div class="set-head">
          <h2 id="set-playback">Playback</h2>
          <p>Tracks and quality picked in the player for one show or film still win over these.</p>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Hardware decoder</div>
            <div class="set-hint">
              Auto picks the best decoder {info?.glRenderer ? "for" : "on this computer"}
              {#if info?.glRenderer}<b>{info.glRenderer}</b>{/if}
              and falls back to software for anything it can't decode. Changes take effect straight away.
            </div>
          </div>
          {@render segmented(
            "Hardware decoder",
            [
              { value: "auto", label: "Auto" },
              { value: "nvdec", label: "NVDEC" },
              { value: "vaapi", label: "VA-API" },
              { value: "software", label: "Software" },
            ],
            s.hwdec,
            (value) => save({ hwdec: value as api.Hwdec }),
          )}
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Prefer direct play</div>
            <div class="set-hint">
              Plays the original file whenever this computer can decode it. Off, the server transcodes everything,
              which costs it CPU and costs you picture.
            </div>
          </div>
          {@render toggle("Prefer direct play", s.directPlay, () => save({ directPlay: !s.directPlay }))}
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Starting quality</div>
            <div class="set-hint">
              The quality the player opens at. Anything below Original has the server re-encode files larger or
              heavier than it.
            </div>
          </div>
          <Select
            label="Starting quality"
            value={s.maxQuality}
            options={QUALITY_OPTIONS}
            onChange={(value) => save({ maxQuality: value })}
          />
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Audio language</div>
            <div class="set-hint">For a file with more than one audio track, when you haven't picked one for that show or film.</div>
          </div>
          <Select
            label="Audio language"
            value={s.audioLanguage ?? ""}
            options={languageOptions("Server's choice")}
            onChange={(value) => save({ audioLanguage: value || null })}
          />
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Subtitles</div>
            <div class="set-hint">
              Forced only shows signs and lines spoken in another language. Server's choice follows your Jellyfin
              account's settings.
            </div>
          </div>
          {@render segmented(
            "Subtitles",
            [
              { value: "server", label: "Server's choice" },
              { value: "off", label: "Off" },
              { value: "forced", label: "Forced only" },
              { value: "always", label: "Always" },
            ],
            s.subtitleMode,
            (value) => save({ subtitleMode: value as api.SubtitleMode }),
          )}
        </div>

        {#if s.subtitleMode === "forced" || s.subtitleMode === "always"}
          <div class="set-row">
            <div>
              <div class="set-name">Subtitle language</div>
              <div class="set-hint">Which language's subtitles to show, where a file has several.</div>
            </div>
            <Select
              label="Subtitle language"
              value={s.subtitleLanguage ?? ""}
              options={languageOptions("Same as the audio")}
              onChange={(value) => save({ subtitleLanguage: value || null })}
            />
          </div>
        {/if}

        <div class="set-row">
          <div>
            <div class="set-name">Subtitle size</div>
            <div class="set-hint">
              For text subtitles, styled ones included. Picture subtitles (PGS, VobSub) stay the size the file draws them.
            </div>
          </div>
          {@render segmented(
            "Subtitle size",
            [
              { value: "small", label: "Small" },
              { value: "normal", label: "Normal" },
              { value: "large", label: "Large" },
              { value: "huge", label: "Huge" },
            ],
            s.subtitleSize,
            (value) => save({ subtitleSize: value as api.Settings["subtitleSize"] }),
          )}
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Subtitle background</div>
            <div class="set-hint">
              A box reads better over bright scenes. Only plain subtitles take it; styled ones keep their own look.
            </div>
          </div>
          {@render segmented(
            "Subtitle background",
            [
              { value: "outline", label: "Outline" },
              { value: "box", label: "Dark box" },
            ],
            s.subtitleBackground,
            (value) => save({ subtitleBackground: value as api.Settings["subtitleBackground"] }),
          )}
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Autoplay next episode</div>
            <div class="set-hint">Starts the next episode ten seconds after one ends. Off, the player waits for you to choose.</div>
          </div>
          {@render toggle("Autoplay next episode", s.autoplayNext, () => save({ autoplayNext: !s.autoplayNext }))}
        </div>
      </section>

      <section class="set-section" aria-labelledby="set-notifications">
        <div class="set-head">
          <h2 id="set-notifications">Notifications</h2>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">New episodes and films</div>
            <div class="set-hint">
              A desktop notification when your server adds an episode or a film you can watch, while Bloom is open. A
              show's new episodes come as one, and a big batch as one in all.
            </div>
          </div>
          {@render toggle("Notify about new episodes and films", s.notifyNewMedia, () =>
            save({ notifyNewMedia: !s.notifyNewMedia }),
          )}
        </div>
      </section>

      <section class="set-section" aria-labelledby="set-discord">
        <div class="set-head">
          <h2 id="set-discord">Discord</h2>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Show what you're watching</div>
            <div class="set-hint">
              {#if info && !info.discordReady}
                Not available in this build yet: Bloom's Discord application isn't registered.
              {:else}
                While something plays, your Discord profile shows its cover, title, episode and time left, through the
                Discord app on this computer. It clears when playback stops. The cover is a public poster from your
                server's metadata sources (such as TMDB), since Discord can't reach your server. Nothing is sent when
                Discord isn't running, and nothing goes anywhere else.
              {/if}
            </div>
          </div>
          {#if info && !info.discordReady}
            <button class="switch" role="switch" aria-checked="false" aria-label="Show what you're watching on Discord" disabled>
              <span></span>
            </button>
          {:else}
            {@render toggle("Show what you're watching on Discord", s.discordPresence, () =>
              save({ discordPresence: !s.discordPresence }),
            )}
          {/if}
        </div>

        {#if s.discordPresence && info?.discordReady}
          <div class="set-row">
            <div>
              <div class="set-name">Show the title</div>
              <div class="set-hint">Off, your profile only says you're watching a show or a film, with no cover.</div>
            </div>
            {@render toggle("Show the title on Discord", s.discordShowTitle, () => save({ discordShowTitle: !s.discordShowTitle }))}
          </div>
        {/if}
      </section>

      <section class="set-section" aria-labelledby="set-downloads">
        <div class="set-head">
          <h2 id="set-downloads">Downloads</h2>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Location</div>
            <div class="set-hint">
              Where new downloads are saved, with their artwork and subtitles. Ones already downloaded stay where they
              are.
              {#if locationNote}<b>{locationNote}</b>{/if}
            </div>
            {#if locationError}<div class="set-error" role="alert">{locationError}</div>{/if}
          </div>
          <form class="location" onsubmit={useLocation}>
            <input
              class="set-input"
              type="text"
              spellcheck="false"
              placeholder="~/Videos/Bloom"
              aria-label="Download location"
              aria-invalid={!!locationError}
              bind:value={locationDraft}
            />
            <button class="btn" disabled={checkingLocation || locationDraft.trim() === (s.downloadLocation ?? "")}>Use</button>
          </form>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Download quality</div>
            <div class="set-hint">
              Original is the file itself: no work for the server, and a paused download carries on where it stopped.
              1080p and 720p are converted by the server as they download, at about 3.7 GB and 1.9 GB an hour, and
              start again if paused. A file already lighter than that downloads as it is, since converting it would
              only make it bigger.
            </div>
          </div>
          <Select
            label="Download quality"
            value={s.downloadQuality}
            options={QUALITY_OPTIONS}
            onChange={(value) => save({ downloadQuality: value })}
          />
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Downloads at once</div>
            <div class="set-hint">
              More finishes a queue sooner, but they share the connection, and each 1080p or 720p download is a
              conversion running on the server.
            </div>
          </div>
          {@render segmented(
            "Downloads at once",
            [
              { value: "1", label: "1" },
              { value: "2", label: "2" },
              { value: "3", label: "3" },
            ],
            String(s.downloadParallel),
            (value) => save({ downloadParallel: Number(value) }),
          )}
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Only download on the local network</div>
            <div class="set-hint">
              Downloads wait while {account.server.name} is reached over the internet rather than the network this
              computer is on.
            </div>
          </div>
          {@render toggle("Only download on the local network", s.downloadLocalOnly, () =>
            save({ downloadLocalOnly: !s.downloadLocalOnly }),
          )}
        </div>
      </section>

      <section class="set-section" aria-labelledby="set-appearance">
        <div class="set-head">
          <h2 id="set-appearance">Appearance</h2>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Theme</div>
            <div class="set-hint">
              Auto follows the desktop's light or dark preference. Choosing one of these leaves a custom theme.
            </div>
          </div>
          {@render segmented(
            "Theme",
            [
              { value: "auto", label: "Auto" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ],
            s.customTheme ? "" : s.theme,
            (value) => save({ theme: value as api.Theme, customTheme: null }),
          )}
        </div>

        <div class="set-row is-stacked">
          <div>
            <div class="set-name">Custom themes</div>
            <div class="set-hint">
              Your own background, panel, text and accent colours; Bloom works out the shades between. A theme is only
              used when its text and accent are readable, and the player's controls keep amber.
            </div>
          </div>
          <ThemeEditor
            themes={s.customThemes ?? []}
            activeId={s.customTheme}
            onChange={(themes, activeId) => save({ customThemes: themes, customTheme: activeId })}
          />
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Accent</div>
            <div class="set-hint">
              Each colour has a darker shade for the light theme, so it stays readable as text. The player's own
              controls keep amber, and a custom theme brings its own accent.
            </div>
          </div>
          <div class="accents" role="group" aria-label="Accent colour">
            {#each ACCENTS as accent (accent.id)}
              <button
                type="button"
                aria-pressed={s.accent === accent.id}
                aria-label={accent.name}
                title={accent.name}
                style:--pair-light={accent.light}
                style:--pair-dark={accent.dark}
                onclick={() => save({ accent: accent.id })}
              ></button>
            {/each}
          </div>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Zoom</div>
            <div class="set-hint">Also Ctrl with + and −, Ctrl with 0 to reset, or Ctrl with the scroll wheel.</div>
          </div>
          <div class="zoom" role="group" aria-label="Zoom">
            <button class="btn" aria-label="Zoom out" onclick={() => stepZoom(-1)} disabled={view.zoom <= 0.5}>−</button>
            <button class="btn zoom-level" aria-label="Reset zoom to 100%" onclick={() => stepZoom(0)}>{Math.round(view.zoom * 100)}%</button>
            <button class="btn" aria-label="Zoom in" onclick={() => stepZoom(1)} disabled={view.zoom >= 2}>+</button>
          </div>
        </div>

        <div class="set-row">
          <div>
            <div class="set-name">Reduce motion</div>
            <div class="set-hint">
              On, animations and transitions stop and the spotlight stays on one title. The desktop's own setting
              does the same. Buffering still shows, because that's real waiting.
            </div>
          </div>
          {@render toggle("Reduce motion", s.reduceMotion, () => save({ reduceMotion: !s.reduceMotion }))}
        </div>
      </section>

      <section class="set-section" aria-labelledby="set-about">
        <div class="set-head">
          <h2 id="set-about">About</h2>
        </div>
        <dl class="about">
          <div><dt>Bloom</dt><dd>{info ? `Version ${info.version}` : "Version unknown"}</dd></div>
          <div>
            <dt>Server</dt>
            <dd>{account.server.name}, Jellyfin {account.server.version}<br /><span class="dim">{account.server.address}</span></dd>
          </div>
          <div><dt>Signed in as</dt><dd>{account.user.name}</dd></div>
          <div><dt>Graphics</dt><dd>{info?.glRenderer ?? "Not detected"}</dd></div>
          <div><dt>Player</dt><dd>{info?.playerReady ? "Ready" : "Not started"}</dd></div>
        </dl>
      </section>
    </div>
  {/if}
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 24px;
    max-width: 840px;
  }
  /* Each group is a tonal card; its title is a small primary label, as in M3 settings lists. */
  .set-section {
    display: flex;
    flex-direction: column;
    padding: 20px 24px 8px;
    border-radius: var(--md-sys-shape-xl);
    background: var(--md-sys-color-surface-container-low);
  }
  .set-head {
    margin-bottom: 4px;
  }
  .set-head h2 {
    margin: 0;
    font: 500 14px/20px var(--f-ui);
    letter-spacing: 0.1px;
    color: var(--md-sys-color-primary);
  }
  .set-head p {
    margin: 4px 0 0;
    max-width: 62ch;
    font: 400 14px/20px var(--f-ui);
    letter-spacing: 0.25px;
    color: var(--md-sys-color-on-surface-variant);
  }
  /* A two-column grid: name and consequence on the left, the control hard right. */
  .set-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 24px;
    padding: 16px 0;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  .set-row:last-child {
    border-bottom: 0;
  }
  /* A row whose control needs the width: the theme editor, under its name and hint. */
  .set-row.is-stacked {
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
  }
  .set-name {
    font: 400 16px/24px var(--f-ui);
    letter-spacing: 0.5px;
  }
  .set-hint {
    margin-top: 2px;
    max-width: 54ch;
    font: 400 14px/20px var(--f-ui);
    letter-spacing: 0.25px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .set-hint b {
    font-weight: 500;
    color: var(--md-sys-color-on-surface);
  }

  .switch:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .set-error {
    margin-top: 6px;
    font-size: 12.5px;
    color: var(--alert);
  }
  .location {
    display: flex;
    gap: 6px;
  }
  .set-input {
    width: 260px;
    max-width: 100%;
    height: 40px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--md-sys-shape-xs) var(--md-sys-shape-xs) 0 0;
    background: var(--md-sys-color-surface-container-highest);
    box-shadow: inset 0 -1px 0 var(--md-sys-color-on-surface-variant);
    color: var(--md-sys-color-on-surface);
    font: 400 14px/20px var(--f-ui);
    transition: box-shadow var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard);
  }
  .set-input:focus-visible {
    outline: none;
    box-shadow: inset 0 -2px 0 var(--md-sys-color-primary);
  }
  .set-input[aria-invalid="true"] {
    box-shadow: inset 0 -2px 0 var(--md-sys-color-error);
  }

  .seg {
    display: inline-flex;
    /* M3 segmented button: joined pills with a shared outline; the chosen one fills. */
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-full);
    overflow: hidden;
  }
  .seg button {
    height: 40px;
    padding: 0 16px;
    border: 0;
    border-left: 1px solid var(--md-sys-color-outline);
    border-radius: 0;
    background: none;
    color: var(--md-sys-color-on-surface);
    font: 500 14px/20px var(--f-ui);
    letter-spacing: 0.1px;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard),
      color var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard);
  }
  .seg button:first-child {
    border-left: 0;
  }
  .seg button:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .seg button[aria-pressed="true"] {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }

  .accents {
    display: flex;
    gap: 8px;
  }
  /* Each swatch shows the half of its pair the current theme uses. */
  .accents button {
    width: 40px;
    height: 40px;
    padding: 0;
    border: 3px solid transparent;
    border-radius: 50%;
    background: var(--pair-light);
    background-clip: padding-box;
    box-shadow: inset 0 0 0 3px var(--md-sys-color-surface-container-low);
    cursor: pointer;
    transition:
      border-color var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard),
      transform var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized);
  }
  .accents button:hover {
    transform: scale(1.1);
  }
  .accents button[aria-pressed="true"] {
    border-color: var(--md-sys-color-on-surface);
  }

  .zoom {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .zoom .btn {
    min-width: 36px;
    justify-content: center;
    font-variant-numeric: tabular-nums;
  }
  .zoom .zoom-level {
    min-width: 64px;
  }

  .about {
    margin: 0;
  }
  .about div {
    display: grid;
    grid-template-columns: 160px minmax(0, 1fr);
    gap: 16px;
    padding: 12px 0;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    font: 400 14px/20px var(--f-ui);
    letter-spacing: 0.25px;
  }
  .about div:last-child {
    border-bottom: 0;
  }
  .about dt {
    color: var(--md-sys-color-on-surface-variant);
  }
  .about dd {
    margin: 0;
    overflow-wrap: anywhere;
    font-variant-numeric: tabular-nums;
  }
  .dim {
    color: var(--ink-3);
  }

  @media (max-width: 700px) {
    .set-row {
      grid-template-columns: minmax(0, 1fr);
      gap: 10px;
    }
  }
</style>

//! Device settings: everything the Settings screen changes, kept in `settings.json` in the app
//! data folder. One store owns the file, so saving one setting never drops another.

use crate::playback::Quality;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::State;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follows the desktop's light or dark preference.
    #[default]
    Auto,
    Light,
    Dark,
}

/// The curated accents; each is a pair of values, one per theme, in `src/app.css`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Amber,
    Green,
    Blue,
    Red,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Hwdec {
    #[default]
    Auto,
    Nvdec,
    Vaapi,
    Software,
}

impl Hwdec {
    /// As mpv's `hwdec` takes it.
    pub fn mpv_value(self) -> &'static str {
        match self {
            // The safe list only: decoders mpv knows to be reliable, falling back to software.
            Hwdec::Auto => "auto-safe",
            Hwdec::Nvdec => "nvdec",
            Hwdec::Vaapi => "vaapi",
            Hwdec::Software => "no",
        }
    }
}

/// Which subtitles a file starts with when nothing was picked for its show or film.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleMode {
    /// The server's pick, from the user's Jellyfin account settings.
    #[default]
    Server,
    Off,
    /// Only forced tracks: signs and lines in a language other than the audio's.
    Forced,
    Always,
}

/// How big subtitles are drawn. Applies to plain and styled (ASS) subtitles; image subtitles
/// (PGS, VobSub) are pictures at the file's own size.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleSize {
    Small,
    #[default]
    Normal,
    Large,
    Huge,
}

impl SubtitleSize {
    /// As mpv's `sub-scale` takes it.
    pub fn mpv_scale(self) -> &'static str {
        match self {
            SubtitleSize::Small => "0.8",
            SubtitleSize::Normal => "1",
            SubtitleSize::Large => "1.25",
            SubtitleSize::Huge => "1.5",
        }
    }
}

/// What's behind plain subtitles. Styled ones keep their own look, which typesetting relies on.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleBackground {
    #[default]
    Outline,
    Box,
}

impl SubtitleBackground {
    /// As mpv's `sub-border-style` takes it.
    pub fn mpv_value(self) -> &'static str {
        match self {
            SubtitleBackground::Outline => "outline-and-shadow",
            SubtitleBackground::Box => "background-box",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeBase {
    Light,
    #[default]
    Dark,
}

/// A theme someone made: four colours as `#rrggbb`, from which the page works out every other
/// token (src/lib/theme.ts), and whether it's a light or a dark one.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub base: ThemeBase,
    pub ground: String,
    pub surface: String,
    pub ink: String,
    pub accent: String,
    /// Tokens set by hand in the CSS editor, over the ones worked out from the four colours.
    pub tokens: BTreeMap<String, String>,
    /// A font installed on the computer, by family name; None is Bloom's Archivo.
    pub font: Option<String>,
}

/// More than anyone keeps; a bound on what the file can hold.
const MAX_THEMES: usize = 24;
/// The colour tokens a theme may set by hand. Mirrored in src/lib/theme.ts.
const THEME_TOKENS: [&str; 13] = [
    "--ground",
    "--surface",
    "--surface-2",
    "--raise",
    "--line",
    "--line-soft",
    "--ink",
    "--ink-2",
    "--ink-3",
    "--accent",
    "--accent-ink",
    "--good",
    "--alert",
];

/// A font family name as a CSS value can hold it: nothing that could close the string or the
/// declaration it's written into.
fn font_name(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    let safe = !name.is_empty() && name.chars().count() <= 80 && !name.chars().any(|c| matches!(c, '"' | '\'' | ';' | '{' | '}' | '\\' | '<' | '>') || c.is_control());
    safe.then(|| name.to_string())
}

fn hex_colour(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())).then(|| value.to_ascii_lowercase())
}

/// Themes as they're kept: valid colours, a short name, and a unique id safe to compare.
fn clean_themes(themes: &[CustomTheme]) -> Vec<CustomTheme> {
    let mut kept: Vec<CustomTheme> = Vec::new();
    for theme in themes {
        if kept.len() == MAX_THEMES {
            break;
        }
        let id_ok = !theme.id.is_empty() && theme.id.len() <= 64 && theme.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let colours = (hex_colour(&theme.ground), hex_colour(&theme.surface), hex_colour(&theme.ink), hex_colour(&theme.accent));
        let (true, (Some(ground), Some(surface), Some(ink), Some(accent))) = (id_ok, colours) else { continue };
        if kept.iter().any(|k| k.id == theme.id) {
            continue;
        }
        let name: String = theme.name.trim().chars().take(40).collect();
        let tokens = theme
            .tokens
            .iter()
            .filter(|(token, _)| THEME_TOKENS.contains(&token.as_str()))
            .filter_map(|(token, value)| Some((token.clone(), hex_colour(value)?)))
            .collect();
        kept.push(CustomTheme {
            id: theme.id.clone(),
            name: if name.is_empty() { "Custom theme".into() } else { name },
            base: theme.base,
            ground,
            surface,
            ink,
            accent,
            tokens,
            font: font_name(theme.font.as_deref()),
        });
    }
    kept
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Changed through the zoom commands (zoom.rs), not the Settings screen's save.
    pub zoom: f64,
    pub theme: Theme,
    pub accent: Accent,
    /// Themes made in Settings → Appearance.
    pub custom_themes: Vec<CustomTheme>,
    /// The custom theme in use, by id; None uses `theme` and `accent`.
    pub custom_theme: Option<String>,
    pub reduce_motion: bool,
    pub hwdec: Hwdec,
    /// Off asks the server to transcode everything.
    pub direct_play: bool,
    /// The quality the player starts at.
    pub max_quality: Quality,
    /// A language code ("ja" or "jpn"); None leaves the choice to the server.
    pub audio_language: Option<String>,
    pub subtitle_mode: SubtitleMode,
    /// None follows the audio's language.
    pub subtitle_language: Option<String>,
    pub subtitle_size: SubtitleSize,
    pub subtitle_background: SubtitleBackground,
    pub autoplay_next: bool,
    /// The player's volume, 0 to 100, as it was last set. Changed through the player's own
    /// command, not the Settings screen's save.
    pub volume: f64,
    /// Where downloads are saved; None is `~/Videos/Bloom`. Changing it applies to new downloads.
    pub download_location: Option<String>,
    pub download_quality: Quality,
    /// Downloads wait while the server is reached over the internet rather than the local network.
    pub download_local_only: bool,
    /// How many downloads run at once, 1 to 3.
    pub download_parallel: u8,
    /// What's playing on the user's Discord profile, through the local Discord app.
    pub discord_presence: bool,
    /// Off, Discord only hears that a show or a film is playing.
    pub discord_show_title: bool,
    /// A desktop notification when the server adds an episode or a film, while Bloom is open.
    pub notify_new_media: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            theme: Theme::default(),
            accent: Accent::default(),
            custom_themes: Vec::new(),
            custom_theme: None,
            reduce_motion: false,
            hwdec: Hwdec::default(),
            direct_play: true,
            max_quality: Quality::default(),
            audio_language: None,
            subtitle_mode: SubtitleMode::default(),
            subtitle_language: None,
            subtitle_size: SubtitleSize::default(),
            subtitle_background: SubtitleBackground::default(),
            autoplay_next: true,
            volume: 100.0,
            download_location: None,
            download_quality: Quality::default(),
            download_local_only: false,
            download_parallel: 1,
            discord_presence: false,
            discord_show_title: true,
            notify_new_media: false,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("settings.json");
        let current = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, current: Mutex::new(current) }
    }

    fn lock(&self) -> MutexGuard<'_, Settings> {
        self.current.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self) -> Settings {
        self.lock().clone()
    }

    /// Changes the settings and writes them out, returning what was saved. A failed write isn't
    /// worth failing the change over: at worst it isn't remembered next launch.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Settings {
        let mut current = self.lock();
        change(&mut current);
        // Written while still held, so two changes can't land on disk in the wrong order.
        if let Ok(bytes) = serde_json::to_vec_pretty(&*current) {
            let tmp = self.path.with_extension("tmp");
            if let Some(dir) = self.path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, &self.path)).is_err() {
                eprintln!("bloom: couldn't save settings to {}", self.path.display());
            }
        }
        current.clone()
    }
}

/// The font families installed on this computer, for a custom theme's font, by name and each
/// once. Empty where fontconfig isn't there to ask.
#[tauri::command]
pub async fn system_fonts() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Ok(output) = std::process::Command::new("fc-list").args([":", "family"]).output() else { return Vec::new() };
        let mut families: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            // "Family,Localised name": the first is the name CSS knows it by.
            .filter_map(|line| font_name(line.split(',').next()))
            .collect();
        families.sort_by_key(|family| family.to_lowercase());
        families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        families
    })
    .await
    .unwrap_or_default()
}

/// A language code as settings keep it: two or three letters, lower case.
fn language_code(code: Option<String>) -> Option<String> {
    code.filter(|c| (2..=3).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_alphabetic())).map(|c| c.to_ascii_lowercase())
}

#[tauri::command]
pub fn settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

/// Saves what the Settings screen shows, and returns it as saved. The zoom level and volume are
/// kept as they are: they have their own commands, and the page's copy may be older.
#[tauri::command]
pub fn update_settings(
    store: State<'_, SettingsStore>,
    presence: State<'_, crate::discord::Presence>,
    downloads: State<'_, crate::downloads::Downloads>,
    playback: State<'_, crate::playback::Playback>,
    settings: Settings,
) -> Settings {
    let previous = store.get();
    let themes = clean_themes(&settings.custom_themes);
    let active_theme = settings.custom_theme.clone().filter(|id| themes.iter().any(|t| &t.id == id));
    let saved = store.update(|current| {
        *current = Settings {
            zoom: current.zoom,
            volume: current.volume,
            custom_themes: themes,
            custom_theme: active_theme,
            audio_language: language_code(settings.audio_language.clone()),
            subtitle_language: language_code(settings.subtitle_language.clone()),
            download_location: settings.download_location.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(String::from),
            download_parallel: settings.download_parallel.clamp(1, crate::downloads::MAX_PARALLEL),
            ..settings
        };
    });
    #[cfg(target_os = "linux")]
    if saved.hwdec != previous.hwdec && playback.is_playing() {
        // Takes effect at once, even mid-film: mpv reopens the decoder. With nothing playing it
        // waits: playback sets it before the next file loads.
        if let Err(e) = crate::player::set_hwdec(saved.hwdec.mpv_value()) {
            eprintln!("bloom: couldn't switch the decoder: {e}");
        }
    }
    #[cfg(target_os = "linux")]
    if (saved.subtitle_size, saved.subtitle_background) != (previous.subtitle_size, previous.subtitle_background) {
        // Also at once: the subtitle on screen redraws at the new size.
        if let Err(e) = crate::player::set_subtitle_style(saved.subtitle_size.mpv_scale(), saved.subtitle_background.mpv_value()) {
            eprintln!("bloom: couldn't restyle subtitles: {e}");
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (previous, &playback);
    presence.set(saved.discord_presence, saved.discord_show_title);
    // More downloads at once can start straight away.
    downloads.nudge();
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_file_keeps_its_zoom_and_gets_defaults() {
        let saved: Settings = serde_json::from_str(r#"{"zoom":1.25}"#).unwrap();
        assert_eq!(saved.zoom, 1.25);
        assert!(saved.direct_play && saved.autoplay_next);
        assert_eq!(saved.theme, Theme::Auto);
        assert_eq!(saved.hwdec.mpv_value(), "auto-safe");
        assert_eq!((saved.subtitle_size.mpv_scale(), saved.subtitle_background.mpv_value()), ("1", "outline-and-shadow"));
    }

    #[test]
    fn subtitle_style_in_the_pages_spelling() {
        let saved: Settings = serde_json::from_str(r#"{"subtitleSize":"huge","subtitleBackground":"box"}"#).unwrap();
        assert_eq!((saved.subtitle_size, saved.subtitle_background), (SubtitleSize::Huge, SubtitleBackground::Box));
        assert_eq!((saved.subtitle_size.mpv_scale(), saved.subtitle_background.mpv_value()), ("1.5", "background-box"));
    }

    #[test]
    fn settings_round_trip_in_the_pages_spelling() {
        let settings = Settings {
            theme: Theme::Light,
            accent: Accent::Blue,
            max_quality: Quality::Hd720,
            audio_language: Some("ja".into()),
            subtitle_mode: SubtitleMode::Forced,
            ..Settings::default()
        };
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["maxQuality"], "720p");
        assert_eq!(json["subtitleMode"], "forced");
        assert_eq!(json["accent"], "blue");
        assert_eq!(serde_json::from_value::<Settings>(json).unwrap(), settings);
    }

    #[test]
    fn custom_themes_are_kept_clean() {
        let theme = |id: &str, ground: &str| CustomTheme {
            id: id.into(),
            name: "  Midnight  ".into(),
            base: ThemeBase::Dark,
            ground: ground.into(),
            surface: "#232624".into(),
            ink: "#E9ECEB".into(),
            accent: "#d98237".into(),
            tokens: BTreeMap::from([
                ("--ink-3".to_string(), "#9A9F9D".to_string()),
                ("--shadow".to_string(), "#000000".to_string()),
                ("--line".to_string(), "red".to_string()),
            ]),
            font: Some("  Inter  ".into()),
        };
        let kept = clean_themes(&[
            theme("a1", "#141615"),
            theme("a1", "#000000"),
            theme("bad id!", "#141615"),
            theme("b2", "red"),
            theme("c3", "#12345"),
            CustomTheme { name: String::new(), ..theme("d4", "#101010") },
        ]);
        assert_eq!(kept.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a1", "d4"]);
        assert_eq!(kept[0].name, "Midnight");
        assert_eq!(kept[0].ink, "#e9eceb");
        assert_eq!(kept[1].name, "Custom theme");
        assert_eq!(
            kept[0].tokens,
            BTreeMap::from([("--ink-3".to_string(), "#9a9f9d".to_string())]),
            "a token Bloom doesn't have, or a colour that isn't hex, was kept"
        );
        assert_eq!(kept[0].font.as_deref(), Some("Inter"));
        assert_eq!(font_name(Some("x\"; } body { color: red")), None);
        assert_eq!(font_name(Some("Noto Sans CJK JP")), Some("Noto Sans CJK JP".into()));

        let json = serde_json::to_value(&kept[0]).unwrap();
        assert_eq!(json["base"], "dark");
        let saved: Settings = serde_json::from_str(r#"{"zoom":1.0}"#).unwrap();
        assert!(saved.custom_themes.is_empty() && saved.custom_theme.is_none());
    }

    #[test]
    fn language_codes_are_cleaned() {
        assert_eq!(language_code(Some("JPN".into())), Some("jpn".into()));
        assert_eq!(language_code(Some("".into())), None);
        assert_eq!(language_code(Some("english".into())), None);
        assert_eq!(language_code(Some("e1".into())), None);
    }
}

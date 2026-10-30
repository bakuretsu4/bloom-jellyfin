//! The Settings screen: the core's `Settings` as option indexes for the screen, and the screen's
//! changes back into `update_settings`, which saves them and applies what takes effect at once
//! (the decoder, the subtitle style, Discord).
//!
//! The options a control offers are listed here and in `ui/settings.slint`, in the same order;
//! `SettingsData` carries only which one is chosen.

use crate::{theme, App, NavTarget, Screen, SettingsData};
use bloom_core::discord::{self, Presence};
use bloom_core::downloads::Downloads;
use bloom_core::playback::{Playback, Quality};
use bloom_core::settings::{self, Accent, Hwdec, Settings, SettingsStore, SubtitleBackground, SubtitleMode, SubtitleSize, Theme};
use bloom_core::Ctx;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::sync::OnceLock;

/// Where this interface keeps its data, shown under About. Set once at startup.
static DATA_FOLDER: OnceLock<String> = OnceLock::new();

pub fn set_data_folder(path: &std::path::Path) {
    let _ = DATA_FOLDER.set(path.display().to_string());
}

const HWDEC: [Hwdec; 4] = [Hwdec::Auto, Hwdec::Nvdec, Hwdec::Vaapi, Hwdec::Software];
const QUALITY: [Quality; 3] = [Quality::Original, Quality::Hd1080, Quality::Hd720];
const SUBTITLE_MODE: [SubtitleMode; 4] = [SubtitleMode::Server, SubtitleMode::Off, SubtitleMode::Forced, SubtitleMode::Always];
const SUBTITLE_SIZE: [SubtitleSize; 4] = [SubtitleSize::Small, SubtitleSize::Normal, SubtitleSize::Large, SubtitleSize::Huge];
const SUBTITLE_BACKGROUND: [SubtitleBackground; 2] = [SubtitleBackground::Outline, SubtitleBackground::Box];
const THEME: [Theme; 3] = [Theme::Auto, Theme::Light, Theme::Dark];
const ACCENT: [Accent; 4] = [Accent::Amber, Accent::Green, Accent::Blue, Accent::Red];

/// Offered as defaults; the player can still pick any track in any file.
const LANGUAGES: [(&str, &str); 10] =
    [("ja", "Japanese"), ("en", "English"), ("es", "Spanish"), ("fr", "French"), ("de", "German"), ("it", "Italian"), ("pt", "Portuguese"), ("ko", "Korean"), ("zh", "Chinese"), ("ru", "Russian")];

fn index_of<T: PartialEq>(options: &[T], value: T) -> i32 {
    options.iter().position(|o| *o == value).unwrap_or(0) as i32
}

fn pick<T: Copy>(options: &[T], index: i32) -> Option<T> {
    options.get(usize::try_from(index).ok()?).copied()
}

/// The list's index for a saved language code (two or three letters), 0 when none or unlisted.
fn language_index(code: &Option<String>) -> i32 {
    let Some(code) = code else { return 0 };
    let code = code.to_ascii_lowercase();
    let two = match code.as_str() {
        "jpn" => "ja",
        "eng" => "en",
        "spa" => "es",
        "fre" | "fra" => "fr",
        "ger" | "deu" => "de",
        "ita" => "it",
        "por" => "pt",
        "kor" => "ko",
        "chi" | "zho" => "zh",
        "rus" => "ru",
        other => other,
    };
    LANGUAGES.iter().position(|(c, _)| *c == two).map_or(0, |i| i as i32 + 1)
}

fn language_code(index: i32) -> Option<String> {
    let i = usize::try_from(index).ok()?.checked_sub(1)?;
    LANGUAGES.get(i).map(|(c, _)| c.to_string())
}

/// The screen's view of `s`: every field that comes from the settings. The rest of
/// `SettingsData` (About, storage) is filled by `show` and survives changes.
fn fill(shown: &mut SettingsData, s: &Settings) {
    shown.hwdec = index_of(&HWDEC, s.hwdec);
    shown.direct_play = s.direct_play;
    shown.quality = index_of(&QUALITY, s.max_quality);
    shown.audio_language = language_index(&s.audio_language);
    shown.subtitle_mode = index_of(&SUBTITLE_MODE, s.subtitle_mode);
    shown.subtitle_language = language_index(&s.subtitle_language);
    shown.subtitle_size = index_of(&SUBTITLE_SIZE, s.subtitle_size);
    shown.subtitle_background = index_of(&SUBTITLE_BACKGROUND, s.subtitle_background);
    shown.autoplay_next = s.autoplay_next;
    shown.theme = index_of(&THEME, s.theme);
    shown.accent = index_of(&ACCENT, s.accent);
    shown.custom_active = s.custom_theme.is_some();
    shown.reduce_motion = s.reduce_motion;
    shown.notify_new_media = s.notify_new_media;
    shown.discord_presence = s.discord_presence;
    shown.discord_show_title = s.discord_show_title;
    shown.discord_available = discord::configured();
    shown.download_quality = index_of(&QUALITY, s.download_quality);
    shown.download_local_only = s.download_local_only;
    shown.download_parallel = i32::from(s.download_parallel.clamp(1, 3)) - 1;
    fill_zoom(shown, s.zoom);
}

/// The zoom level as the screen shows it.
pub fn fill_zoom(shown: &mut SettingsData, zoom: f64) {
    shown.zoom_text = format!("{}%", (zoom * 100.0).round()).into();
    shown.zoom_can_out = crate::zoom::next_level(zoom, -1) < zoom;
    shown.zoom_can_in = crate::zoom::next_level(zoom, 1) > zoom;
}

fn data(s: &Settings) -> SettingsData {
    let mut shown = SettingsData::default();
    fill(&mut shown, s);
    shown
}

/// Zoom changed (here or by the keys): the screen's readout follows.
pub fn zoom_changed(app: &App, zoom: f64) {
    let mut shown = app.get_settings_data();
    fill_zoom(&mut shown, zoom);
    app.set_settings_data(shown);
}

/// `s` with the setting `key` set to option `value` (an index, or 0 or 1). None for a key or an
/// index that isn't one of the screen's.
fn changed(mut s: Settings, key: &str, value: i32) -> Option<Settings> {
    let on = value != 0;
    match key {
        "hwdec" => s.hwdec = pick(&HWDEC, value)?,
        "direct-play" => s.direct_play = on,
        "quality" => s.max_quality = pick(&QUALITY, value)?,
        "audio-language" => s.audio_language = language_code(value),
        "subtitle-mode" => s.subtitle_mode = pick(&SUBTITLE_MODE, value)?,
        "subtitle-language" => s.subtitle_language = language_code(value),
        "subtitle-size" => s.subtitle_size = pick(&SUBTITLE_SIZE, value)?,
        "subtitle-background" => s.subtitle_background = pick(&SUBTITLE_BACKGROUND, value)?,
        "autoplay-next" => s.autoplay_next = on,
        // Choosing one of the three leaves a custom theme.
        "theme" => {
            s.theme = pick(&THEME, value)?;
            s.custom_theme = None;
        }
        "reduce-motion" => s.reduce_motion = on,
        "accent" => s.accent = pick(&ACCENT, value)?,
        "notify-new-media" => s.notify_new_media = on,
        "discord-presence" => s.discord_presence = on,
        "discord-show-title" => s.discord_show_title = on,
        "download-quality" => s.download_quality = pick(&QUALITY, value)?,
        "download-local-only" => s.download_local_only = on,
        // The control's index is one less than how many run at once.
        "download-parallel" => s.download_parallel = u8::try_from(value.checked_add(1)?).ok().filter(|n| (1..=3).contains(n))?,
        _ => return None,
    }
    Some(s)
}

fn language_options(first: &str) -> ModelRc<SharedString> {
    let names: Vec<SharedString> = std::iter::once(first).chain(LANGUAGES.iter().map(|(_, n)| *n)).map(SharedString::from).collect();
    ModelRc::new(VecModel::from(names))
}

/// Shows the screen. UI thread.
pub fn show(ctx: &Ctx, app: &App) {
    let store = ctx.state::<SettingsStore>();
    let current = store.get();
    let mut shown = data(&current);
    shown.version = env!("CARGO_PKG_VERSION").into();
    shown.data_folder = DATA_FOLDER.get().cloned().unwrap_or_default().into();
    shown.storage_text = crate::downloads::storage_text(ctx).into();
    shown.renderer = crate::mpv::renderer().unwrap_or_default().into();
    shown.player_ready = true;
    shown.account = app.get_user_name();
    shown.server_name = app.get_server_name();
    let version = app.get_server_version();
    shown.server_line = match (app.get_server_name().is_empty(), version.is_empty()) {
        (true, _) => SharedString::new(),
        (false, true) => app.get_server_name(),
        (false, false) => format!("{}, Jellyfin {version}", app.get_server_name()).into(),
    };
    shown.server_address = app.get_address();
    app.set_settings_data(shown);
    app.set_download_location(current.download_location.clone().unwrap_or_default().into());
    app.set_audio_languages(language_options("Server's choice"));
    app.set_subtitle_languages(language_options("Same as the audio"));
    crate::themes_editor::show(ctx, app);
    app.set_nav_target(NavTarget::Settings);
    app.set_screen(Screen::Settings);
}

/// One setting changed on the screen: saved, applied, and the screen brought in step.
pub fn change(ctx: Ctx, weak: Weak<App>, key: String, value: i32) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let store = ctx.state::<SettingsStore>();
        let Some(next) = changed(store.get(), &key, value) else { return };
        let saved = settings::update_settings(&ctx, &store, &ctx.state::<Presence>(), &ctx.state::<Downloads>(), &ctx.state::<Playback>(), next);
        let mut shown = app.get_settings_data();
        fill(&mut shown, &saved);
        app.set_settings_data(shown);
        if matches!(key.as_str(), "theme" | "accent") {
            theme::apply_settings(&app, &saved, true);
            crate::themes_editor::refresh_chips(&app, &saved);
        }
        if key == "reduce-motion" {
            app.global::<crate::Theme>().set_reduce_motion(saved.reduce_motion);
        }
    });
}

/// Enter in the location field: checks the folder (it must be somewhere downloads can go) and
/// saves it; empty goes back to the default folder.
pub fn set_location(ctx: Ctx, weak: Weak<App>, path: String) {
    bloom_core::rt::spawn(async move {
        let store = ctx.state::<SettingsStore>();
        let dl = ctx.state::<Downloads>();
        let location = if path.trim().is_empty() {
            None
        } else {
            match settings_check(&dl, path) {
                Ok(resolved) => Some(resolved),
                Err(message) => return crate::watch::say(&weak, message),
            }
        };
        let mut next = store.get();
        next.download_location = location;
        let saved = settings::update_settings(&ctx, &store, &ctx.state::<Presence>(), &dl, &ctx.state::<Playback>(), next);
        let text = crate::downloads::storage_text(&ctx);
        let _ = weak.upgrade_in_event_loop(move |app| {
            let mut shown = app.get_settings_data();
            shown.storage_text = text.into();
            app.set_settings_data(shown);
            app.set_download_location(saved.download_location.clone().unwrap_or_default().into());
        });
        crate::watch::say(&weak, "New downloads will be saved there.".into());
    });
}

fn settings_check(dl: &Downloads, path: String) -> Result<String, String> {
    bloom_core::downloads::check_download_location(dl, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_maps_back_to_itself() {
        let s = Settings::default();
        for (i, h) in HWDEC.iter().enumerate() {
            assert_eq!(changed(s.clone(), "hwdec", i as i32).unwrap().hwdec, *h);
        }
        for i in 0..ACCENT.len() {
            assert_eq!(index_of(&ACCENT, changed(s.clone(), "accent", i as i32).unwrap().accent), i as i32);
        }
        for (i, t) in SUBTITLE_SIZE.iter().enumerate() {
            let saved = changed(s.clone(), "subtitle-size", i as i32).unwrap();
            assert_eq!(saved.subtitle_size, *t);
            assert_eq!(data(&saved).subtitle_size, i as i32);
        }
    }

    #[test]
    fn an_unknown_key_or_index_changes_nothing() {
        let s = Settings::default();
        assert!(changed(s.clone(), "no-such-setting", 1).is_none());
        assert!(changed(s.clone(), "hwdec", 9).is_none());
        assert!(changed(s, "hwdec", -1).is_none());
    }

    #[test]
    fn languages_round_trip_and_older_three_letter_codes_are_found() {
        assert_eq!(language_code(0), None);
        assert_eq!(language_code(1).as_deref(), Some("ja"));
        assert_eq!(language_code(11), None);
        assert_eq!(language_index(&Some("en".into())), 2);
        assert_eq!(language_index(&Some("jpn".into())), 1);
        assert_eq!(language_index(&Some("xx".into())), 0);
        assert_eq!(language_index(&None), 0);
    }

    #[test]
    fn downloads_run_one_to_three_at_once_and_nothing_else() {
        let s = Settings::default();
        for (index, at_once) in [(0, 1), (1, 2), (2, 3)] {
            let saved = changed(s.clone(), "download-parallel", index).unwrap();
            assert_eq!(saved.download_parallel, at_once);
            assert_eq!(data(&saved).download_parallel, index);
        }
        assert!(changed(s.clone(), "download-parallel", 3).is_none());
        assert!(changed(s, "download-parallel", -1).is_none());
    }

    #[test]
    fn choosing_one_of_the_three_themes_leaves_a_custom_theme() {
        let s = Settings { custom_theme: Some("mine".into()), ..Settings::default() };
        assert!(data(&s).custom_active);
        let next = changed(s, "theme", 2).unwrap();
        assert!(next.custom_theme.is_none() && next.theme == Theme::Dark);
        assert!(!data(&next).custom_active);
    }

    #[test]
    fn the_zoom_control_stops_at_the_ends_of_the_scale() {
        let mut shown = SettingsData::default();
        fill_zoom(&mut shown, 1.0);
        assert_eq!((shown.zoom_text.as_str(), shown.zoom_can_out, shown.zoom_can_in), ("100%", true, true));
        fill_zoom(&mut shown, 0.5);
        assert!(!shown.zoom_can_out && shown.zoom_can_in);
        fill_zoom(&mut shown, 2.0);
        assert!(shown.zoom_can_out && !shown.zoom_can_in);
    }

    #[test]
    fn a_switch_turns_a_setting_on_and_off() {
        let s = changed(Settings::default(), "direct-play", 0).unwrap();
        assert!(!s.direct_play);
        assert!(changed(s, "direct-play", 1).unwrap().direct_play);
    }
}

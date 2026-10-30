//! Watching: starting playback, the core's player events into the controls, and the controls back
//! into the core.
//!
//! The core owns the session (reporting to the server, remembering tracks, resume points); this
//! turns its `player:state`, `player:tracks` and `player:ended` events into properties, and the
//! controls' callbacks into the same calls the page makes. What it adds is what the controls need
//! to know about the file being watched: its skippable stretches, its chapters, and the episode
//! that follows.

use crate::playpage::{self, Trickplay};
use crate::{cards, nav, text, App, Player, Screen, TrackData};
use bloom_core::downloads::Downloads;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::playback::{self, Playback, Quality};
use bloom_core::settings::SettingsStore;
use bloom_core::{library, Ctx};
use serde::Deserialize;
use serde_json::Value;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::sync::{LazyLock, Mutex};

/// The spotlight's Play button names no type: it plays whatever the spotlight is (a film, or a
/// show's next episode, which the core works out).
pub const PLAY_ANY: &str = "Play";

/// The next episode starts this many seconds after the offer, with "play the next episode" on.
const COUNTDOWN: std::time::Duration = std::time::Duration::from_secs(10);
/// A credits stretch counts as the closing credits when it starts this far into the file.
const CREDITS_AFTER: f64 = 0.6;

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Segment {
    /// "intro", "recap", "credits" or "preview".
    kind: String,
    start_seconds: f64,
    end_seconds: f64,
}

#[derive(Clone)]
struct Next {
    item_id: String,
    title: String,
    subtitle: String,
    image: Option<String>,
}

/// The next episode's offer is counting down.
#[derive(Clone)]
struct Countdown {
    /// "credits" (the picture still plays) or "end" (this one has finished).
    kind: &'static str,
    remaining_ms: u64,
}

#[derive(Clone)]
struct TrackInfo {
    id: i64,
    audio: bool,
    label: String,
    selected: bool,
    /// The file's own pick, shown as current when nothing has been selected (mpv reports none
    /// selected while there is no audio device).
    default: bool,
    forced: bool,
    detail: String,
}

/// What the controls need to know about the file being watched.
#[derive(Default)]
struct Watching {
    item_id: String,
    /// A trailer is playing (for a title), not a file of the library.
    trailer: bool,
    quality: Quality,
    position: f64,
    duration: f64,
    segments: Vec<Segment>,
    chapters: Vec<f64>,
    chapter_names: Vec<String>,
    trickplay: Option<Trickplay>,
    up_next: Option<Next>,
    countdown: Option<Countdown>,
    /// "Watch credits" was chosen: the offer doesn't come back over this file.
    credits_dismissed: bool,
    started: bool,
    paused: bool,
    tracks: Vec<TrackInfo>,
    audio: Option<i64>,
    subtitle: Option<i64>,
    /// The subtitle track last turned off, for C to bring back.
    last_subtitle: Option<i64>,
}

/// How many times the trailer on screen has been looked for again after the player couldn't open it.
pub static TRAILER_RETRIES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A trailer's stream wouldn't open (YouTube sometimes refuses a stream it has just offered): look
/// for it again once, which gives a fresh address, and after that open it in the browser.
fn recover_trailer(ctx: Ctx, weak: &Weak<App>, id: String, problem: String) {
    use std::sync::atomic::Ordering;
    if TRAILER_RETRIES.fetch_add(1, Ordering::SeqCst) < 1 {
        eprintln!("bloom: the trailer wouldn't open ({problem}); looking for it again");
        return play_trailer(ctx, weak.clone(), id, 0, || {});
    }
    TRAILER_RETRIES.store(0, Ordering::SeqCst);
    eprintln!("bloom: the trailer wouldn't open twice ({problem}); opening it in the browser");
    let weak = weak.clone();
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let result = bloom_core::trailer::open_trailer(&jf, id, 0).await;
        ui(&weak, move |app| {
            let text = match result {
                Ok(()) => "This trailer wouldn't play here, so it opened in your browser.".to_string(),
                Err(e) => format!("This trailer wouldn't play here, and the browser wouldn't open it either. {e}"),
            };
            app.global::<Player>().set_up_next_kind("".into());
            app.global::<Player>().set_error(text.into());
        });
    });
}

/// Which countdown is current; a timer that finds it changed has been replaced.
static COUNT_RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The id of what is playing.
pub fn current_item() -> String {
    watching().item_id.clone()
}

/// What the seek preview needs: the length, the chapters' starts and names, and the thumbnails.
pub fn preview_facts() -> (f64, Vec<f64>, Vec<String>, Option<Trickplay>) {
    let w = watching();
    (w.duration, w.chapters.clone(), w.chapter_names.clone(), w.trickplay.clone())
}

impl Watching {
    /// Skipping isn't offered before the picture has started (tests, with nothing playing, aside).
    fn started_or_unknown(&self) -> bool {
        self.started || self.item_id.is_empty()
    }
}

static WATCHING: LazyLock<Mutex<Watching>> = LazyLock::new(Mutex::default);

fn watching() -> std::sync::MutexGuard<'static, Watching> {
    WATCHING.lock().unwrap_or_else(|e| e.into_inner())
}

fn ui(weak: &Weak<App>, f: impl FnOnce(&App) + Send + 'static) {
    let _ = weak.upgrade_in_event_loop(move |app| f(&app));
}

/// Shows a message at the bottom of the window for a few seconds.
pub fn say(weak: &Weak<App>, text: String) {
    if crate::session::is_signed_out(&text) {
        return crate::session::ended(weak);
    }
    ui(weak, move |app| {
        app.set_message(text.into());
        let weak = app.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_secs(4), move || {
            if let Some(app) = weak.upgrade() {
                app.set_message("".into());
            }
        });
    });
}

/// Plays `id` (a film, an episode, or the spotlight's `PLAY_ANY`, which the core works out).
pub fn play(ctx: Ctx, weak: Weak<App>, id: String) {
    play_with(ctx, weak, id, None, None);
}

/// Plays a trailer of `id` in the player, through yt-dlp. When it can't, the trailer opens in the
/// browser and the page says so. `finished` runs once it is settled either way (the trailer
/// playing, the browser opened, or an error said), so the page can let the button be pressed again.
pub fn play_trailer(ctx: Ctx, weak: Weak<App>, id: String, index: usize, finished: impl FnOnce() + Send + 'static) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let pb = ctx.state::<playback::Playback>();
        let store = ctx.state::<bloom_core::settings::SettingsStore>();
        match playback::play_trailer(&ctx, &jf, &pb, &store, id.clone(), index).await {
            Ok(now) => {
                finished();
                begin(&ctx, &weak, now, false).await
            }
            Err(why) => {
                eprintln!("bloom: the trailer couldn't play in the player ({why}); opening it in the browser");
                let result = bloom_core::trailer::open_trailer(&jf, id, index).await;
                finished();
                ui(&weak, move |app| match result {
                    Ok(()) => app.set_title_action_note("Couldn't play the trailer here, so it opened in your browser.".into()),
                    Err(e) => app.set_title_action_error(format!("Couldn't play the trailer: {e}").into()),
                });
            }
        }
    });
}

/// Plays `id` at a quality, from a position: what a change of quality or of audio track during a
/// transcode asks for.
fn play_with(ctx: Ctx, weak: Weak<App>, id: String, quality: Option<Quality>, start: Option<f64>) {
    bloom_core::rt::spawn(async move {
        match playback::play(&ctx, id, quality, start).await {
            Ok(now) => begin(&ctx, &weak, now, true).await,
            Err(e) => fail_to_start(&weak, e.to_string()),
        }
    });
}

/// Playback couldn't start: the player screen opens anyway and says why, with a way back.
fn fail_to_start(weak: &Weak<App>, message: String) {
    if crate::session::is_signed_out(&message) {
        return crate::session::ended(weak);
    }
    ui(weak, move |app| {
        let player = app.global::<Player>();
        player.set_error(message.into());
        app.set_watch_title("".into());
        app.set_watch_subtitle("".into());
        app.set_nav_target(crate::NavTarget::None);
        app.set_screen(Screen::Watch);
    });
}

/// Readout of the source: "1920\u{d7}1080, HEVC, 12.4 Mb/s", and which lighter qualities are worth offering.
fn source_facts(now: &playback::NowPlaying) -> (String, bool, bool) {
    let src = serde_json::to_value(&now.source).unwrap_or(Value::Null);
    let (w, h, bitrate) = (src["width"].as_i64().unwrap_or(0), src["height"].as_i64().unwrap_or(0), src["bitrate"].as_i64().unwrap_or(0));
    let mut parts = Vec::new();
    if w > 0 && h > 0 {
        parts.push(format!("{w}\u{d7}{h}"));
    }
    if let Some(codec) = src["codec"].as_str() {
        parts.push(codec_label(codec));
    }
    if bitrate > 0 {
        parts.push(text::bitrate(bitrate));
    }
    // A downloaded copy is one fixed file, and so is a trailer.
    let fixed = now.downloaded || now.kind == "Trailer";
    let known = !src.is_null();
    let lighter = |qw: i64, qh: i64, qb: i64| !fixed && (!known || w > qw || h > qh || bitrate > qb);
    (parts.join(", "), lighter(1920, 1080, 12_000_000), lighter(1280, 720, 6_000_000))
}

/// What the player screen shows as playback begins. `learn`: also fetch what the controls want to
/// know about the item (a trailer has no chapters, segments or next episode).
async fn begin(ctx: &Ctx, weak: &Weak<App>, now: playback::NowPlaying, learn: bool) {
    // A film replaces whatever song was playing.
    crate::music::clear(weak);
    let item_id = now.item_id.clone();
    COUNT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    *watching() = Watching { item_id: item_id.clone(), trailer: !learn, quality: now.quality, duration: now.duration_seconds.unwrap_or(0.0), ..Watching::default() };
    let (title, quality) = (now.title.clone(), now.quality);
    let (original_detail, has_1080, has_720) = source_facts(&now);
    let method = now.method;
    let transcoding = now.transcoding;
    let subtitle = now.subtitle.clone().unwrap_or_default();
    ui(weak, move |app| {
        app.set_watch_title(title.clone().into());
        app.set_watch_subtitle(subtitle.into());
        app.set_duration(now.duration_seconds.unwrap_or(0.0) as f32);
        app.set_position(0.0);
        app.set_cache_end(0.0);
        app.set_paused(false);
        app.set_started(false);
        app.set_buffering(false);
        app.set_time_text("".into());
        app.set_controls_visible(true);
        app.set_menu_open(false);
        app.set_quality(quality_key(quality).into());
        app.set_speed(1.0);
        app.set_skip_label("".into());
        app.set_chapters(ModelRc::new(VecModel::<f32>::default()));
        app.set_audio_tracks(ModelRc::new(VecModel::<TrackData>::default()));
        app.set_subtitle_tracks(ModelRc::new(VecModel::<TrackData>::default()));
        app.set_nav_target(crate::NavTarget::None);
        let p = app.global::<Player>();
        p.set_error("".into());
        p.set_is_trailer(!learn);
        p.set_up_next_kind("".into());
        p.set_has_next(false);
        p.set_preview_visible(false);
        p.set_heading(title.into());
        p.set_method(method.into());
        p.set_transcoding(transcoding);
        p.set_readout("".into());
        p.set_decoder_text("".into());
        p.set_subtitle_selected(false);
        p.set_original_detail(original_detail.into());
        p.set_has_1080(has_1080);
        p.set_has_720(has_720);
        p.set_chapter_names(ModelRc::new(VecModel::<SharedString>::default()));
        p.set_action_error("".into());
        p.set_queue(ModelRc::new(VecModel::<crate::QueueData>::default()));
        p.set_queue_loading(learn);
        p.set_queue_next_name("".into());
        app.set_screen(Screen::Watch);
    });
    if learn {
        learn_about(ctx.clone(), weak.clone(), item_id, now.subtitle.unwrap_or_default()).await;
    } else {
        let w = weak.clone();
        ui(&w, |app| {
            let p = app.global::<Player>();
            p.set_meta(ModelRc::new(VecModel::<crate::MetaPart>::default()));
            p.set_overview("".into());
            p.set_media(ModelRc::new(VecModel::default()));
            p.set_cast(ModelRc::new(VecModel::<crate::PersonData>::default()));
            p.set_has_owner(false);
        });
    }
}

fn quality_key(q: Quality) -> &'static str {
    match q {
        Quality::Original => "original",
        Quality::Hd1080 => "1080p",
        Quality::Hd720 => "720p",
    }
}

/// Fetches what the controls and the page want to know about `item_id`: skippable stretches,
/// chapters, thumbnails, the episode after it, and the details.
async fn learn_about(ctx: Ctx, weak: Weak<App>, item_id: String, subtitle: String) {
    let jf = ctx.state::<Jellyfin>();
    let downloads = ctx.state::<Downloads>();
    let (segments, detail, next, trickplay) =
        tokio::join!(library::skip_segments(&jf, &downloads, item_id.clone()), library::item_detail(&jf, &downloads, item_id.clone()), playback::next_episode(&jf, &downloads, item_id.clone()), playback::trickplay_with(&jf, &item_id));

    let segments: Vec<Segment> = segments.ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let detail: Option<crate::title::Detail> = detail.ok().and_then(|v| serde_json::from_value(v).ok());
    let chapters: Vec<(f64, String)> = detail.as_ref().map(|d| d.chapters.iter().map(|c| (c.start_seconds, c.name.clone().unwrap_or_default())).collect()).unwrap_or_default();
    let next = next.ok().flatten().map(|n| Next { item_id: n.item_id, title: n.title, subtitle: n.subtitle.unwrap_or_default(), image: n.image.map(|i| i.path(320)) });
    let trickplay: Option<Trickplay> = trickplay.ok().flatten().and_then(|t| serde_json::to_value(t).ok()).and_then(|v| serde_json::from_value(v).ok());

    let mut w = watching();
    // Dropped if another file has started since.
    if w.item_id != item_id {
        return;
    }
    w.segments = segments;
    w.chapters = chapters.iter().map(|c| c.0).collect();
    w.chapter_names = chapters.iter().map(|c| c.1.clone()).collect();
    w.up_next = next.clone();
    w.trickplay = trickplay;
    drop(w);
    let (ctx2, weak2) = (ctx.clone(), weak.clone());
    ui(&weak, move |app| {
        let model: Vec<f32> = chapters.iter().map(|c| c.0 as f32).collect();
        app.set_chapters(ModelRc::new(VecModel::from(model)));
        let p = app.global::<Player>();
        p.set_chapter_names(ModelRc::new(VecModel::from(chapters.iter().map(|c| SharedString::from(c.1.as_str())).collect::<Vec<_>>())));
        p.set_has_next(next.is_some());
        if watching().item_id != item_id {
            return;
        }
        playpage::show_details(&ctx2, &weak2, app, &item_id, &subtitle, detail, cards::generation());
    });
}

/// Back to the page behind the player, ending playback.
pub fn back(ctx: Ctx, weak: Weak<App>) {
    ui(&weak, playpage::reset_mode);
    nav::resume(ctx.clone(), weak.clone());
    stop(ctx, weak);
}

/// Playback ends, and the page `route` opens in its place (a show, a person, Downloads).
pub fn leave_to(ctx: Ctx, weak: Weak<App>, route: nav::Route) {
    ui(&weak, playpage::reset_mode);
    nav::go(ctx.clone(), weak.clone(), route);
    stop(ctx, weak);
}

/// Ends playback and waits for the server to be told where it stopped. For what is about to take the
/// sign-in away (signing out), after which the report could no longer be delivered.
pub async fn stop_and_report(ctx: &Ctx) {
    COUNT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    *watching() = Watching::default();
    let pb = ctx.state::<Playback>();
    let _ = playback::playback_stop(ctx, &pb).await;
}

/// The player screen has gone, however it was left. Whatever is still playing stops, as the Back button
/// would have stopped it; after Back (which has already stopped it) there is nothing to do.
pub fn left_screen(ctx: Ctx, weak: Weak<App>) {
    if current_item().is_empty() {
        return;
    }
    ui(&weak, playpage::reset_mode);
    stop(ctx, weak);
}

fn stop(ctx: Ctx, weak: Weak<App>) {
    COUNT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    *watching() = Watching::default();
    bloom_core::rt::spawn(async move {
        let pb = ctx.state::<Playback>();
        if let Err(e) = playback::playback_stop(&ctx, &pb).await {
            say(&weak, e.to_string());
        }
    });
}

pub fn toggle_pause(ctx: Ctx, weak: Weak<App>) {
    bloom_core::rt::spawn(async move {
        if let Err(e) = playback::playback_toggle_pause(&ctx).await {
            say(&weak, e.to_string());
        }
    });
}

pub fn seek_by(ctx: Ctx, weak: Weak<App>, seconds: f32) {
    bloom_core::rt::spawn(async move {
        if let Err(e) = playback::playback_seek(&ctx, seconds as f64, true).await {
            say(&weak, e.to_string());
        }
    });
}

/// `seconds` is already worked out on the UI thread from the fraction pressed.
pub fn seek_to(ctx: Ctx, weak: Weak<App>, seconds: f64) {
    bloom_core::rt::spawn(async move {
        if let Err(e) = playback::playback_seek(&ctx, seconds, false).await {
            say(&weak, e.to_string());
        }
    });
}

pub fn set_volume(ctx: Ctx, weak: Weak<App>, volume: f32) {
    bloom_core::rt::spawn(async move {
        let store = ctx.state::<SettingsStore>();
        if let Err(e) = playback::playback_set_volume(&ctx, &store, volume as f64).await {
            say(&weak, e.to_string());
        }
    });
}

pub fn set_muted(ctx: Ctx, weak: Weak<App>, muted: bool) {
    bloom_core::rt::spawn(async move {
        if let Err(e) = playback::playback_set_muted(&ctx, muted).await {
            say(&weak, e.to_string());
        }
    });
}

pub fn set_speed(ctx: Ctx, weak: Weak<App>, speed: f32) {
    bloom_core::rt::spawn(async move {
        if let Err(e) = playback::playback_set_speed(&ctx, speed as f64).await {
            say(&weak, e.to_string());
        }
    });
}

/// Picks an audio (`audio`) or subtitle track; `id` -1 turns subtitles off.
pub fn set_track(ctx: Ctx, weak: Weak<App>, audio: bool, id: i32) {
    bloom_core::rt::spawn(async move {
        let pb = ctx.state::<Playback>();
        let kind = if audio { "audio" } else { "subtitle" }.to_string();
        let id = (id >= 0).then_some(id as i64);
        match playback::playback_set_track(&ctx, &pb, kind, id).await {
            // The stream has to be asked for again to carry the choice (a transcode).
            Ok(true) => restart_here(ctx, weak, None),
            Ok(false) => {}
            Err(e) => say(&weak, e.to_string()),
        }
    });
}

pub fn set_quality(ctx: Ctx, weak: Weak<App>, key: String) {
    let quality = match key.as_str() {
        "1080p" => Quality::Hd1080,
        "720p" => Quality::Hd720,
        _ => Quality::Original,
    };
    restart_here(ctx, weak, Some(quality));
}

/// Plays the same item again where it is, at `quality` (or the current one).
fn restart_here(ctx: Ctx, weak: Weak<App>, quality: Option<Quality>) {
    let (id, position, current) = {
        let w = watching();
        (w.item_id.clone(), w.position, w.quality)
    };
    if id.is_empty() {
        return;
    }
    play_with(ctx, weak, id, Some(quality.unwrap_or(current)), Some(position));
}

/// The stretch of the file the playhead is in, if it is one the player offers to skip.
fn current_segment(w: &Watching) -> Option<&Segment> {
    w.started_or_unknown().then(|| w.segments.iter().find(|s| ["intro", "recap", "credits", "preview"].contains(&s.kind.as_str()) && w.position >= s.start_seconds && w.position < s.end_seconds - 1.0)).flatten()
}

fn skip_label(segment: &Segment, has_next: bool) -> &'static str {
    match segment.kind.as_str() {
        "intro" => "Skip intro",
        "recap" => "Skip recap",
        "preview" => "Skip preview",
        // Over the credits, skipping goes on to the next episode when there is one.
        _ if has_next => "Next episode",
        _ => "Skip credits",
    }
}

pub fn skip(ctx: Ctx, weak: Weak<App>) {
    let (segment, next) = {
        let w = watching();
        (current_segment(&w).cloned(), w.up_next.clone())
    };
    let Some(segment) = segment else { return };
    // Skipping credits with another episode waiting goes straight to it.
    match (segment.kind.as_str(), next) {
        ("credits", Some(next)) => play(ctx, weak, next.item_id),
        _ => seek_to(ctx, weak, segment.end_seconds),
    }
}

pub fn play_next(ctx: Ctx, weak: Weak<App>) {
    let next = watching().up_next.clone();
    if let Some(next) = next {
        play(ctx, weak, next.item_id);
    }
}

/// Over the credits, "Watch credits" keeps playing and the offer doesn't come back; after the
/// end, "Cancel" leaves the player.
pub fn cancel_up_next(ctx: Ctx, weak: Weak<App>) {
    COUNT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let kind = {
        let mut w = watching();
        let kind = w.countdown.take().map(|c| c.kind);
        if kind == Some("credits") {
            w.credits_dismissed = true;
        }
        kind
    };
    match kind {
        Some("credits") => ui(&weak, |app| app.global::<Player>().set_up_next_kind("".into())),
        Some(_) => back(ctx, weak),
        None => {}
    }
}

/// The next episode is offered, and (with "play the next episode" on) counts down to starting.
fn begin_countdown(ctx: &Ctx, weak: &Weak<App>, kind: &'static str) {
    let run = COUNT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let autoplay = ctx.state::<SettingsStore>().get().autoplay_next;
    let next = {
        let mut w = watching();
        let Some(next) = w.up_next.clone() else { return };
        w.countdown = Some(Countdown { kind, remaining_ms: COUNTDOWN.as_millis() as u64 });
        next
    };
    let (ctx2, weak2) = (ctx.clone(), weak.clone());
    let path = next.image.clone();
    ui(weak, move |app| {
        let p = app.global::<Player>();
        p.set_up_next_kind(kind.into());
        p.set_up_next_timer(autoplay);
        p.set_up_next_seconds(COUNTDOWN.as_secs() as i32);
        p.set_up_next_fraction(1.0);
        p.set_up_next_has_image(false);
        app.set_up_next_title(next.title.into());
        app.set_up_next_subtitle(next.subtitle.into());
        if let Some(path) = path {
            cards::fetch_one(&ctx2, &weak2, cards::generation(), path, |app, image| {
                let p = app.global::<Player>();
                p.set_up_next_image(image);
                p.set_up_next_has_image(true);
            });
        }
    });
    if !autoplay {
        return;
    }
    let (ctx, weak) = (ctx.clone(), weak.clone());
    bloom_core::rt::spawn(async move {
        const STEP: u64 = 250;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(STEP)).await;
            if COUNT_RUN.load(std::sync::atomic::Ordering::SeqCst) != run {
                return;
            }
            let (remaining, waiting) = {
                let mut w = watching();
                let paused = w.paused;
                let Some(c) = w.countdown.as_mut() else { return };
                // Paused during the credits, the countdown waits as well.
                let waiting = c.kind == "credits" && paused;
                if !waiting {
                    c.remaining_ms = c.remaining_ms.saturating_sub(STEP);
                }
                (c.remaining_ms, waiting)
            };
            if waiting {
                continue;
            }
            if remaining == 0 {
                return play_next(ctx, weak);
            }
            ui(&weak, move |app| {
                let p = app.global::<Player>();
                p.set_up_next_seconds(remaining.div_ceil(1000) as i32);
                p.set_up_next_fraction(remaining as f32 / COUNTDOWN.as_millis() as f32);
            });
        }
    });
}

/// Jumps to the next (`direction` 1) or previous (-1) chapter.
pub fn jump_chapter(ctx: Ctx, weak: Weak<App>, direction: i32) {
    let target = {
        let w = watching();
        let starts = &w.chapters;
        if direction > 0 {
            // Past the last chapter is the end.
            starts.iter().copied().find(|s| *s > w.position + 1.0).or((w.duration > 0.0).then_some(w.duration))
        } else {
            // Within the first seconds of a chapter, back goes to the one before it.
            starts.iter().copied().rev().find(|s| *s < w.position - 2.0).or(Some(0.0))
        }
    };
    if let Some(seconds) = target {
        seek_to(ctx, weak, seconds);
    }
}

/// C: subtitles off if one is showing, else back on: the last one shown, or the file's own pick.
pub fn toggle_subtitles(ctx: Ctx, weak: Weak<App>) {
    let (current, target) = {
        let mut w = watching();
        let current = w.tracks.iter().find(|t| !t.audio && Some(t.id) == w.subtitle.or_else(|| t.selected.then_some(t.id))).map(|t| t.id);
        if let Some(id) = current {
            w.last_subtitle = Some(id);
        }
        let subs: Vec<&TrackInfo> = w.tracks.iter().filter(|t| !t.audio).collect();
        let target = w
            .last_subtitle
            .filter(|id| subs.iter().any(|t| t.id == *id))
            .or_else(|| subs.iter().find(|t| t.default && !t.forced).map(|t| t.id))
            .or_else(|| subs.iter().find(|t| !t.forced).map(|t| t.id))
            .or_else(|| subs.first().map(|t| t.id));
        (current, target)
    };
    match (current, target) {
        (Some(_), _) => set_track(ctx, weak, false, -1),
        (None, Some(id)) => set_track(ctx, weak, false, id as i32),
        (None, None) => {}
    }
}

/// The core's events, as they reach the UI. Called from whichever thread emitted them.
pub fn on_event(ctx: Option<Ctx>, weak: &Weak<App>, event: &str, payload: Value) {
    // Development: BLOOM_LOG_EVENTS=1 prints the player's events as they arrive.
    static LOG: LazyLock<bool> = LazyLock::new(|| std::env::var_os("BLOOM_LOG_EVENTS").is_some());
    if *LOG && event.starts_with("player:") {
        eprintln!("event {event} {payload}");
    }
    match event {
        "player:state" => on_state(ctx, weak, &payload),
        "player:tracks" => {
            let tracks = payload["tracks"].as_array().map(|t| t.iter().filter_map(track_info).collect()).unwrap_or_default();
            let readout = readout_of(payload["tracks"].as_array().map(Vec::as_slice).unwrap_or(&[]));
            watching().tracks = tracks;
            show_tracks(weak);
            ui(weak, move |app| app.global::<Player>().set_readout(readout.into()));
        }
        "player:ended" => {
            let Some(ctx) = ctx else { return };
            // A song ending is the music player's: the page stays, and the next song starts.
            if crate::music::active() {
                return crate::music::on_ended(ctx, weak, &payload);
            }
            let reason = payload["reason"].as_str().unwrap_or("").to_string();
            let message = payload["message"].as_str().map(str::to_string);
            if reason == "error" {
                let (trailer, id) = {
                    let w = watching();
                    (w.trailer, w.item_id.clone())
                };
                if trailer {
                    return recover_trailer(ctx, weak, id, message.unwrap_or_else(|| "it stopped with an error".into()));
                }
                let text = match message {
                    Some(m) => format!("The player reported: {m}."),
                    None => "Playback stopped with an error.".to_string(),
                };
                COUNT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                watching().countdown = None;
                return ui(weak, move |app| {
                    let p = app.global::<Player>();
                    p.set_up_next_kind("".into());
                    p.set_error(text.into());
                });
            }
            let (has_next, over_credits) = {
                let w = watching();
                (w.up_next.is_some(), w.countdown.as_ref().is_some_and(|c| c.kind == "credits"))
            };
            if !has_next {
                return back(ctx, weak.clone());
            }
            if over_credits {
                // Already counting down over the credits: carry on, in the full panel.
                if let Some(c) = watching().countdown.as_mut() {
                    c.kind = "end";
                }
                ui(weak, |app| app.global::<Player>().set_up_next_kind("end".into()));
            } else {
                begin_countdown(&ctx, weak, "end");
            }
        }
        _ => {}
    }
}

fn on_state(ctx: Option<Ctx>, weak: &Weak<App>, payload: &Value) {
    let num = |key: &str| payload[key].as_f64();
    let (position, duration, cache_end, volume, speed) = (num("position"), num("duration"), num("cacheEnd"), num("volume"), num("speed"));
    let (paused, buffering, started, muted) = (payload["paused"].as_bool(), payload["buffering"].as_bool(), payload["started"].as_bool(), payload["muted"].as_bool());
    let decoder = payload["decoder"].as_str().map(str::to_string);
    let (audio, subtitle) = (payload["audioTrack"].as_i64(), payload["subtitleTrack"].as_i64());

    let (skip, offer, withdraw, tracks_changed) = {
        let mut w = watching();
        if let Some(p) = position {
            w.position = p;
        }
        if let Some(d) = duration {
            w.duration = d;
        }
        w.started = started.unwrap_or(false);
        w.paused = paused.unwrap_or(false);
        let tracks_changed = (w.audio, w.subtitle) != (audio, subtitle);
        w.audio = audio;
        w.subtitle = subtitle;
        // Nothing is offered to skip while the next episode's offer is up.
        let skip = if w.countdown.is_some() { None } else { current_segment(&w).map(|s| skip_label(s, w.up_next.is_some()).to_string()) };
        // The offer comes as the closing credits start, unless "Watch credits" was chosen; seeking
        // back out of the credits takes it away again.
        let in_credits = w.started && w.up_next.is_some() && credits_start(&w).is_some_and(|at| w.position >= at);
        let offer = in_credits && w.countdown.is_none() && !w.credits_dismissed;
        let withdraw = !in_credits && w.countdown.as_ref().is_some_and(|c| c.kind == "credits");
        if withdraw {
            w.countdown = None;
        }
        (skip, offer, withdraw, tracks_changed)
    };
    if offer {
        if let Some(ctx) = &ctx {
            begin_countdown(ctx, weak, "credits");
        }
    }
    if withdraw {
        COUNT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    if tracks_changed {
        show_tracks(weak);
    }
    ui(weak, move |app| {
        if let Some(p) = position {
            app.set_position(p as f32);
        }
        if let Some(d) = duration {
            app.set_duration(d as f32);
        }
        app.set_cache_end(cache_end.unwrap_or(0.0) as f32);
        if let Some(v) = volume {
            app.set_volume(v as f32);
        }
        if let Some(s) = speed {
            app.set_speed(s as f32);
        }
        app.set_paused(paused.unwrap_or(false));
        app.set_buffering(buffering.unwrap_or(false));
        app.set_started(started.unwrap_or(false));
        app.set_muted(muted.unwrap_or(false));
        app.set_skip_label(skip.unwrap_or_default().into());
        let p = app.global::<Player>();
        p.set_decoder_text(if started.unwrap_or(false) { decoder_label(decoder.as_deref()) } else { String::new() }.into());
        if withdraw {
            p.set_up_next_kind("".into());
        }
        app.set_time_text(format!("{} / {}", text::clock(app.get_position() as f64), text::clock(app.get_duration() as f64)).into());
    });
}

/// Rebuilds the audio and subtitle lists from the tracks and the selection last heard of.
fn show_tracks(weak: &Weak<App>) {
    let (audio, mut subtitles) = {
        let w = watching();
        // The core's choice where it has made one for that kind of track, else what mpv selected.
        let of_kind = |audio: bool, chosen: Option<i64>| -> Vec<(i32, String, String, bool)> {
            let tracks: Vec<&TrackInfo> = w.tracks.iter().filter(|t| t.audio == audio).collect();
            let any_selected = tracks.iter().any(|t| t.selected);
            tracks
                .iter()
                .map(|t| {
                    let current = match chosen {
                        Some(id) => id == t.id,
                        None => t.selected || (!any_selected && audio && t.default),
                    };
                    (t.id as i32, t.label.clone(), t.detail.clone(), current)
                })
                .collect()
        };
        (of_kind(true, w.audio), of_kind(false, w.subtitle))
    };
    // "Off" leads the subtitles, and is the choice when none is selected.
    let none_selected = !subtitles.iter().any(|s| s.3);
    subtitles.insert(0, (-1, "Off".to_string(), String::new(), none_selected));
    ui(weak, move |app| {
        let rows = |v: Vec<(i32, String, String, bool)>| ModelRc::new(VecModel::from(v.into_iter().map(|(id, label, detail, selected)| TrackData { id, label: label.into(), detail: detail.into(), selected }).collect::<Vec<_>>()));
        app.global::<Player>().set_subtitle_selected(!none_selected);
        app.set_audio_tracks(rows(audio));
        app.set_subtitle_tracks(rows(subtitles));
    });
}

fn track_info(t: &Value) -> Option<TrackInfo> {
    let audio = match t["kind"].as_str()? {
        "audio" => true,
        "sub" => false,
        _ => return None,
    };
    Some(TrackInfo {
        id: t["id"].as_i64()?,
        audio,
        label: track_lines(t).0,
        selected: t["selected"].as_bool().unwrap_or(false),
        default: t["default"].as_bool().unwrap_or(false),
        forced: t["forced"].as_bool().unwrap_or(false),
        detail: track_lines(t).1,
    })
}

/// The video's codec, size and rate, and the audio's codec and layout: one line.
fn readout_of(tracks: &[Value]) -> String {
    let selected = |kind: &str| tracks.iter().find(|t| t["kind"].as_str() == Some(kind) && t["selected"].as_bool().unwrap_or(false)).or_else(|| tracks.iter().find(|t| t["kind"].as_str() == Some(kind)));
    let mut parts = Vec::new();
    if let Some(v) = selected("video") {
        parts.extend(v["codec"].as_str().map(codec_label));
        if let (Some(w), Some(h)) = (v["width"].as_i64(), v["height"].as_i64()) {
            parts.push(format!("{w}\u{d7}{h}"));
        }
        if let Some(fps) = v["fps"].as_f64().filter(|f| *f > 0.0) {
            parts.push(format!("{} fps", (fps * 1000.0).round() / 1000.0));
        }
    }
    if let Some(a) = selected("audio") {
        if let Some(codec) = a["codec"].as_str() {
            parts.push(match a["channels"].as_i64() {
                Some(c) => format!("{} {}", codec_label(codec), channel_label(c)),
                None => codec_label(codec),
            });
        }
    }
    parts.join(", ")
}

/// A codec's usual name.
pub fn codec_label(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "hevc" => "HEVC".into(),
        "h264" => "H.264".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "mpeg2video" => "MPEG-2".into(),
        "aac" => "AAC".into(),
        "ac3" => "AC-3".into(),
        "eac3" => "E-AC-3".into(),
        "truehd" => "TrueHD".into(),
        "dts" => "DTS".into(),
        "flac" => "FLAC".into(),
        "opus" => "Opus".into(),
        "mp3" => "MP3".into(),
        "vorbis" => "Vorbis".into(),
        "subrip" => "SRT".into(),
        "ass" => "ASS".into(),
        "ssa" => "SSA".into(),
        "webvtt" => "WebVTT".into(),
        "mov_text" => "Timed text".into(),
        "hdmv_pgs_subtitle" | "pgssub" => "PGS".into(),
        "dvd_subtitle" | "dvdsub" => "VobSub".into(),
        "dvb_subtitle" => "DVB".into(),
        other => other.to_uppercase(),
    }
}

/// "mono", "2.0", "5.1", "7.1", or "N ch".
fn channel_label(count: i64) -> String {
    match count {
        1 => "mono".into(),
        2 => "2.0".into(),
        6 => "5.1".into(),
        8 => "7.1".into(),
        n => format!("{n} ch"),
    }
}

/// What the picture is decoded by.
fn decoder_label(decoder: Option<&str>) -> String {
    match decoder.filter(|d| !d.is_empty() && *d != "no") {
        None => "decoded in software".into(),
        Some(d) if d.starts_with("nvdec") || d.starts_with("cuda") => "decoded locally via NVDEC".into(),
        Some(d) if d.starts_with("vaapi") => "decoded locally via VA-API".into(),
        Some(d) if d.starts_with("vulkan") => "decoded locally via Vulkan".into(),
        Some(d) => format!("decoded locally via {d}"),
    }
}

/// Where the closing credits start: the first credits stretch in the last 40% of the file. An
/// opening song marked as credits isn't the end of the episode.
fn credits_start(w: &Watching) -> Option<f64> {
    if w.duration <= 0.0 {
        return None;
    }
    w.segments.iter().find(|s| s.kind == "credits" && s.start_seconds >= w.duration * CREDITS_AFTER).map(|s| s.start_seconds)
}

/// "Japanese  •  Stereo  •  AAC": what tells one track from another.
/// A track as two lines: its language, then what tells it apart (its own title, the format, flags).
fn track_lines(t: &Value) -> (String, String) {
    let lang = t["lang"].as_str().map(crate::text::language);
    let layout = t["channels"].as_i64().map(channels);
    // A title that only says what the language or the channel layout already say is left out.
    let repeats = |s: &str| Some(s) == lang.as_deref() || Some(s) == layout.as_deref();
    let mut parts: Vec<String> = Vec::new();
    if let Some(title) = t["title"].as_str().filter(|s| !s.is_empty() && !repeats(s)) {
        parts.push(title.to_string());
    }
    let mut tech = Vec::new();
    tech.extend(t["codec"].as_str().map(|c| c.to_uppercase()));
    tech.extend(layout);
    if !tech.is_empty() {
        parts.push(tech.join(" "));
    }
    for (flag, word) in [("forced", "forced"), ("hearingImpaired", "SDH"), ("external", "external")] {
        if t[flag].as_bool() == Some(true) {
            parts.push(word.to_string());
        }
    }
    (lang.unwrap_or_else(|| "Unknown".into()), parts.join("  \u{2022}  "))
}

fn channels(count: i64) -> String {
    match count {
        1 => "Mono".into(),
        2 => "Stereo".into(),
        6 => "5.1".into(),
        8 => "7.1".into(),
        n => format!("{n}ch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_clock_reads_minutes_then_hours() {
        assert_eq!(text::clock(0.0), "0:00");
        assert_eq!(text::clock(65.4), "1:05");
        assert_eq!(text::clock(3600.0), "1:00:00");
        assert_eq!(text::clock(5025.9), "1:23:45");
    }

    #[test]
    fn a_clock_given_nonsense_reads_zero() {
        assert_eq!(text::clock(f64::NAN), "0:00");
        assert_eq!(text::clock(-5.0), "0:00");
    }

    #[test]
    fn a_track_is_told_apart_by_language_title_and_format() {
        let t = json!({"kind": "audio", "id": 2, "lang": "jpn", "codec": "aac", "channels": 2});
        assert_eq!(track_lines(&t), ("Japanese".to_string(), "AAC Stereo".to_string()));
        let t = json!({"kind": "sub", "id": 3, "lang": "eng", "title": "Signs & Songs", "codec": "ass", "forced": true});
        assert_eq!(track_lines(&t), ("English".to_string(), "Signs & Songs  \u{2022}  ASS  \u{2022}  forced".to_string()));
    }

    #[test]
    fn a_title_that_only_repeats_the_language_or_layout_is_left_out() {
        let t = json!({"kind": "sub", "id": 3, "lang": "eng", "title": "English"});
        assert_eq!(track_lines(&t), ("English".to_string(), String::new()));
        let t = json!({"kind": "audio", "id": 1, "lang": "jpn", "title": "Stereo", "codec": "aac", "channels": 2});
        assert_eq!(track_lines(&t), ("Japanese".to_string(), "AAC Stereo".to_string()));
    }

    #[test]
    fn a_track_with_no_language_is_unknown_and_an_unlisted_code_is_shown_as_is() {
        assert_eq!(track_lines(&json!({"kind": "audio", "id": 1})).0, "Unknown");
        assert_eq!(crate::text::language("xx"), "XX");
    }

    fn watching_at(position: f64) -> Watching {
        Watching {
            position,
            duration: 1500.0,
            started: true,
            segments: vec![
                Segment { kind: "intro".into(), start_seconds: 10.0, end_seconds: 100.0 },
                Segment { kind: "credits".into(), start_seconds: 1300.0, end_seconds: 1450.0 },
                Segment { kind: "preview".into(), start_seconds: 1450.0, end_seconds: 1490.0 },
            ],
            ..Watching::default()
        }
    }

    #[test]
    fn a_skip_is_offered_only_inside_a_skippable_stretch() {
        assert!(current_segment(&watching_at(5.0)).is_none());
        assert_eq!(current_segment(&watching_at(50.0)).map(|s| skip_label(s, false)), Some("Skip intro"));
        assert_eq!(current_segment(&watching_at(1400.0)).map(|s| skip_label(s, false)), Some("Skip credits"));
        // Over the credits the button goes on to the next episode when there is one.
        assert_eq!(current_segment(&watching_at(1400.0)).map(|s| skip_label(s, true)), Some("Next episode"));
        assert_eq!(current_segment(&watching_at(1460.0)).map(|s| skip_label(s, true)), Some("Skip preview"));
        // The last second of a stretch isn't worth a button.
        assert!(current_segment(&watching_at(99.5)).is_none());
        // Nothing is offered before the picture has started.
        let mut early = watching_at(50.0);
        early.started = false;
        early.item_id = "x".into();
        assert!(current_segment(&early).is_none());
    }

    #[test]
    fn closing_credits_start_in_the_last_forty_percent() {
        assert_eq!(credits_start(&watching_at(0.0)), Some(1300.0));
        // An opening song marked as credits isn't the end.
        let mut early = watching_at(0.0);
        early.segments = vec![Segment { kind: "credits".into(), start_seconds: 20.0, end_seconds: 100.0 }];
        assert_eq!(credits_start(&early), None);
        assert_eq!(credits_start(&Watching::default()), None);
    }

    #[test]
    fn codecs_channels_and_decoders_are_named_as_the_original_names_them() {
        assert_eq!(codec_label("hevc"), "HEVC");
        assert_eq!(codec_label("eac3"), "E-AC-3");
        assert_eq!(codec_label("hdmv_pgs_subtitle"), "PGS");
        assert_eq!(codec_label("weird"), "WEIRD");
        assert_eq!(channel_label(6), "5.1");
        assert_eq!(channel_label(3), "3 ch");
        assert_eq!(decoder_label(None), "decoded in software");
        assert_eq!(decoder_label(Some("no")), "decoded in software");
        assert_eq!(decoder_label(Some("nvdec-copy")), "decoded locally via NVDEC");
        assert_eq!(decoder_label(Some("vaapi")), "decoded locally via VA-API");
    }

    #[test]
    fn the_readout_is_video_then_audio() {
        let tracks = vec![
            json!({"kind": "video", "selected": true, "codec": "hevc", "width": 1920, "height": 1080, "fps": 23.976}),
            json!({"kind": "audio", "selected": true, "codec": "aac", "channels": 2}),
        ];
        assert_eq!(readout_of(&tracks), "HEVC, 1920\u{d7}1080, 23.976 fps, AAC 2.0");
        assert_eq!(readout_of(&[]), "");
    }
}

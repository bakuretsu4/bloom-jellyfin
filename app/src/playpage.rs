//! What the player shows besides the picture: the sizes it comes in, where the picture goes, the
//! details under it, the list beside it, and the thumbnail that rides over the seek bar.
//!
//! The picture itself is mpv's, drawn under the window. The screen leaves a box clear for it and
//! reports the box's place, which is handed to the player as margins (see `mpv::view`).

use crate::cards;
use crate::downloads;
use crate::text;
use crate::title::{self, Detail, Img};
use crate::watch;
use crate::{App, MetaPart, PersonData, Player, QueueData};
use bloom_core::jellyfin::Jellyfin;
use bloom_core::{home, library, Ctx};
use serde::Deserialize;
use slint::{ComponentHandle, Image, Model, ModelRc, SharedPixelBuffer, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

// ---- the sizes and the box

/// Remembered when going full screen, so leaving it goes back to theater if it came from there.
static THEATER_BEFORE_FULLSCREEN: AtomicBool = AtomicBool::new(false);

/// Changes the size of the player: "default", "theater" or "fullscreen". UI thread.
pub fn set_mode(weak: &Weak<App>, requested: &str) {
    let Some(app) = weak.upgrade() else { return };
    let player = app.global::<Player>();
    let current = player.get_mode().to_string();
    let next = match (requested, current.as_str()) {
        (r, c) if r == c => return,
        ("fullscreen", c) => {
            THEATER_BEFORE_FULLSCREEN.store(c == "theater", Ordering::SeqCst);
            "fullscreen"
        }
        // Leaving full screen goes back to where it came from.
        ("default", "fullscreen") if THEATER_BEFORE_FULLSCREEN.swap(false, Ordering::SeqCst) => "theater",
        (r, "fullscreen") => r,
        ("theater", _) => "theater",
        _ => "default",
    };
    app.window().set_fullscreen(next == "fullscreen");
    player.set_mode(next.into());
}

/// The player leaves the screen: the size goes back to the default.
pub fn reset_mode(app: &App) {
    app.window().set_fullscreen(false);
    THEATER_BEFORE_FULLSCREEN.store(false, Ordering::SeqCst);
    app.global::<Player>().set_mode("default".into());
}

/// The box's place in the window, in logical pixels: handed on as the fractions of the window left
/// free on each side. UI thread.
pub fn viewport(ctx: &Ctx, x: f32, y: f32, w: f32, h: f32, window_w: f32, window_h: f32) {
    if window_w <= 0.0 || window_h <= 0.0 {
        return;
    }
    let sides = sides(x, y, w, h, window_w, window_h);
    // No picture yet (the page has just opened) is not an error worth telling anyone about.
    let _ = ctx.shell().set_viewport(sides[0], sides[1], sides[2], sides[3], f64::from(window_w) / f64::from(window_h));
}

/// Left, top, right and bottom: how much of the window is left free beside a box, as fractions.
pub fn sides(x: f32, y: f32, w: f32, h: f32, window_w: f32, window_h: f32) -> [f64; 4] {
    let (x, y, w, h, ww, wh) = (f64::from(x), f64::from(y), f64::from(w), f64::from(h), f64::from(window_w), f64::from(window_h));
    [x / ww, y / wh, (ww - x - w) / ww, (wh - y - h) / wh]
}

// ---- the details under the picture

/// What's on the page now, so actions can change the right rows. UI thread only.
struct Page {
    item_id: String,
    series_id: String,
    queue: Rc<VecModel<QueueData>>,
    next: Rc<VecModel<QueueData>>,
    /// The seasons' ids, as the select lists them.
    seasons: Vec<String>,
}

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

fn page<R>(f: impl FnOnce(&mut Page) -> R) -> Option<R> {
    PAGE.with(|p| p.borrow_mut().as_mut().map(f))
}

fn part(text: impl Into<SharedString>) -> MetaPart {
    MetaPart { text: text.into(), good: false }
}

/// The facts after the title: its line in the show, length, genres, score.
fn meta_parts(subtitle: &str, d: &Detail) -> Vec<MetaPart> {
    let mut parts = Vec::new();
    if !subtitle.is_empty() {
        parts.push(part(subtitle));
    }
    if let Some(minutes) = d.runtime_minutes {
        parts.push(part(text::runtime(minutes)));
    }
    if !d.genres.is_empty() {
        parts.push(part(d.genres.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    if let Some(rating) = d.community_rating {
        parts.push(part(format!("{rating:.1}/10")));
    }
    parts
}

/// The page for what has just started playing: the details, and the list beside the picture.
/// `now_subtitle` is the line the core gave ("S1 E1, Pilot"). UI thread.
pub fn show_details(ctx: &Ctx, weak: &Weak<App>, app: &App, item_id: &str, now_subtitle: &str, detail: Option<Detail>, generation: u64) {
    let player = app.global::<Player>();
    let Some(detail) = detail else {
        clear_details(app, item_id);
        return;
    };
    player.set_meta(ModelRc::new(VecModel::from(meta_parts(now_subtitle, &detail))));
    player.set_rating(detail.official_rating.clone().unwrap_or_default().into());
    player.set_favorite(detail.favorite);
    player.set_played(detail.played);
    player.set_overview(detail.overview.clone().unwrap_or_default().into());
    player.set_media(ModelRc::new(VecModel::from(title::media_rows(&detail))));
    let download = downloads::of_item(item_id);
    player.set_download_label(title::download_label(download.as_ref()).into());
    player.set_download_status(download.map(|d| d.0).unwrap_or_default().into());

    match &detail.series {
        Some(series) => {
            player.set_has_owner(true);
            player.set_owner_name(series.title.clone().into());
            player.set_owner_sub(detail.season_name.clone().unwrap_or_default().into());
            if let Some(poster) = &series.poster {
                cards::fetch_one(ctx, weak, generation, poster.path(120), |app, image| {
                    let p = app.global::<Player>();
                    p.set_owner_image(image);
                    p.set_owner_has_image(true);
                });
            }
        }
        None => player.set_has_owner(false),
    }

    let cast = Rc::new(VecModel::from(
        detail
            .people
            .iter()
            .take(40)
            .map(|p| PersonData { id: p.id.clone().into(), name: p.name.clone().into(), role: p.role.clone().unwrap_or_default().into(), initial: text::initial(&p.name).into(), image: Image::default(), has_image: false })
            .collect::<Vec<_>>(),
    ));
    let tracker = cards::track(&cast, |row: &mut PersonData, image| {
        row.image = image;
        row.has_image = true;
    });
    player.set_cast(ModelRc::from(cast));
    let jobs = detail.people.iter().take(40).enumerate().filter_map(|(i, p)| p.image.as_ref().map(|img: &Img| (i, img.path(240)))).collect();
    cards::fetch_rows(ctx, weak, tracker, jobs);

    // The list beside the picture.
    let series_id = detail.series.as_ref().map(|s| s.id.clone()).unwrap_or_default();
    PAGE.with(|p| {
        *p.borrow_mut() = Some(Page { item_id: item_id.to_string(), series_id: series_id.clone(), queue: Rc::new(VecModel::default()), next: Rc::new(VecModel::default()), seasons: Vec::new() })
    });
    player.set_queue_loading(true);
    let (ctx, weak, item) = (ctx.clone(), weak.clone(), item_id.to_string());
    let is_episode = detail.kind == "Episode" && !series_id.is_empty();
    let (season, series_title) = (detail.season_id.clone().unwrap_or_default(), detail.series.as_ref().map(|s| s.title.clone()).unwrap_or_default());
    bloom_core::rt::spawn(async move {
        if is_episode {
            load_season(ctx, weak, item, series_id, season, series_title, generation).await;
        } else {
            load_similar(ctx, weak, item, generation).await;
        }
    });
}

/// With no detail to go on (a trailer, say) the page is only the picture.
fn clear_details(app: &App, item_id: &str) {
    let player = app.global::<Player>();
    player.set_meta(ModelRc::new(VecModel::<MetaPart>::default()));
    player.set_rating("".into());
    player.set_has_owner(false);
    player.set_overview("".into());
    player.set_media(ModelRc::new(VecModel::default()));
    player.set_cast(ModelRc::new(VecModel::<PersonData>::default()));
    player.set_queue(ModelRc::new(VecModel::<QueueData>::default()));
    player.set_queue_loading(false);
    PAGE.with(|p| *p.borrow_mut() = Some(Page { item_id: item_id.to_string(), series_id: String::new(), queue: Rc::new(VecModel::default()), next: Rc::new(VecModel::default()), seasons: Vec::new() }));
}

// ---- the list beside the picture

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Entry {
    id: String,
    title: String,
    meta: Option<String>,
    image: Option<Img>,
    runtime_minutes: Option<u32>,
    progress: Option<f64>,
    played: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct NextSeason {
    season_id: String,
    name: String,
    entries: Vec<Entry>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SeasonList {
    seasons: Vec<title::Season>,
    season_id: String,
    entries: Vec<Entry>,
    next: Option<NextSeason>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Similar {
    heading: String,
    entries: Vec<Entry>,
}

fn row(e: &Entry, current: &str) -> QueueData {
    QueueData {
        id: e.id.clone().into(),
        title: e.title.clone().into(),
        meta: e.meta.clone().unwrap_or_default().into(),
        runtime: e.runtime_minutes.map(text::runtime).unwrap_or_default().into(),
        progress: if e.played { 0.0 } else { e.progress.unwrap_or(0.0) as f32 },
        played: e.played,
        current: e.id == current,
        image: Image::default(),
        has_image: false,
    }
}

/// A list's rows, with their pictures on the way. UI thread.
fn set_rows(ctx: &Ctx, weak: &Weak<App>, entries: &[Entry], current: &str) -> Rc<VecModel<QueueData>> {
    let model = Rc::new(VecModel::from(entries.iter().map(|e| row(e, current)).collect::<Vec<_>>()));
    let tracker = cards::track(&model, |row: &mut QueueData, image| {
        row.image = image;
        row.has_image = true;
    });
    let jobs = entries.iter().enumerate().filter_map(|(i, e)| e.image.as_ref().map(|img| (i, img.path(320)))).collect();
    cards::fetch_rows(ctx, weak, tracker, jobs);
    model
}

/// "Season 1" and its episodes, the start of the next season, and the picker.
async fn load_season(ctx: Ctx, weak: Weak<App>, item: String, series_id: String, season_id: String, series_title: String, generation: u64) {
    let jf = ctx.state::<Jellyfin>();
    let result = library::season_list_with(&jf, &series_id, &season_id).await;
    let list: Option<SeasonList> = result.ok().and_then(|l| serde_json::to_value(l).ok()).and_then(|v| serde_json::from_value(v).ok());
    // With no server, the downloaded episodes of the show are what there is to list.
    let offline: Vec<Entry> = if list.is_none() { downloaded_episodes(&series_id) } else { Vec::new() };
    let (ctx2, weak2) = (ctx.clone(), weak.clone());
    let _ = weak.upgrade_in_event_loop(move |app| {
        let _ = generation;
        // Another file may have started since this was asked for.
        if watch::current_item() != item {
            return;
        }
        let player = app.global::<Player>();
        player.set_queue_loading(false);
        let Some(list) = list else {
            player.set_queue_heading("Episodes".into());
            player.set_queue_eyebrow(series_title.into());
            player.set_queue_seasons(ModelRc::new(VecModel::<SharedString>::default()));
            player.set_queue_empty("Couldn't load this season's episodes.".into());
            let queue = set_rows(&ctx2, &weak2, &offline, &item);
            player.set_queue(ModelRc::from(queue.clone()));
            player.set_queue_next_name("".into());
            player.set_queue_next(ModelRc::new(VecModel::<QueueData>::default()));
            page(|p| p.queue = queue);
            return;
        };
        let index = list.seasons.iter().position(|s| s.id == list.season_id).unwrap_or(0);
        let names: Vec<SharedString> = list.seasons.iter().map(|s| SharedString::from(s.name.as_str())).collect();
        player.set_queue_heading(list.seasons.get(index).map(|s| s.name.clone()).unwrap_or_else(|| "Episodes".into()).into());
        player.set_queue_eyebrow(series_title.into());
        player.set_queue_seasons(ModelRc::new(VecModel::from(names)));
        player.set_queue_season_index(index as i32);
        player.set_queue_empty("".into());
        let queue = set_rows(&ctx2, &weak2, &list.entries, &item);
        player.set_queue(ModelRc::from(queue.clone()));
        player.set_queue_current(list.entries.iter().position(|e| e.id == item).map(|i| i as i32).unwrap_or(-1));
        let (next_name, next_rows) = match &list.next {
            Some(next) => (next.name.clone(), set_rows(&ctx2, &weak2, &next.entries, &item)),
            None => (String::new(), Rc::new(VecModel::default())),
        };
        player.set_queue_next_name(next_name.into());
        player.set_queue_next(ModelRc::from(next_rows.clone()));
        let next_season = list.next.as_ref().map(|n| n.season_id.clone());
        page(|p| {
            p.queue = queue;
            p.next = next_rows;
            p.seasons = list.seasons.iter().map(|s| s.id.clone()).collect();
        });
        NEXT_SEASON.with(|n| *n.borrow_mut() = next_season);
    });
}

thread_local! {
    /// The season "All of ..." goes to. UI thread.
    static NEXT_SEASON: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The show's finished downloads, in order: what can be played with no server.
fn downloaded_episodes(series_id: &str) -> Vec<Entry> {
    downloads::episodes_of(series_id)
        .into_iter()
        .map(|d| Entry {
            id: d.item_id,
            title: d.title,
            meta: d.subtitle,
            image: d.image,
            runtime_minutes: d.runtime_seconds.map(|s| (s / 60.0).round() as u32).filter(|m| *m > 0),
            progress: None,
            played: d.played,
        })
        .collect()
}

/// For a film, what is like it.
async fn load_similar(ctx: Ctx, weak: Weak<App>, item: String, generation: u64) {
    let jf = ctx.state::<Jellyfin>();
    let queue: Option<Similar> = library::watch_queue_with(&jf, &item).await.ok().and_then(|q| serde_json::to_value(q).ok()).and_then(|v| serde_json::from_value(v).ok());
    let (ctx2, weak2) = (ctx.clone(), weak.clone());
    let _ = weak.upgrade_in_event_loop(move |app| {
        let _ = generation;
        if watch::current_item() != item {
            return;
        }
        let player = app.global::<Player>();
        player.set_queue_loading(false);
        let queue = queue.unwrap_or_default();
        player.set_queue_heading(if queue.heading.is_empty() { "More like this".into() } else { SharedString::from(queue.heading.as_str()) });
        player.set_queue_eyebrow("".into());
        player.set_queue_seasons(ModelRc::new(VecModel::<SharedString>::default()));
        player.set_queue_empty("The server found nothing similar in the library.".into());
        player.set_queue_next_name("".into());
        let rows = set_rows(&ctx2, &weak2, &queue.entries, &item);
        player.set_queue(ModelRc::from(rows.clone()));
        player.set_queue_current(-1);
        page(|p| p.queue = rows);
    });
}

/// An entry was pressed: it plays, unless it's what is already playing.
pub fn queue_play(ctx: Ctx, weak: Weak<App>, id: String) {
    if id != watch::current_item() {
        watch::play(ctx, weak, id);
    }
}

/// An entry's watched state: toggled on the server, then shown.
pub fn queue_toggle_played(ctx: Ctx, weak: Weak<App>, id: String) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |_| {
        let Some(played) = page(|p| p.queue.iter().chain(p.next.iter()).find(|r| r.id == id.as_str()).map(|r| !r.played)).flatten() else { return };
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            match library::set_played_with(&jf, &id, played).await {
                Ok(now) => {
                    let _ = weak.upgrade_in_event_loop(move |_| {
                        page(|p| {
                            for model in [&p.queue, &p.next] {
                                for i in 0..model.row_count() {
                                    if let Some(mut r) = model.row_data(i).filter(|r| r.id == id.as_str()) {
                                        r.played = now;
                                        if now {
                                            r.progress = 0.0;
                                        }
                                        model.set_row_data(i, r);
                                    }
                                }
                            }
                        });
                    });
                }
                Err(e) => watch::say(&weak, e.to_string()),
            }
        });
    });
}

/// A season was picked in the list's header.
pub fn queue_season(ctx: Ctx, weak: Weak<App>, index: i32) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let Some((series, season, item)) = page(|p| (p.series_id.clone(), usize::try_from(index).ok().and_then(|i| p.seasons.get(i).cloned()), p.item_id.clone())).and_then(|(s, season, i)| season.map(|x| (s, x, i))) else { return };
        let title = app.global::<Player>().get_queue_eyebrow().to_string();
        app.global::<Player>().set_queue_loading(true);
        bloom_core::rt::spawn(async move { load_season(ctx, weak, item, series, season, title, cards::generation()).await });
    });
}

/// "All of {next season}".
pub fn queue_next_season(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let Some(season) = NEXT_SEASON.with(|n| n.borrow().clone()) else { return };
        let Some((series, item)) = page(|p| (p.series_id.clone(), p.item_id.clone())) else { return };
        let title = app.global::<Player>().get_queue_eyebrow().to_string();
        app.global::<Player>().set_queue_loading(true);
        bloom_core::rt::spawn(async move { load_season(ctx, weak, item, series, season, title, cards::generation()).await });
    });
}

// ---- actions under the picture

pub fn toggle_favorite(ctx: Ctx, weak: Weak<App>) {
    let id = watch::current_item();
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let favorite = !app.global::<Player>().get_favorite();
        app.global::<Player>().set_action_error("".into());
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            match home::set_favorite_with(&jf, id.clone(), favorite).await {
                Ok(now) => {
                    crate::hover::forget(&id);
                    let _ = weak.upgrade_in_event_loop(move |app| app.global::<Player>().set_favorite(now));
                }
                Err(e) => {
                    let message = e.to_string();
                    let _ = weak.upgrade_in_event_loop(move |app| app.global::<Player>().set_action_error(message.into()));
                }
            }
        });
    });
}

pub fn toggle_played(ctx: Ctx, weak: Weak<App>) {
    let id = watch::current_item();
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let played = !app.global::<Player>().get_played();
        app.global::<Player>().set_action_error("".into());
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            match library::set_played_with(&jf, &id, played).await {
                Ok(now) => {
                    crate::hover::forget(&id);
                    let _ = weak.upgrade_in_event_loop(move |app| app.global::<Player>().set_played(now));
                }
                Err(e) => {
                    let message = e.to_string();
                    let _ = weak.upgrade_in_event_loop(move |app| app.global::<Player>().set_action_error(message.into()));
                }
            }
        });
    });
}

/// The download button: queues it, or goes to the Downloads screen once it is listed.
pub fn download(ctx: Ctx, weak: Weak<App>) {
    let id = watch::current_item();
    if downloads::of_item(&id).is_some() {
        watch::leave_to(ctx, weak, crate::nav::Route::Downloads);
    } else {
        downloads::download_item(ctx, weak, id);
    }
}

/// The downloads changed: the button says how this one is doing. UI thread.
pub fn refresh_downloads(app: &App) {
    if app.get_screen() != crate::Screen::Watch {
        return;
    }
    let download = downloads::of_item(&watch::current_item());
    let player = app.global::<Player>();
    player.set_download_label(title::download_label(download.as_ref()).into());
    player.set_download_status(download.map(|d| d.0).unwrap_or_default().into());
}

/// The show or season the episode belongs to: playback ends and its page opens.
pub fn open_owner(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |_| {
        let Some(series) = page(|p| p.series_id.clone()).filter(|s| !s.is_empty()) else { return };
        watch::leave_to(ctx, weak, crate::nav::Route::Title(series));
    });
}

pub fn open_person(ctx: Ctx, weak: Weak<App>, id: String, name: String) {
    watch::leave_to(ctx, weak, crate::nav::Route::Person { id, name });
}

// ---- the thumbnail over the seek bar

/// A thumbnail's place on the server: its tile, and which one in it.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Trickplay {
    pub item_id: String,
    pub media_source_id: String,
    pub width: u32,
    pub height: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub count: u32,
    pub interval_ms: u32,
}

impl Trickplay {
    /// The thumbnail for `seconds` into the file, as the path the core's image fetch takes.
    pub fn path(&self, seconds: f64) -> Option<(u32, String)> {
        if self.count == 0 || self.interval_ms == 0 || self.tile_width == 0 || self.tile_height == 0 {
            return None;
        }
        let index = ((seconds.max(0.0) * 1000.0 / f64::from(self.interval_ms)) as u32).min(self.count - 1);
        let per_tile = self.tile_width * self.tile_height;
        let (tile, n) = (index / per_tile, index % per_tile);
        Some((index, format!("/trickplay/{}/{}/{}/{}x{}/{}/{}", self.item_id, self.media_source_id, self.width, self.tile_width, self.tile_height, tile, n)))
    }
}

/// The name of the chapter `seconds` falls in.
pub fn chapter_at<'a>(starts: &[f64], names: &'a [String], seconds: f64) -> Option<&'a str> {
    let i = starts.iter().rposition(|s| *s <= seconds)?;
    names.get(i).map(String::as_str).filter(|n| !n.is_empty())
}

/// Thumbnails already fetched, so moving back over a spot is instant.
type Thumbs = HashMap<(String, u32), SharedPixelBuffer<slint::Rgba8Pixel>>;
static THUMBS: std::sync::LazyLock<Mutex<Thumbs>> = std::sync::LazyLock::new(Mutex::default);
/// The latest hover, so a thumbnail that arrives late for an earlier spot is dropped.
static PREVIEW_SEQ: AtomicU64 = AtomicU64::new(0);

/// The pointer is over the seek bar at `fraction` (or has left: negative). UI thread.
pub fn scrub_hover(ctx: Ctx, weak: Weak<App>, fraction: f32) {
    let Some(app) = weak.upgrade() else { return };
    let player = app.global::<Player>();
    let seq = PREVIEW_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    if fraction < 0.0 {
        player.set_preview_visible(false);
        return;
    }
    let (duration, starts, names, trickplay) = watch::preview_facts();
    if duration <= 0.0 {
        return;
    }
    let seconds = f64::from(fraction) * duration;
    player.set_preview_fraction(fraction);
    player.set_preview_time(text::clock(seconds).into());
    player.set_preview_chapter(chapter_at(&starts, &names, seconds).unwrap_or("").into());
    player.set_preview_visible(true);
    let Some((index, path)) = trickplay.as_ref().and_then(|t| t.path(seconds).map(|p| (p.0, p.1))) else {
        player.set_preview_has_image(false);
        return;
    };
    let key = (trickplay.map(|t| t.item_id).unwrap_or_default(), index);
    let cached = THUMBS.lock().unwrap_or_else(|e| e.into_inner()).get(&key).cloned();
    if let Some(pixels) = cached {
        player.set_preview_image(Image::from_rgba8(pixels));
        player.set_preview_has_image(true);
        return;
    }
    bloom_core::rt::spawn(async move {
        let Some(pixels) = cards::pixels(&ctx, &path).await else { return };
        {
            let mut cache = THUMBS.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() > 120 {
                cache.clear();
            }
            cache.insert(key, pixels.clone());
        }
        let _ = weak.upgrade_in_event_loop(move |app| {
            if PREVIEW_SEQ.load(Ordering::SeqCst) == seq {
                let player = app.global::<Player>();
                player.set_preview_image(Image::from_rgba8(pixels));
                player.set_preview_has_image(true);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_inside_the_window_leaves_the_rest_free_on_each_side() {
        let s = sides(100.0, 20.0, 800.0, 450.0, 1000.0, 600.0);
        assert!((s[0] - 0.1).abs() < 1e-9 && (s[1] - 20.0 / 600.0).abs() < 1e-9);
        assert!((s[2] - 0.1).abs() < 1e-9 && (s[3] - 130.0 / 600.0).abs() < 1e-9);
        // Full screen leaves nothing free.
        assert_eq!(sides(0.0, 0.0, 1000.0, 600.0, 1000.0, 600.0), [0.0; 4]);
    }

    fn trickplay() -> Trickplay {
        Trickplay { item_id: "it".into(), media_source_id: "src".into(), width: 320, height: 180, tile_width: 10, tile_height: 10, count: 250, interval_ms: 10_000 }
    }

    #[test]
    fn a_time_finds_its_tile_and_its_place_in_it() {
        let t = trickplay();
        // 10 s a thumbnail: 1234 s is the 123rd, the 24th in the second tile.
        assert_eq!(t.path(1234.0), Some((123, "/trickplay/it/src/320/10x10/1/23".to_string())));
        assert_eq!(t.path(0.0).map(|p| p.0), Some(0));
        // Past the last thumbnail stays on the last.
        assert_eq!(t.path(99_999.0).map(|p| p.0), Some(249));
        assert!(Trickplay::default().path(5.0).is_none());
    }

    #[test]
    fn a_chapter_is_the_last_one_started() {
        let starts = [0.0, 60.0, 300.0];
        let names: Vec<String> = ["Opening", "", "Finale"].iter().map(|s| s.to_string()).collect();
        assert_eq!(chapter_at(&starts, &names, 30.0), Some("Opening"));
        assert_eq!(chapter_at(&starts, &names, 100.0), None);
        assert_eq!(chapter_at(&starts, &names, 301.0), Some("Finale"));
        assert_eq!(chapter_at(&[], &[], 5.0), None);
    }
}

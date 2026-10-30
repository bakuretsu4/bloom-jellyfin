//! A film's, show's or collection's page: the hero, the synopsis and facts, a collection's titles or
//! a show's episodes, cast, media info, the collections a title is part of, and what's similar.
//!
//! The core hands the page's data over as JSON (`library::item_detail`, `season_episodes`), as it
//! does for the Svelte page, and the shapes below are the parts of it this page reads.

use crate::cards::{self, PlainCard};
use crate::downloads;
use crate::text;
use crate::{nav, watch, App, CollectionRow, EpisodeData, FactData, MetaPart, PersonData, Screen, SeasonData, TitleData};
use bloom_core::downloads::Downloads;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::{home, library, Ctx};
use serde::Deserialize;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Img {
    pub(crate) item_id: String,
    pub(crate) kind: String,
    pub(crate) tag: String,
}

impl Img {
    pub fn path(&self, width: u32) -> String {
        format!("/{}/{}/{}/{}", self.item_id, self.kind, self.tag, width)
    }
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Season {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) episode_count: u32,
    pub(crate) unplayed: u32,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct PlayTarget {
    pub(crate) item_id: String,
    pub(crate) label: String,
    pub(crate) resume: bool,
    pub(crate) season_id: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Person {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) role: Option<String>,
    pub(crate) image: Option<Img>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Trailer {
    #[allow(dead_code)]
    pub(crate) name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Media {
    pub(crate) container: Option<String>,
    pub(crate) size_bytes: Option<i64>,
    pub(crate) bitrate: Option<i64>,
    pub(crate) video: Vec<String>,
    pub(crate) audio: Vec<String>,
    pub(crate) subtitles: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Detail {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) year: Option<i32>,
    pub(crate) end_year: Option<i32>,
    pub(crate) status: Option<String>,
    pub(crate) official_rating: Option<String>,
    pub(crate) community_rating: Option<f64>,
    pub(crate) genres: Vec<String>,
    pub(crate) studios: Vec<String>,
    pub(crate) tagline: Option<String>,
    pub(crate) overview: Option<String>,
    pub(crate) backdrop: Option<Img>,
    pub(crate) poster: Option<Img>,
    pub(crate) logo: Option<Img>,
    pub(crate) audio_languages: Vec<String>,
    pub(crate) subtitle_languages: Vec<String>,
    pub(crate) favorite: bool,
    pub(crate) played: bool,
    pub(crate) runtime_minutes: Option<u32>,
    pub(crate) episode_count: Option<u32>,
    pub(crate) unplayed_count: Option<u32>,
    pub(crate) seasons: Vec<Season>,
    pub(crate) play: Option<PlayTarget>,
    pub(crate) people: Vec<Person>,
    pub(crate) media: Option<Media>,
    pub(crate) trailers: Vec<Trailer>,
    /// For an episode: its show.
    pub(crate) series: Option<SeriesRef>,
    pub(crate) season_id: Option<String>,
    pub(crate) season_name: Option<String>,
    pub(crate) chapters: Vec<Chapter>,
}

/// The show an episode belongs to.
#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SeriesRef {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) poster: Option<Img>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Chapter {
    pub(crate) name: Option<String>,
    pub(crate) start_seconds: f64,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Episode {
    pub(crate) id: String,
    pub(crate) number: Option<i32>,
    pub(crate) title: String,
    pub(crate) runtime_minutes: Option<u32>,
    pub(crate) image: Option<Img>,
    pub(crate) played: bool,
    pub(crate) progress: Option<f64>,
    pub(crate) audio_languages: Vec<String>,
    pub(crate) has_subtitles: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EpisodePage {
    episodes: Vec<Episode>,
}

/// What the page on screen keeps between actions. UI thread only.
struct Page {
    title: String,
    seasons: Vec<Season>,
    season_id: String,
    episodes: Vec<Episode>,
    newest_first: bool,
    /// The episode Play would start, and whether that resumes it.
    play_target: Option<(String, bool)>,
    /// For the show's meta line, kept up to date when a season is marked.
    detail_meta: Meta,
    model: Option<Rc<VecModel<EpisodeData>>>,
}

#[derive(Clone, Default)]
pub struct Meta {
    pub kind: String,
    pub year: Option<i32>,
    pub end_year: Option<i32>,
    pub status: Option<String>,
    pub runtime: Option<u32>,
    pub rating: Option<f64>,
    pub season_count: usize,
    pub episode_count: Option<u32>,
    pub unplayed: Option<u32>,
}

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

/// The page last asked for, so "Try again" can ask again.
static LAST: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);

fn page<R>(f: impl FnOnce(&mut Page) -> R) -> Option<R> {
    PAGE.with(|p| p.borrow_mut().as_mut().map(f))
}

fn initial(name: &str) -> SharedString {
    text::initial(name).into()
}

fn part(text: impl Into<SharedString>) -> MetaPart {
    MetaPart { text: text.into(), good: false }
}

/// The facts after the title: year (or years), length or season counts, and the score.
pub fn meta_parts(m: &Meta) -> Vec<MetaPart> {
    let mut parts = Vec::new();
    let series = m.kind == "Series";
    if let Some(year) = m.year {
        parts.push(part(match (series, m.status.as_deref(), m.end_year) {
            (true, Some("Continuing"), _) => format!("{year}\u{2013}"),
            (true, _, Some(end)) if end != year => format!("{year}\u{2013}{end}"),
            _ => year.to_string(),
        }));
    }
    if series {
        if m.season_count > 0 {
            parts.push(part(text::count(m.season_count as u64, "season")));
        }
        if let Some(episodes) = m.episode_count {
            parts.push(part(text::count(u64::from(episodes), "episode")));
            if let Some(unplayed) = m.unplayed {
                parts.push(part(format!("{} of {} watched", episodes.saturating_sub(unplayed), episodes)));
            }
        }
    } else if let Some(minutes) = m.runtime {
        parts.push(part(text::runtime(minutes)));
    }
    if let Some(rating) = m.rating {
        parts.push(part(format!("{rating:.1}/10")));
    }
    parts
}

fn facts(d: &Detail) -> Vec<FactData> {
    let mut facts = Vec::new();
    if !d.audio_languages.is_empty() {
        facts.push(FactData { label: "Audio".into(), value: text::language_list(&d.audio_languages, 8).into() });
    }
    if !d.subtitle_languages.is_empty() {
        facts.push(FactData { label: "Subtitles".into(), value: text::language_list(&d.subtitle_languages, 8).into() });
    }
    if let Some(rating) = d.official_rating.as_deref().filter(|r| !r.trim().is_empty()) {
        facts.push(FactData { label: "Age rating".into(), value: rating.into() });
    }
    if !d.studios.is_empty() {
        facts.push(FactData { label: "Studio".into(), value: d.studios.join(", ").into() });
    }
    if let Some(status) = d.status.as_deref().filter(|s| !s.is_empty()) {
        facts.push(FactData { label: "Status".into(), value: if status == "Continuing" { "Still airing".into() } else { status.into() } });
    }
    facts
}

pub(crate) fn media_rows(d: &Detail) -> Vec<FactData> {
    let Some(m) = &d.media else { return Vec::new() };
    let row = |label: &str, value: String| (!value.is_empty()).then(|| FactData { label: label.into(), value: value.into() });
    [
        row("Container", m.container.clone().unwrap_or_default()),
        row("Video", m.video.join(", ")),
        row("Audio", m.audio.join(", ")),
        row("Subtitles", m.subtitles.join(", ")),
        row("Size", m.size_bytes.map(text::size).unwrap_or_default()),
        row("Bitrate", m.bitrate.map(text::bitrate).unwrap_or_default()),
        row("Studio", d.studios.join(", ")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// A button's label for a download's state: "Downloading 43%" while it runs.
pub(crate) fn download_label(download: Option<&(String, f32)>) -> String {
    match download {
        Some((status, fraction)) if status == "downloading" => format!("Downloading {}%", (fraction * 100.0).round()),
        other => downloads::button_label(other).to_string(),
    }
}

fn download_action(status: Option<&str>) -> &'static str {
    match status {
        None => "Download episode",
        Some("done") => "Delete download",
        Some(_) => "Cancel download",
    }
}

/// An episode as the grid shows it.
fn episode_row(e: &Episode, series: &str, play_target: &Option<(String, bool)>) -> (EpisodeData, Option<String>) {
    let download = downloads::of_item(&e.id);
    let mut foot = Vec::new();
    if !e.audio_languages.is_empty() {
        foot.push(part(text::language_list(&e.audio_languages, 2)));
    }
    if e.has_subtitles {
        foot.push(part("Subtitles"));
    }
    if e.played {
        foot.push(MetaPart { text: "Watched".into(), good: true });
    }
    if download.as_ref().is_some_and(|d| d.0 == "done") {
        foot.push(MetaPart { text: "Downloaded".into(), good: true });
    }
    let badge = match play_target {
        Some((id, resume)) if *id == e.id => {
            if *resume {
                "Continue"
            } else {
                "Up next"
            }
        }
        _ => "",
    };
    let data = EpisodeData {
        id: e.id.clone().into(),
        number: e.number.map(|n| format!("E{n}")).unwrap_or_default().into(),
        title: e.title.clone().into(),
        series: series.into(),
        runtime: e.runtime_minutes.map(text::runtime).unwrap_or_default().into(),
        badge: badge.into(),
        foot: ModelRc::new(VecModel::from(foot)),
        image: Default::default(),
        has_image: false,
        progress: if e.played { 0.0 } else { e.progress.unwrap_or(0.0) as f32 },
        played: e.played,
        download_status: download.as_ref().map(|d| d.0.clone()).unwrap_or_default().into(),
        download_progress: download.as_ref().map(|d| d.1).unwrap_or(0.0),
        download_action: download_action(download.as_ref().map(|d| d.0.as_str())).into(),
    };
    (data, e.image.as_ref().map(|i| i.path(720)))
}

/// Shows the page for `id`: empty at once, then filled in. `season` is the one to list episodes of,
/// when the page was opened from a season. UI thread.
pub fn show(ctx: &Ctx, weak: &Weak<App>, app: &App, id: String, season: Option<String>) {
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((id.clone(), season.clone()));
    PAGE.with(|p| *p.borrow_mut() = None);
    app.set_title_data(TitleData::default());
    app.set_title_loading(true);
    app.set_title_load_error("".into());
    app.set_title_action_error("".into());
    app.set_title_action_note("".into());
    app.set_title_expanded(false);
    app.set_title_has_backdrop(false);
    app.set_title_has_logo(false);
    app.set_title_seasons(ModelRc::new(VecModel::<SeasonData>::default()));
    app.set_title_season_names(ModelRc::new(VecModel::<SharedString>::default()));
    app.set_title_selected_season("".into());
    app.set_title_season_count("".into());
    app.set_title_prev_season("".into());
    app.set_title_next_season("".into());
    app.set_title_newest_first(false);
    app.set_title_episodes(ModelRc::new(VecModel::<EpisodeData>::default()));
    app.set_title_episodes_loading(false);
    app.set_title_episodes_error("".into());
    app.set_title_episode_error("".into());
    app.set_title_cast(ModelRc::new(VecModel::<PersonData>::default()));
    app.set_title_collections(ModelRc::new(VecModel::<CollectionRow>::default()));
    app.set_title_collection_cards(ModelRc::new(VecModel::<crate::CardData>::default()));
    app.set_title_collection_loading(false);
    app.set_title_similar(ModelRc::new(VecModel::<crate::CardData>::default()));
    app.set_screen(Screen::Title);

    let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
    bloom_core::rt::spawn(async move { load(ctx, weak, id, season, generation).await });
}

/// "Try again" after the page failed to load.
pub fn retry(ctx: Ctx, weak: Weak<App>) {
    let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some((id, season)) = last {
        let w = weak.clone();
        let _ = w.upgrade_in_event_loop(move |app| show(&ctx, &weak, &app, id, season));
    }
}

fn fail(weak: &Weak<App>, generation: u64, message: String) {
    if crate::session::is_signed_out(&message) {
        return crate::session::ended(weak);
    }
    let _ = weak.upgrade_in_event_loop(move |app| {
        if cards::generation() == generation {
            app.set_title_loading(false);
            app.set_title_load_error(message.into());
        }
    });
}

async fn load(ctx: Ctx, weak: Weak<App>, id: String, season: Option<String>, generation: u64) {
    let jf = ctx.state::<Jellyfin>();
    let downloads = ctx.state::<Downloads>();
    let detail = match library::item_detail(&jf, &downloads, id.clone()).await {
        Ok(json) => match serde_json::from_value::<Detail>(json) {
            Ok(d) => d,
            Err(e) => return fail(&weak, generation, e.to_string()),
        },
        Err(e) => return fail(&weak, generation, e.to_string()),
    };

    let is_series = detail.kind == "Series";
    let is_collection = detail.kind == "BoxSet";
    let first_season = season
        .filter(|s| detail.seasons.iter().any(|d| d.id == *s))
        .or_else(|| detail.play.as_ref().and_then(|p| p.season_id.clone()))
        .or_else(|| detail.seasons.first().map(|s| s.id.clone()));
    let meta = Meta {
        kind: detail.kind.clone(),
        year: detail.year,
        end_year: detail.end_year,
        status: detail.status.clone(),
        runtime: detail.runtime_minutes,
        rating: detail.community_rating,
        season_count: detail.seasons.len(),
        episode_count: detail.episode_count,
        unplayed: detail.unplayed_count,
    };
    let play_target = detail.play.as_ref().filter(|_| is_series).map(|p| (p.item_id.clone(), p.resume));

    let people: Vec<(String, String, String, Option<String>)> =
        detail.people.iter().take(40).map(|p| (p.id.clone(), p.name.clone(), p.role.clone().unwrap_or_default(), p.image.as_ref().map(|i| i.path(240)))).collect();
    let backdrop = detail.backdrop.as_ref().map(|i| i.path(1920));
    // The page takes its colours from the poster's dominant colour (or the backdrop's, with no poster).
    let tint_art = detail.poster.as_ref().or(detail.backdrop.as_ref()).map(|i| i.path(160));
    let logo = detail.logo.as_ref().map(|i| i.path(480));
    let download = downloads::of_item(&detail.id);
    let (series_title, seasons_kept, season_start) = (detail.title.clone(), detail.seasons.clone(), first_season.clone().unwrap_or_default());
    let meta_kept = meta.clone();
    let (ctx2, weak2, season2) = (ctx.clone(), weak.clone(), first_season.clone());

    // Everything the page shows at once, built off the UI thread where it can be.
    let studio = detail.studios.join(", ");
    let status = detail.status.as_deref().map(|s| if s == "Continuing" { "Still airing".to_string() } else { s.to_string() }).unwrap_or_default();
    // The studio is already under the synopsis on this page.
    let (facts_rows, media) = (facts(&detail), media_rows(&detail).into_iter().filter(|r| r.label != "Studio").collect::<Vec<_>>());
    let play = detail.play.as_ref().map(|p| (p.label.clone(), p.item_id.clone()));
    let has_trailer = !detail.trailers.is_empty();
    let _ = weak.upgrade_in_event_loop(move |app| {
        if cards::generation() != generation {
            return;
        }
        PAGE.with(|p| {
            *p.borrow_mut() = Some(Page {
                title: series_title,
                seasons: seasons_kept,
                season_id: season_start,
                episodes: Vec::new(),
                newest_first: false,
                play_target,
                detail_meta: meta_kept,
                model: None,
            })
        });
        app.set_title_data(TitleData {
            id: detail.id.clone().into(),
            title: detail.title.clone().into(),
            meta: ModelRc::new(VecModel::from(meta_parts(&meta))),
            rating: detail.official_rating.clone().unwrap_or_default().into(),
            genres: detail.genres.join(", ").into(),
            tagline: detail.tagline.clone().unwrap_or_default().into(),
            overview: detail.overview.clone().unwrap_or_default().into(),
            studio: studio.into(),
            status: status.into(),
            facts: ModelRc::new(VecModel::from(facts_rows)),
            media: ModelRc::new(VecModel::from(media)),
            has_play: play.is_some(),
            play_label: play.as_ref().map(|p| p.0.clone()).unwrap_or_default().into(),
            play_id: play.map(|p| p.1).unwrap_or_default().into(),
            favorite: detail.favorite,
            played: detail.played,
            is_series,
            is_collection,
            is_film: !is_series && !is_collection,
            has_trailer,
            download_label: download_label(download.as_ref()).into(),
            download_status: download.as_ref().map(|d| d.0.clone()).unwrap_or_default().into(),
        });
        app.set_title_loading(false);
        let rows: Vec<SeasonData> = detail.seasons.iter().map(|s| SeasonData { id: s.id.clone().into(), name: s.name.clone().into() }).collect();
        app.set_title_seasons(ModelRc::new(VecModel::from(rows)));
        app.set_title_season_names(ModelRc::new(VecModel::from(detail.seasons.iter().map(|s| SharedString::from(s.name.as_str())).collect::<Vec<_>>())));
        app.set_title_selected_season(season2.clone().unwrap_or_default().into());
        update_season_chrome(&app);

        let cast = Rc::new(VecModel::from(
            people.iter().map(|(id, name, role, _)| PersonData { id: id.clone().into(), name: name.clone().into(), role: role.clone().into(), initial: initial(name), image: Default::default(), has_image: false }).collect::<Vec<_>>(),
        ));
        let tracker = cards::track(&cast, |row: &mut PersonData, image| {
            row.image = image;
            row.has_image = true;
        });
        app.set_title_cast(ModelRc::from(cast));
        let jobs = people.iter().enumerate().filter_map(|(i, p)| p.3.clone().map(|path| (i, path))).collect();
        cards::fetch_rows(&ctx2, &weak2, tracker, jobs);

        if let Some(path) = tint_art {
            let (ctx, weak) = (ctx2.clone(), weak2.clone());
            bloom_core::rt::spawn(async move {
                let Some(pixels) = cards::pixels(&ctx, &path).await else { return };
                let seed = bloom_core::rt::spawn_blocking(move || crate::theme::seed_of(&pixels)).await.ok().flatten();
                if let Some(seed) = seed {
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        if cards::generation() == generation {
                            crate::theme::set_page_seed(&app, Some(seed));
                        }
                    });
                }
            });
        }
        if let Some(path) = backdrop {
            cards::fetch_one(&ctx2, &weak2, generation, path, |app, image| {
                app.set_title_backdrop(image);
                app.set_title_has_backdrop(true);
            });
        }
        if let Some(path) = logo {
            cards::fetch_one(&ctx2, &weak2, generation, path, |app, image| {
                app.set_title_logo(image);
                app.set_title_has_logo(true);
            });
        }
    });

    if is_series {
        if let Some(season) = first_season {
            load_episodes(&ctx, &weak, id.clone(), season, generation);
        }
    }

    if is_collection {
        let w = weak.clone();
        let _ = w.upgrade_in_event_loop(|app| app.set_title_collection_loading(true));
        match library::collection_items_with(&jf, &id).await {
            Ok(found) => {
                let plain: Vec<PlainCard> = found.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect();
                let (ctx, weak2) = (ctx.clone(), weak.clone());
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if cards::generation() != generation {
                        return;
                    }
                    let (tracker, model) = cards::card_model(&plain);
                    app.set_title_collection_cards(ModelRc::from(model));
                    app.set_title_collection_loading(false);
                    cards::fetch_rows(&ctx, &weak2, tracker, cards::jobs(&plain, 0));
                });
            }
            Err(e) => {
                let message = e.to_string();
                let _ = weak.upgrade_in_event_loop(move |app| {
                    app.set_title_collection_loading(false);
                    app.set_title_action_error(message.into());
                });
            }
        }
    } else if let Ok(rows) = library::item_collections_with(&jf, &id).await {
        // The collections it is part of: quiet when they can't be read.
        let plain: Vec<(String, String, Vec<PlainCard>)> = rows.iter().map(|r| (r.id.clone(), r.title.clone(), r.cards.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect())).collect();
        let (ctx, weak2) = (ctx.clone(), weak.clone());
        let _ = weak.upgrade_in_event_loop(move |app| {
            if cards::generation() != generation {
                return;
            }
            let mut out = Vec::new();
            for (id, title, cards_plain) in &plain {
                let (tracker, model) = cards::card_model(cards_plain);
                cards::fetch_rows(&ctx, &weak2, tracker, cards::jobs(cards_plain, 0));
                out.push(CollectionRow { id: id.clone().into(), title: title.clone().into(), cards: ModelRc::from(model) });
            }
            app.set_title_collections(ModelRc::new(VecModel::from(out)));
        });
    }

    // Similar titles come last: they're the bottom of the page.
    if let Ok(found) = library::similar_with(&jf, &id).await {
        let plain: Vec<PlainCard> = found.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect();
        let (ctx, weak2) = (ctx.clone(), weak.clone());
        let _ = weak.upgrade_in_event_loop(move |app| {
            if cards::generation() != generation {
                return;
            }
            let (tracker, model) = cards::card_model(&plain);
            app.set_title_similar(ModelRc::from(model));
            cards::fetch_rows(&ctx, &weak2, tracker, cards::jobs(&plain, 0));
        });
    }
}

/// Everything about the chosen season that isn't the episode list: its count line, whether it is all
/// watched or all downloaded, and the neighbouring seasons' names. UI thread.
fn update_season_chrome(app: &App) {
    let Some((count, watched, listed, prev, next, index, selected)) = page(|p| {
        let index = p.seasons.iter().position(|s| s.id == p.season_id);
        let season = index.and_then(|i| p.seasons.get(i));
        let count = season
            .map(|s| {
                let mut line = text::count(u64::from(s.episode_count), "episode");
                if s.unplayed > 0 {
                    line.push_str(&format!(", {} unwatched", s.unplayed));
                }
                line
            })
            .unwrap_or_default();
        let watched = !p.episodes.is_empty() && p.episodes.iter().all(|e| e.played);
        let listed = !p.episodes.is_empty() && p.episodes.iter().all(|e| downloads::of_item(&e.id).is_some());
        let prev = index.and_then(|i| i.checked_sub(1)).and_then(|i| p.seasons.get(i)).map(|s| s.name.clone()).unwrap_or_default();
        let next = index.and_then(|i| p.seasons.get(i + 1)).map(|s| s.name.clone()).unwrap_or_default();
        (count, watched, listed, prev, next, index.map(|i| i as i32).unwrap_or(0), p.season_id.clone())
    }) else {
        return;
    };
    app.set_title_season_count(count.into());
    app.set_title_season_watched(watched);
    app.set_title_season_listed(listed);
    app.set_title_prev_season(prev.into());
    app.set_title_next_season(next.into());
    app.set_title_season_index(index);
    app.set_title_selected_season(selected.into());
}

/// Puts the page's episodes on screen, in the order chosen. UI thread.
fn show_episodes(ctx: &Ctx, weak: &Weak<App>, app: &App) {
    let Some((rows, jobs)) = page(|p| {
        let mut list: Vec<&Episode> = p.episodes.iter().collect();
        if p.newest_first {
            list.reverse();
        }
        let built: Vec<(EpisodeData, Option<String>)> = list.iter().map(|e| episode_row(e, &p.title, &p.play_target)).collect();
        let jobs: Vec<(usize, String)> = built.iter().enumerate().filter_map(|(i, r)| r.1.clone().map(|path| (i, path))).collect();
        (built.into_iter().map(|r| r.0).collect::<Vec<_>>(), jobs)
    }) else {
        return;
    };
    let model = Rc::new(VecModel::from(rows));
    page(|p| p.model = Some(model.clone()));
    let tracker = cards::track(&model, |row: &mut EpisodeData, image| {
        row.image = image;
        row.has_image = true;
    });
    app.set_title_episodes(ModelRc::from(model));
    app.set_title_newest_first(page(|p| p.newest_first).unwrap_or(false));
    cards::fetch_rows(ctx, weak, tracker, jobs);
}

/// The episodes of one season, replacing the list on the page.
fn load_episodes(ctx: &Ctx, weak: &Weak<App>, series_id: String, season_id: String, generation: u64) {
    let (ctx, weak) = (ctx.clone(), weak.clone());
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(|app| {
        app.set_title_episodes_loading(true);
        app.set_title_episodes_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let downloads = ctx.state::<Downloads>();
        let page_json = library::season_episodes(&jf, &downloads, series_id, season_id.clone()).await;
        let (ctx2, weak2) = (ctx.clone(), weak.clone());
        let _ = weak.upgrade_in_event_loop(move |app| {
            if cards::generation() != generation || page(|p| p.season_id != season_id).unwrap_or(true) {
                return;
            }
            app.set_title_episodes_loading(false);
            match page_json {
                Ok(json) => {
                    let episodes = serde_json::from_value::<EpisodePage>(json).unwrap_or_default().episodes;
                    page(|p| p.episodes = episodes);
                    show_episodes(&ctx2, &weak2, &app);
                    update_season_chrome(&app);
                }
                Err(e) => app.set_title_episodes_error(e.to_string().into()),
            }
        });
    });
}

/// The downloads changed: the page's download button, its episodes and the season's state say so.
/// UI thread.
pub fn refresh_downloads(app: &App) {
    if app.get_screen() != Screen::Title {
        return;
    }
    let mut data = app.get_title_data();
    if data.is_film {
        let download = downloads::of_item(&data.id);
        data.download_label = download_label(download.as_ref()).into();
        data.download_status = download.map(|d| d.0).unwrap_or_default().into();
        app.set_title_data(data);
    }
    let Some(Some(model)) = page(|p| p.model.clone()) else { return };
    for i in 0..model.row_count() {
        let Some(mut row) = model.row_data(i) else { continue };
        let download = downloads::of_item(&row.id);
        let (status, progress) = download.clone().unwrap_or_default();
        let action = download_action(download.as_ref().map(|d| d.0.as_str()));
        if row.download_status != status.as_str() || row.download_progress != progress || row.download_action != action {
            // "Downloaded" joins or leaves the foot line.
            let mut foot: Vec<MetaPart> = row.foot.iter().filter(|f| f.text != "Downloaded").collect();
            if status == "done" {
                foot.push(MetaPart { text: "Downloaded".into(), good: true });
            }
            row.foot = ModelRc::new(VecModel::from(foot));
            row.download_status = status.into();
            row.download_progress = progress;
            row.download_action = action.into();
            model.set_row_data(i, row);
        }
    }
    update_season_chrome(app);
}

/// The server says an item's user data changed: this page shows it if the page is for that item, or
/// has it among its episodes. UI thread.
pub fn apply_change(app: &App, change: &crate::live::Change) {
    if app.get_screen() != Screen::Title {
        return;
    }
    let mut data = app.get_title_data();
    if data.id == change.item_id.as_str() {
        data.favorite = change.favorite;
        if data.is_film {
            data.played = change.played;
        }
        if data.is_series {
            if let (Some(unplayed), Some(meta)) = (change.unplayed_count, page(|p| p.detail_meta.clone())) {
                let mut meta = meta;
                meta.unplayed = Some(unplayed);
                page(|p| p.detail_meta = meta.clone());
                data.meta = ModelRc::new(VecModel::from(meta_parts(&meta)));
            }
        }
        app.set_title_data(data);
    }
    let touched = page(|p| {
        let mut touched = false;
        for e in p.episodes.iter_mut().filter(|e| e.id == change.item_id) {
            e.played = change.played;
            e.progress = if change.played { None } else { change.progress };
            touched = true;
        }
        touched
    })
    .unwrap_or(false);
    if touched {
        if let Some(Some(model)) = page(|p| p.model.clone()) {
            for i in 0..model.row_count() {
                let Some(mut row) = model.row_data(i).filter(|r| r.id == change.item_id.as_str()) else { continue };
                row.played = change.played;
                row.progress = if change.played { 0.0 } else { change.progress.unwrap_or(0.0) as f32 };
                let mut foot: Vec<MetaPart> = row.foot.iter().filter(|f| f.text != "Watched").collect();
                if change.played {
                    foot.push(MetaPart { text: "Watched".into(), good: true });
                }
                row.foot = ModelRc::new(VecModel::from(foot));
                model.set_row_data(i, row);
            }
        }
        update_season_chrome(app);
    }
}

/// The page's own download button.
pub fn download_title(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| downloads::pressed(ctx, weak, app.get_title_data().id.to_string()));
}

/// "Download all": every film of a collection, or every season of a show.
pub fn download_all(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let data = app.get_title_data();
        let id = data.id.to_string();
        if data.is_series {
            let seasons: Vec<String> = PAGE.with(|p| p.borrow().as_ref().map(|p| p.seasons.iter().map(|s| s.id.clone()).collect()).unwrap_or_default());
            downloads::download_show(ctx, weak, id, seasons);
        } else {
            downloads::download_collection(ctx, weak, id);
        }
    });
}

/// "Download season", or the Downloads screen once every episode is listed there.
pub fn download_season(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        if app.get_title_season_listed() {
            return nav::go(ctx, weak, nav::Route::Downloads);
        }
        let (series, season) = (app.get_title_data().id.to_string(), app.get_title_selected_season().to_string());
        if !season.is_empty() {
            downloads::download_season(ctx, weak, series, season);
        }
    });
}

pub fn season_chosen(ctx: Ctx, weak: Weak<App>, season: String) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let series = app.get_title_data().id.to_string();
        page(|p| {
            p.season_id = season.clone();
            p.episodes.clear();
        });
        app.set_title_episode_error("".into());
        app.set_title_episodes(ModelRc::new(VecModel::<EpisodeData>::default()));
        update_season_chrome(&app);
        load_episodes(&ctx, &weak, series, season, cards::generation());
    });
}

/// "Newest first" / "Oldest first": the same episodes, turned round.
pub fn toggle_sort(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        page(|p| p.newest_first = !p.newest_first);
        show_episodes(&ctx, &weak, &app);
    });
}

/// Marks the whole season watched, or unwatched when it all is already.
pub fn toggle_season_played(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let Some((season_id, played)) = page(|p| (p.season_id.clone(), !(!p.episodes.is_empty() && p.episodes.iter().all(|e| e.played)))) else { return };
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            let result = library::set_played_with(&jf, &season_id, played).await;
            let (ctx2, weak2) = (ctx.clone(), weak.clone());
            let _ = weak.upgrade_in_event_loop(move |app| {
                if page(|p| p.season_id != season_id).unwrap_or(true) {
                    return;
                }
                match result {
                    Ok(_) => {
                        page(|p| {
                            for e in &mut p.episodes {
                                e.played = played;
                                if played {
                                    e.progress = None;
                                }
                            }
                            let Some(season) = p.seasons.iter_mut().find(|s| s.id == p.season_id) else { return };
                            let before = season.unplayed;
                            season.unplayed = if played { 0 } else { season.episode_count };
                            if let Some(total) = p.detail_meta.unplayed {
                                p.detail_meta.unplayed = Some((total + season.unplayed).saturating_sub(before));
                            }
                        });
                        show_episodes(&ctx2, &weak2, &app);
                        update_season_chrome(&app);
                        if let Some(meta) = page(|p| p.detail_meta.clone()) {
                            let mut data = app.get_title_data();
                            data.meta = ModelRc::new(VecModel::from(meta_parts(&meta)));
                            app.set_title_data(data);
                        }
                    }
                    Err(e) => app.set_title_episode_error(e.to_string().into()),
                }
            });
        });
        let _ = &app;
    });
}

/// An action in an episode's menu.
pub fn episode_menu(ctx: Ctx, weak: Weak<App>, id: String, action: String) {
    match action.as_str() {
        "download" => {
            let status = downloads::of_item(&id).map(|d| d.0);
            match status.as_deref() {
                None => downloads::download_item(ctx, weak, id),
                Some(_) => downloads::remove_for_item(&ctx, &id),
            }
        }
        "played" => {
            let w = weak.clone();
            let _ = w.upgrade_in_event_loop(move |app| {
                let Some(played) = page(|p| p.episodes.iter().find(|e| e.id == id).map(|e| !e.played)).flatten() else { return };
                bloom_core::rt::spawn(async move {
                    let jf = ctx.state::<Jellyfin>();
                    let result = library::set_played_with(&jf, &id, played).await;
                    let (ctx2, weak2) = (ctx.clone(), weak.clone());
                    let _ = weak.upgrade_in_event_loop(move |app| match result {
                        Ok(now) => {
                            page(|p| {
                                if let Some(e) = p.episodes.iter_mut().find(|e| e.id == id) {
                                    e.played = now;
                                    if now {
                                        e.progress = None;
                                    }
                                }
                            });
                            show_episodes(&ctx2, &weak2, &app);
                            update_season_chrome(&app);
                        }
                        Err(e) => app.set_title_episode_error(e.to_string().into()),
                    });
                });
                let _ = &app;
            });
        }
        _ => {}
    }
}

pub fn toggle_favorite(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let data = app.get_title_data();
        let (id, favorite) = (data.id.to_string(), !data.favorite);
        app.set_title_action_error("".into());
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            match home::set_favorite_with(&jf, id, favorite).await {
                Ok(now) => {
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        let mut data = app.get_title_data();
                        data.favorite = now;
                        app.set_title_data(data);
                    });
                }
                Err(e) => {
                    let message = e.to_string();
                    let _ = weak.upgrade_in_event_loop(move |app| app.set_title_action_error(message.into()));
                }
            }
        });
    });
}

pub fn toggle_played(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let data = app.get_title_data();
        let (id, played) = (data.id.to_string(), !data.played);
        app.set_title_action_error("".into());
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            match library::set_played_with(&jf, &id, played).await {
                Ok(now) => {
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        let mut data = app.get_title_data();
                        data.played = now;
                        app.set_title_data(data);
                    });
                }
                Err(e) => {
                    let message = e.to_string();
                    let _ = weak.upgrade_in_event_loop(move |app| app.set_title_action_error(message.into()));
                }
            }
        });
    });
}

/// "Trailer": in the player when yt-dlp is installed, otherwise in the browser, saying so. Finding
/// the stream takes a few seconds, so the button says it is working, and a press while it works is
/// ignored (a second one would start a second search and cancel the first).
pub fn trailer(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        if app.get_title_trailer_busy() {
            return;
        }
        let id = app.get_title_data().id.to_string();
        app.set_title_action_error("".into());
        app.set_title_action_note("Finding the trailer".into());
        app.set_title_trailer_busy(true);
        watch::TRAILER_RETRIES.store(0, std::sync::atomic::Ordering::SeqCst);
        let done = {
            let weak = weak.clone();
            move || {
                let _ = weak.upgrade_in_event_loop(|app| {
                    app.set_title_trailer_busy(false);
                    if app.get_title_action_note().as_str() == "Finding the trailer" {
                        app.set_title_action_note("".into());
                    }
                });
            }
        };
        if bloom_core::trailer::trailers_in_app() {
            return watch::play_trailer(ctx, weak, id, 0, done);
        }
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            let result = bloom_core::trailer::open_trailer(&jf, id, 0).await;
            done();
            let _ = weak.upgrade_in_event_loop(move |app| match result {
                Ok(()) => {
                    app.set_title_action_note("Opened the trailer in your browser. Install yt-dlp to play trailers in Bloom.".into());
                    let weak = app.as_weak();
                    slint::Timer::single_shot(std::time::Duration::from_secs(10), move || {
                        if let Some(app) = weak.upgrade() {
                            app.set_title_action_note("".into());
                        }
                    });
                }
                Err(e) => app.set_title_action_error(e.to_string().into()),
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(parts: &[MetaPart]) -> Vec<String> {
        parts.iter().map(|p| p.text.to_string()).collect()
    }

    #[test]
    fn a_running_show_reads_year_to_open_end_and_counts_what_is_watched() {
        let m = Meta { kind: "Series".into(), year: Some(2020), status: Some("Continuing".into()), season_count: 3, episode_count: Some(30), unplayed: Some(12), rating: Some(8.04), ..Default::default() };
        assert_eq!(texts(&meta_parts(&m)), ["2020\u{2013}", "3 seasons", "30 episodes", "18 of 30 watched", "8.0/10"]);
    }

    #[test]
    fn an_ended_show_reads_its_years_and_one_season_is_singular() {
        let m = Meta { kind: "Series".into(), year: Some(2020), end_year: Some(2023), season_count: 1, episode_count: Some(1), ..Default::default() };
        assert_eq!(texts(&meta_parts(&m)), ["2020\u{2013}2023", "1 season", "1 episode"]);
        let same = Meta { kind: "Series".into(), year: Some(2020), end_year: Some(2020), ..Default::default() };
        assert_eq!(texts(&meta_parts(&same)), ["2020"]);
    }

    #[test]
    fn a_film_reads_year_length_and_score() {
        let m = Meta { year: Some(2026), runtime: Some(112), rating: Some(7.84), ..Default::default() };
        assert_eq!(texts(&meta_parts(&m)), ["2026", "1h 52m", "7.8/10"]);
    }

    #[test]
    fn an_episodes_menu_offers_what_its_download_allows() {
        assert_eq!(download_action(None), "Download episode");
        assert_eq!(download_action(Some("downloading")), "Cancel download");
        assert_eq!(download_action(Some("done")), "Delete download");
    }

    #[test]
    fn only_the_episode_play_would_start_wears_a_badge() {
        let e = |id: &str| Episode { id: id.into(), ..Default::default() };
        let target = Some(("b".to_string(), false));
        assert_eq!(episode_row(&e("a"), "Show", &target).0.badge, "");
        assert_eq!(episode_row(&e("b"), "Show", &target).0.badge, "Up next");
        assert_eq!(episode_row(&e("b"), "Show", &Some(("b".to_string(), true))).0.badge, "Continue");
    }

    #[test]
    fn an_avatar_shows_the_names_first_letter() {
        assert_eq!(initial("ada Lovelace"), "A");
        assert_eq!(initial(""), "");
    }
}

//! Home: the spotlight and the sections, then their artwork as it arrives.
//!
//! The text comes first and the screen is usable at once; each poster fills in when its image has
//! been fetched (from the core's cache or the server) and decoded off the UI thread. The spotlight
//! is drawn once per sign-in and kept, so coming back to Home doesn't reshuffle it; the sections
//! are asked for again on every visit, and the ones on screen stay until the new ones arrive.

use crate::cards::{self, PlainCard};
use crate::text;
use crate::{App, MetaPart, SectionData, SpotlightData};
use bloom_core::downloads::Downloads;
use bloom_core::home::{self, Shape, Spotlight};
use bloom_core::jellyfin::Jellyfin;
use bloom_core::Ctx;
use slint::{Image, Model, ModelRc, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The spotlight has been drawn for this sign-in.
static SPOTLIGHT_DRAWN: AtomicBool = AtomicBool::new(false);

/// The slides on screen, and each one's length in minutes (what its "Ends at" depends on).
type Slides = (Rc<VecModel<SpotlightData>>, Vec<Option<u32>>);

thread_local! {
    static SPOTS: RefCell<Option<Slides>> = const { RefCell::new(None) };
}

struct PlainSection {
    title: String,
    wide: bool,
    square: bool,
    cards: Vec<PlainCard>,
}

/// The next sign-in starts with a fresh spotlight.
pub fn forget() {
    SPOTLIGHT_DRAWN.store(false, Ordering::SeqCst);
    SPOTS.with(|s| *s.borrow_mut() = None);
}

fn part(text: impl Into<slint::SharedString>) -> MetaPart {
    MetaPart { text: text.into(), good: false }
}

/// A slide's facts: score, year, length (and when it would end) or seasons.
fn spot_meta(s: &Spotlight, runtime: Option<u32>) -> Vec<MetaPart> {
    let mut parts = Vec::new();
    parts.extend(s.community_rating.map(|r| part(format!("{r:.1}/10"))));
    parts.extend(s.year.map(|y| part(y.to_string())));
    match (runtime, s.seasons) {
        (Some(minutes), _) => {
            parts.push(part(text::runtime(minutes)));
            parts.push(part(text::ends_at(minutes)));
        }
        (None, Some(seasons)) => parts.push(part(text::count(u64::from(seasons), "season"))),
        _ => {}
    }
    parts
}

/// Sections, then the spotlight if this sign-in hasn't drawn one.
pub async fn load(ctx: Ctx, weak: Weak<App>) {
    let generation = cards::generation();
    let jf = ctx.state::<Jellyfin>();
    let downloads = ctx.state::<Downloads>();
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(|app| {
        app.set_home_loading(true);
        app.set_home_error("".into());
        app.set_spotlight_loading(!SPOTLIGHT_DRAWN.load(Ordering::SeqCst));
    });

    // The spotlight is separate from the rows, so a slow random pick never holds them up.
    if !SPOTLIGHT_DRAWN.load(Ordering::SeqCst) {
        let (ctx, weak) = (ctx.clone(), weak.clone());
        bloom_core::rt::spawn(async move { load_spotlight(ctx, weak, generation).await });
    } else {
        // "Ends at" moves on with the clock.
        let _ = weak.upgrade_in_event_loop(|_| refresh_ends_at());
    }

    // "Because you watched" is a bonus: it never holds Home up for long, and its failing costs nothing.
    let because = async {
        tokio::time::timeout(std::time::Duration::from_secs(4), home::because_you_watched_with(&jf, 2)).await.ok().and_then(Result::ok).unwrap_or_default()
    };
    let (sections, saved, because) = tokio::join!(home::home_with(&jf), async { bloom_core::downloads::downloaded_titles(&jf, &downloads) }, because);
    let saved_row = (!saved.is_empty()).then(|| ("downloaded".to_string(), PlainSection { title: "Downloaded".into(), wide: false, square: false, cards: saved.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect() }));

    let (sections, error): (Vec<(String, PlainSection)>, Option<String>) = match sections {
        Ok(found) => {
            let mut rows: Vec<(String, PlainSection)> = found
                .into_iter()
                .map(|s| {
                    let wide = s.shape == Shape::Wide;
                    (s.id, PlainSection { title: s.title, wide, square: s.shape == Shape::Square, cards: s.cards.iter().map(|c| PlainCard::from_core(c, cards::card_width(wide))).collect() })
                })
                .collect();
            // "Because you watched" rows come after Next up, before what was added to each library.
            let at = rows.iter().position(|(id, _)| id.starts_with("latest-")).unwrap_or(rows.len());
            for (i, s) in because.into_iter().enumerate() {
                rows.insert(at + i, (s.id, PlainSection { title: s.title, wide: false, square: false, cards: s.cards.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect() }));
            }
            // Downloaded sits before the first "Latest" row, or last.
            if let Some(row) = saved_row {
                let at = rows.iter().position(|(id, _)| id.starts_with("latest-")).unwrap_or(rows.len());
                rows.insert(at, row);
            }
            (rows, None)
        }
        // Whatever is on this computer still shows, with no server.
        Err(e) => (saved_row.into_iter().collect(), Some(e.to_string())),
    };

    // Development: BLOOM_AUTOPLAY plays the first film or episode on Home, and BLOOM_GO opens a
    // page (see `dev_route`), to try the player and the pages without clicking.
    let every = || sections.iter().flat_map(|(_, s)| &s.cards);
    let autoplay = std::env::var_os("BLOOM_AUTOPLAY").and_then(|_| every().find(|c| ["Movie", "Episode"].contains(&c.kind.as_str())).map(|c| (c.id.clone(), c.kind.clone())));
    let go = std::env::var("BLOOM_GO").ok().and_then(|g| dev_route(&g, every().cloned().collect()));
    let jobs_by_section: Vec<Vec<(usize, String)>> = sections.iter().map(|(_, s)| cards::jobs(&s.cards, 0)).collect();

    let (ctx2, weak2) = (ctx.clone(), weak.clone());
    let _ = weak.upgrade_in_event_loop(move |app| {
        // Nothing to do if the page has since changed.
        if cards::generation() != generation {
            return;
        }
        app.set_home_loading(false);
        crate::flow::loader_ready(&app);
        let empty = sections.is_empty() && error.is_none();
        app.set_home_empty(empty);
        app.set_home_error(error.unwrap_or_default().into());
        let mut rows = Vec::new();
        let mut fetches = Vec::new();
        for ((_, s), jobs) in sections.iter().zip(jobs_by_section) {
            let (id, model) = cards::card_model(&s.cards);
            fetches.push((id, jobs));
            rows.push(SectionData { title: s.title.clone().into(), wide: s.wide, square: s.square, cards: ModelRc::from(model) });
        }
        app.set_sections(ModelRc::new(VecModel::from(rows)));
        for (id, jobs) in fetches {
            cards::fetch_rows(&ctx2, &weak2, id, jobs);
        }
    });

    if let Some((id, kind)) = autoplay {
        crate::nav::activate(ctx.clone(), weak.clone(), id, kind, String::new());
    }
    if let Some(route) = go {
        crate::nav::go(ctx, weak, route);
    }
}

async fn load_spotlight(ctx: Ctx, weak: Weak<App>, generation: u64) {
    let jf = ctx.state::<Jellyfin>();
    let found = home::spotlight_with(&jf).await.unwrap_or_default();
    let (ctx2, weak2) = (ctx.clone(), weak.clone());
    let _ = weak.upgrade_in_event_loop(move |app| {
        app.set_spotlight_loading(false);
        // Another sign-in may have come and gone while this was asked for.
        if cards::generation() != generation && SPOTLIGHT_DRAWN.load(Ordering::SeqCst) {
            return;
        }
        let runtimes: Vec<Option<u32>> = found.iter().map(|s| s.runtime_minutes).collect();
        let rows: Vec<SpotlightData> = found
            .iter()
            .map(|s| SpotlightData {
                id: s.id.clone().into(),
                title: s.title.clone().into(),
                meta: ModelRc::new(VecModel::from(spot_meta(s, s.runtime_minutes))),
                rating: s.official_rating.clone().unwrap_or_default().into(),
                genres: s.genres.join(", ").into(),
                overview: s.overview.clone().unwrap_or_default().into(),
                favorite: s.favorite,
                backdrop: Image::default(),
                has_backdrop: false,
                logo: Image::default(),
                has_logo: false,
            })
            .collect();
        let model = Rc::new(VecModel::from(rows));
        let backdrops = cards::track(&model, |row: &mut SpotlightData, image| {
            row.backdrop = image;
            row.has_backdrop = true;
        });
        let logos = cards::track(&model, |row: &mut SpotlightData, image| {
            row.logo = image;
            row.has_logo = true;
        });
        app.set_spotlights(ModelRc::from(model.clone()));
        app.set_spot_index(0);
        app.set_spot_step(0);
        SPOTS.with(|s| *s.borrow_mut() = Some((model, runtimes)));
        SPOTLIGHT_DRAWN.store(true, Ordering::SeqCst);
        // Each slide's backdrop, and its logo; all of them, since a slide must be there to cross-fade.
        let backdrop_jobs = found.iter().enumerate().map(|(i, s)| (i, s.backdrop.path(1920))).collect();
        let logo_jobs = found.iter().enumerate().filter_map(|(i, s)| s.logo.as_ref().map(|l| (i, l.path(480)))).collect();
        cards::fetch_rows(&ctx2, &weak2, backdrops, backdrop_jobs);
        cards::fetch_rows(&ctx2, &weak2, logos, logo_jobs);
    });
}

/// "Ends at" says when it would end if started now, which moves with the clock. UI thread.
fn refresh_ends_at() {
    SPOTS.with(|s| {
        let s = s.borrow();
        let Some((model, runtimes)) = s.as_ref() else { return };
        for (i, runtime) in runtimes.iter().enumerate() {
            let (Some(minutes), Some(mut row)) = (runtime, model.row_data(i)) else { continue };
            let mut parts: Vec<MetaPart> = row.meta.iter().filter(|p| !p.text.starts_with("Ends at ")).collect();
            parts.push(part(text::ends_at(*minutes)));
            row.meta = ModelRc::new(VecModel::from(parts));
            model.set_row_data(i, row);
        }
    });
}

/// The slide's Favorite button.
pub fn spotlight_favorite(ctx: Ctx, weak: Weak<App>, id: String) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |_| {
        let Some(now) = SPOTS.with(|s| s.borrow().as_ref().and_then(|(m, _)| m.iter().find(|r| r.id == id.as_str()).map(|r| !r.favorite))) else { return };
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            if let Ok(favorite) = home::set_favorite_with(&jf, id.clone(), now).await {
                crate::hover::forget(&id);
                let _ = weak.upgrade_in_event_loop(move |_| {
                    SPOTS.with(|s| {
                        let s = s.borrow();
                        let Some((model, _)) = s.as_ref() else { return };
                        for i in 0..model.row_count() {
                            if let Some(mut row) = model.row_data(i).filter(|r| r.id == id.as_str()) {
                                row.favorite = favorite;
                                model.set_row_data(i, row);
                            }
                        }
                    });
                });
            }
        });
    });
}

/// `BLOOM_GO`: "title" (the first film or show on Home), "library", "libraries", "search:text".
fn dev_route(go: &str, cards: Vec<PlainCard>) -> Option<crate::nav::Route> {
    use crate::nav::Route;
    let first = |kinds: &[&str]| cards.iter().find(|c| kinds.contains(&c.kind.as_str())).cloned();
    match go {
        "title" => first(&["Series", "Movie"]).map(|c| Route::Title(c.id)),
        "library" => first(&["CollectionFolder"]).map(|c| Route::Library { id: c.id, name: c.title }),
        "libraries" => Some(Route::Libraries),
        "settings" => Some(Route::Settings),
        "downloads" => Some(Route::Downloads),
        "servers" => Some(Route::Servers),
        _ => go.strip_prefix("search:").map(|q| Route::Search(q.to_string())),
    }
}

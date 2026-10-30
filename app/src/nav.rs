//! Where the user is, and getting them elsewhere.
//!
//! A stack of routes, the page behind the current one being what Back returns to. Showing a route
//! loads its data again (artwork comes from the core's cache, so a page comes back quickly); pages
//! keep no state of their own across visits. Everything here may be called from any thread: the
//! stack and the models live on the UI thread, so each entry point hops there first.

use crate::cards::{self, PlainCard};
use crate::{home, title, watch, App, CardData, NavTarget, PageScroll, PagerItem, Screen};
use bloom_core::downloads::Downloads;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::library;
use bloom_core::Ctx;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Home,
    Libraries,
    Library { id: String, name: String },
    Collection { id: String, name: String },
    Search(String),
    Title(String),
    /// A show page, at one of its seasons.
    Season { series: String, season: String },
    Person { id: String, name: String },
    Album(String),
    Artist(String),
    Downloads,
    Servers,
    Settings,
}

/// Titles to a page of a library or a person's credits. Mirrors the core's page size.
const PAGE: u32 = 60;

/// The library on screen, kept for paging and re-sorting.
struct Lib {
    id: String,
    sort: usize,
    /// A person's credits rather than a library's items.
    person: bool,
    /// Only the titles downloaded to this computer.
    downloaded: bool,
    page: u32,
}

/// How a library can be sorted, in the order of the select.
const SORTS: [&str; 4] = ["added", "name", "year", "rating"];

/// How many pages keep the place they were scrolled to.
const PLACES_KEPT: usize = 60;

/// Which of a music library's lists is shown: albums, artists or songs (0, 1, 2).
static MUSIC_KIND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The library that list belongs to.
static MUSIC_LIB: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

thread_local! {
    /// The page on screen, and where each page that was left was scrolled to (latest last).
    static SHOWN: RefCell<Option<Route>> = const { RefCell::new(None) };
    /// The next page shown was opened afresh, not returned to: it starts at the top.
    static FRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PLACES: RefCell<Vec<(Route, f32)>> = const { RefCell::new(Vec::new()) };
    static STACK: RefCell<Vec<Route>> = const { RefCell::new(Vec::new()) };
    static LIB: RefCell<Option<Lib>> = const { RefCell::new(None) };

}


/// Notes where the page on screen is scrolled to, as it is left.
fn leave(app: &App) {
    let Some(route) = SHOWN.with(|s| s.borrow().clone()) else { return };
    let y = app.global::<PageScroll>().get_y();
    PLACES.with(|p| {
        let mut p = p.borrow_mut();
        p.retain(|(r, _)| *r != route);
        p.push((route, y));
        if p.len() > PLACES_KEPT {
            p.remove(0);
        }
    });
}

/// The page `route` is on screen: back at the place it was left at, unless it was opened afresh.
fn arrive(app: &App, route: &Route) {
    let fresh = FRESH.with(|f| f.replace(false));
    let place = PLACES.with(|p| p.borrow().iter().find(|(r, _)| r == route).map(|(_, y)| *y)).filter(|y| *y > 1.0 && !fresh);
    let scroll = app.global::<PageScroll>();
    scroll.set_y(0.0);
    match place {
        Some(y) => {
            // The page scrolls there once it has the content to; if it never does, it is left be.
            scroll.set_target(y);
            scroll.set_pending(true);
            let weak = app.as_weak();
            slint::Timer::single_shot(std::time::Duration::from_millis(2500), move || {
                if let Some(app) = weak.upgrade() {
                    app.global::<PageScroll>().set_pending(false);
                }
            });
        }
        None => scroll.set_pending(false),
    }
    SHOWN.with(|s| *s.borrow_mut() = Some(route.clone()));
}

fn on_ui(weak: &Weak<App>, f: impl FnOnce(&App) + Send + 'static) {
    let _ = weak.upgrade_in_event_loop(move |app| f(&app));
}

/// Starts over from `route` (signing in, going Home).
pub fn reset(ctx: Ctx, weak: Weak<App>, route: Route) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        STACK.with(|s| *s.borrow_mut() = vec![route.clone()]);
        FRESH.with(|f| f.set(true));
        show(&ctx, &weak, app, route);
    });
}

/// A rail or palette destination: the history is Home and it, so these never stack up a trail.
pub fn section(ctx: Ctx, weak: Weak<App>, route: Route) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        STACK.with(|s| *s.borrow_mut() = vec![Route::Home, route.clone()]);
        FRESH.with(|f| f.set(true));
        show(&ctx, &weak, app, route);
    });
}

/// Shows the page on top of the history again, as it is now (after the server comes back, or
/// something changed on it).
pub fn reload(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        if matches!(app.get_screen(), Screen::Watch | Screen::SignIn | Screen::Loading) {
            return;
        }
        if let Some(route) = STACK.with(|s| s.borrow().last().cloned()) {
            show(&ctx, &weak, app, route);
        }
    });
}

/// Opens `route` on top of the current page.
pub fn go(ctx: Ctx, weak: Weak<App>, route: Route) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        STACK.with(|s| {
            let mut s = s.borrow_mut();
            if s.last() != Some(&route) {
                s.push(route.clone());
            }
        });
        FRESH.with(|f| f.set(true));
        show(&ctx, &weak, app, route);
    });
}

/// Back to the page behind this one (Home if there is none).
pub fn back(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        let route = STACK.with(|s| {
            let mut s = s.borrow_mut();
            if s.len() > 1 {
                s.pop();
            }
            s.last().cloned().unwrap_or(Route::Home)
        });
        show(&ctx, &weak, app, route);
    });
}

/// Shows the page that is current again (after watching).
pub fn resume(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        let route = STACK.with(|s| s.borrow().last().cloned().unwrap_or(Route::Home));
        show(&ctx, &weak, app, route);
    });
}

/// A card, or the spotlight's Play, was pressed.
pub fn activate(ctx: Ctx, weak: Weak<App>, id: String, kind: String, name: String) {
    match kind.as_str() {
        "Episode" | watch::PLAY_ANY => watch::play(ctx, weak, id),
        "Movie" | "Series" | "Video" => go(ctx, weak, Route::Title(id)),
        "BoxSet" => go(ctx, weak, Route::Title(id)),
        "MusicAlbum" => go(ctx, weak, Route::Album(id)),
        "MusicArtist" => go(ctx, weak, Route::Artist(id)),
        "Audio" => crate::music::play_card(ctx, weak, id),
        "Person" => go(ctx, weak, Route::Person { id, name }),
        "Season" => open_season(ctx, weak, id),
        "CollectionFolder" | "UserView" | "Folder" => go(ctx, weak, Route::Library { id, name }),
        other => watch::say(&weak, format!("A {other} has no page in the Slint interface yet.")),
    }
}

/// A season card opens its show's page at that season.
fn open_season(ctx: Ctx, weak: Weak<App>, season: String) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match library::season_show_with(&jf, &season).await {
            Ok(series) => go(ctx, weak, Route::Season { series, season }),
            Err(e) => watch::say(&weak, e.to_string()),
        }
    });
}

/// Shows `route`. UI thread.
fn show(ctx: &Ctx, weak: &Weak<App>, app: &App, route: Route) {
    leave(app);
    arrive(app, &route);
    // A title page takes its colours from its artwork; every other page has the settings' own.
    if !matches!(route, Route::Title(_) | Route::Season { .. }) {
        crate::theme::set_page_seed(app, None);
    }
    cards::new_generation();
    crate::hover::close(app);
    app.set_can_go_back(STACK.with(|s| s.borrow().len() > 1));
    if !matches!(route, Route::Search(_)) {
        app.set_search_text("".into());
    }
    match route {
        Route::Home => {
            app.set_nav_target(NavTarget::Home);
            app.set_screen(Screen::Home);
            let (ctx, weak) = (ctx.clone(), weak.clone());
            bloom_core::rt::spawn(async move { home::load(ctx, weak).await });
        }
        Route::Libraries => {
            list_page(app, "Libraries", true, false);
            app.set_lib_error_title("Couldn't load your libraries".into());
            app.set_lib_empty_title("No libraries".into());
            app.set_lib_empty_text("This account can't see any libraries. An administrator can give it access in the Jellyfin dashboard, under Users.".into());
            app.set_nav_target(NavTarget::Libraries);
            let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
            bloom_core::rt::spawn(async move {
                let jf = ctx.state::<Jellyfin>();
                match library::libraries_with(&jf).await {
                    Ok(found) => {
                        let plain: Vec<PlainCard> = found.iter().map(|c| PlainCard::from_core(c, cards::card_width(true))).collect();
                        let total = plain.len();
                        apply_list(&ctx, &weak, generation, plain, total, true, None, 0);
                        on_ui(&weak, move |app| app.set_lib_eyebrow(crate::text::count(total as u64, "library").replace("librarys", "libraries").into()));
                    }
                    Err(e) => failed(&weak, generation, e.to_string()),
                }
            });
        }
        Route::Library { id, name } => {
            // A music library opens on its albums; shown again (after a reload) it stays on its tab.
            {
                let mut shown = MUSIC_LIB.lock().unwrap_or_else(|e| e.into_inner());
                if *shown != id {
                    *shown = id.clone();
                    MUSIC_KIND.store(0, std::sync::atomic::Ordering::SeqCst);
                }
            }
            list_page(app, &name, false, true);
            app.set_nav_target(NavTarget::Libraries);
            load_library_page(ctx, weak, id, 0, 0, false);
        }
        Route::Collection { id, name } => {
            list_page(app, if name.is_empty() { "Collection" } else { &name }, false, false);
            app.set_nav_target(NavTarget::Libraries);
            let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
            bloom_core::rt::spawn(async move {
                let jf = ctx.state::<Jellyfin>();
                match library::collection_items_with(&jf, &id).await {
                    Ok(found) => {
                        let plain: Vec<PlainCard> = found.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect();
                        let total = plain.len();
                        apply_list(&ctx, &weak, generation, plain, total, false, None, 0);
                    }
                    Err(e) => failed(&weak, generation, e.to_string()),
                }
            });
        }
        Route::Search(query) => {
            app.set_search_text(query.clone().into());
            crate::search::show(ctx, weak, app, query);
        }
        Route::Title(id) => {
            app.set_nav_target(NavTarget::None);
            title::show(ctx, weak, app, id, None);
        }
        Route::Season { series, season } => {
            app.set_nav_target(NavTarget::None);
            title::show(ctx, weak, app, series, Some(season));
        }
        Route::Person { id, name } => {
            app.set_nav_target(NavTarget::None);
            crate::person::show(ctx, weak, app, id, name);
        }
        Route::Album(id) => crate::music::show_album(ctx, weak, app, id),
        Route::Artist(id) => crate::music::show_artist(ctx, weak, app, id),
        Route::Downloads => crate::downloads::show(ctx, weak, app),
        Route::Servers => crate::servers::show(ctx, weak, app),
        Route::Settings => crate::settings::show(ctx, app),
    }
}

/// A fresh, empty list page. `wide` is whether the cards will be landscape.
pub(crate) fn list_page(app: &App, heading: &str, wide: bool, sortable: bool) {
    LIB.with(|l| *l.borrow_mut() = None);
    app.set_lib_heading(heading.into());
    app.set_lib_cards(ModelRc::new(VecModel::<CardData>::default()));
    app.set_lib_total(0);
    app.set_lib_wide(wide);
    app.set_lib_sortable(sortable);
    app.set_lib_sort_index(0);
    app.set_lib_downloaded(false);
    app.set_lib_music(false);
    app.set_lib_music_kind(MUSIC_KIND.load(std::sync::atomic::Ordering::SeqCst) as i32);
    app.set_lib_page(0);
    app.set_lib_pages(0);
    app.set_lib_pager(ModelRc::new(VecModel::<PagerItem>::default()));
    app.set_lib_eyebrow("".into());
    app.set_lib_error("".into());
    app.set_lib_error_title("Couldn't load this library".into());
    app.set_lib_empty_title(format!("Nothing in {heading} yet").into());
    app.set_lib_empty_text("The library is empty, or it hasn't finished scanning on the server.".into());
    app.set_lib_loading(true);
    app.set_lib_has_intro(false);
    app.set_lib_intro_has_image(false);
    app.set_lib_intro_lines("".into());
    app.set_lib_intro_text("".into());
    app.set_screen(Screen::Library);
}

pub(crate) fn failed(weak: &Weak<App>, generation: u64, message: String) {
    if crate::session::is_signed_out(&message) {
        return crate::session::ended(weak);
    }
    on_ui(weak, move |app| {
        if cards::generation() == generation {
            app.set_lib_loading(false);
            app.set_search_loading(false);
            app.set_lib_error(message.into());
        }
    });
}

/// Where the list on screen comes from, for paging and re-sorting.
pub(crate) struct Source {
    pub(crate) id: String,
    pub(crate) sort: usize,
    pub(crate) person: bool,
    pub(crate) downloaded: bool,
}

/// The buttons of a pager: Previous, the first and last pages and the ones either side of this
/// one with "..." where pages are left out, and Next. `page` is 0-based.
pub(crate) fn pager_items(page: u32, pages: u32) -> Vec<PagerItem> {
    let item = |label: String, target: u32, current: bool, kind: i32, enabled: bool| PagerItem { label: label.into(), target: target as i32, current, kind, enabled };
    let mut out = vec![item("Previous".into(), page.saturating_sub(1), false, 2, page > 0)];
    let mut last_shown: Option<u32> = None;
    for p in 0..pages {
        if p != 0 && p + 1 != pages && p.abs_diff(page) > 1 {
            continue;
        }
        if last_shown.is_some_and(|l| p > l + 1) {
            out.push(item("\u{2026}".into(), 0, false, 1, false));
        }
        out.push(item((p + 1).to_string(), p, p == page, 0, true));
        last_shown = Some(p);
    }
    out.push(item("Next".into(), page + 1, false, 3, page + 1 < pages));
    out
}

/// Puts `plain` on the list page, replacing what is there. `source` says which library (and page)
/// it is, when it is one, so it can be paged and re-sorted.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_list(ctx: &Ctx, weak: &Weak<App>, generation: u64, plain: Vec<PlainCard>, total: usize, wide: bool, source: Option<Source>, page: u32) {
    let (ctx, weak2) = (ctx.clone(), weak.clone());
    on_ui(weak, move |app| {
        if cards::generation() != generation {
            return;
        }
        app.set_lib_loading(false);
        app.set_lib_error("".into());
        app.set_lib_total(total as i32);
        app.set_lib_wide(wide);
        let (tracker, model) = cards::card_model(&plain);
        app.set_lib_cards(ModelRc::from(model));
        if let Some(Source { id, sort, person, downloaded }) = source {
            let pages = (total as u32).div_ceil(PAGE).max(1);
            app.set_lib_page(page as i32);
            app.set_lib_pages(pages as i32);
            app.set_lib_pager(ModelRc::new(VecModel::from(pager_items(page, pages))));
            app.set_lib_eyebrow(format!("{}, page {} of {}", crate::text::count(total as u64, "title"), page + 1, pages).into());
            app.set_lib_downloaded(downloaded);
            if downloaded {
                let heading = app.get_lib_heading();
                app.set_lib_empty_title(format!("Nothing from {heading} downloaded").into());
                app.set_lib_empty_text("Download a film or an episode from its page, and it shows here.".into());
            }
            LIB.with(|l| *l.borrow_mut() = Some(Lib { id, sort, person, downloaded, page }));
        }
        cards::fetch_rows(&ctx, &weak2, tracker, cards::jobs(&plain, 0));
    });
}

/// One page of a library (0-based), with its sort and whether only downloaded titles are wanted.
fn load_library_page(ctx: &Ctx, weak: &Weak<App>, id: String, sort: usize, page: u32, downloaded: bool) {
    let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
    let w = weak.clone();
    on_ui(&w, |app| {
        app.set_lib_loading(true);
        app.set_lib_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let only = downloaded.then(crate::downloads::downloaded_title_ids);
        // A music library lists albums, artists or songs, as chosen.
        let music_kind = MUSIC_KIND.load(std::sync::atomic::Ordering::SeqCst);
        let sort_key = SORTS[sort.min(SORTS.len() - 1)];
        let found = match music_kind {
            1 => bloom_core::library::music_items_with(&jf, &id, "MusicArtist", sort_key, page * PAGE).await,
            2 => bloom_core::library::music_items_with(&jf, &id, "Audio", sort_key, page * PAGE).await,
            _ => library::library_items_with(&jf, &id, sort_key, page * PAGE, only.as_deref()).await,
        };
        match found {
            Ok(grid) => {
                let music = grid.shape == bloom_core::home::Shape::Square;
                on_ui(&weak, move |app| {
                    app.set_lib_music(music);
                    app.set_lib_music_kind(music_kind as i32);
                });
                let wide = grid.shape == bloom_core::home::Shape::Wide;
                let plain: Vec<PlainCard> = grid.cards.iter().map(|c| PlainCard::from_core(c, cards::card_width(wide))).collect();
                apply_list(&ctx, &weak, generation, plain, grid.total as usize, wide, Some(Source { id, sort, person: false, downloaded }), page);
            }
            Err(e) => {
                // With no server, what was downloaded still lists.
                let saved = bloom_core::downloads::downloaded_titles(&jf, &ctx.state::<Downloads>());
                if downloaded || saved.is_empty() {
                    return failed(&weak, generation, e.to_string());
                }
                let plain: Vec<PlainCard> = saved.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect();
                let total = plain.len();
                apply_list(&ctx, &weak, generation, plain, total, false, Some(Source { id, sort, person: false, downloaded: true }), 0);
            }
        }
    });
}

/// A page of the library on screen (0-based).
pub fn page_chosen(ctx: Ctx, weak: Weak<App>, page: i32) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        let Some((id, sort, person, downloaded)) = LIB.with(|l| l.borrow().as_ref().filter(|l| !l.id.is_empty()).map(|l| (l.id.clone(), l.sort, l.person, l.downloaded))) else { return };
        cards::new_generation();
        let page = u32::try_from(page).unwrap_or(0);
        if person {
            crate::person::load_credits(&ctx, &weak, id, page);
        } else {
            load_library_page(&ctx, &weak, id, sort, page, downloaded);
        }
        let _ = &app;
    });
}

/// A different sort for the library on screen (an index into the select).
pub fn sort_chosen(ctx: Ctx, weak: Weak<App>, index: i32) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        let Some((id, downloaded)) = LIB.with(|l| l.borrow().as_ref().map(|l| (l.id.clone(), l.downloaded))) else { return };
        cards::new_generation();
        let sort = usize::try_from(index).unwrap_or(0).min(SORTS.len() - 1);
        app.set_lib_sort_index(sort as i32);
        load_library_page(&ctx, &weak, id, sort, 0, downloaded);
    });
}

/// The "Downloaded" chip: only the titles on this computer, or all of them.
pub fn toggle_downloaded(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        let Some((id, sort, downloaded)) = LIB.with(|l| l.borrow().as_ref().map(|l| (l.id.clone(), l.sort, l.downloaded))) else { return };
        cards::new_generation();
        app.set_lib_downloaded(!downloaded);
        load_library_page(&ctx, &weak, id, sort, 0, !downloaded);
    });
}

/// Albums, artists or songs, in a music library (0, 1, 2).
pub fn music_kind_chosen(ctx: Ctx, weak: Weak<App>, kind: i32) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        let Some((id, sort)) = LIB.with(|l| l.borrow().as_ref().map(|l| (l.id.clone(), l.sort))) else { return };
        cards::new_generation();
        let kind = usize::try_from(kind).unwrap_or(0).min(2);
        MUSIC_KIND.store(kind, std::sync::atomic::Ordering::SeqCst);
        app.set_lib_music_kind(kind as i32);
        load_library_page(&ctx, &weak, id, sort, 0, false);
    });
}

/// "Try again" on the list page.
pub fn retry_list(ctx: Ctx, weak: Weak<App>) {
    let page = LIB.with(|l| l.borrow().as_ref().map(|l| l.page as i32)).unwrap_or(0);
    let current = STACK.with(|s| s.borrow().last().cloned());
    match current {
        Some(Route::Libraries | Route::Collection { .. }) => go_again(ctx, weak),
        _ => page_chosen(ctx, weak, page),
    }
}

/// Shows the page on top of the history again.
fn go_again(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        if let Some(route) = STACK.with(|s| s.borrow().last().cloned()) {
            show(&ctx, &weak, app, route);
        }
    });
}


#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[PagerItem]) -> Vec<String> {
        items.iter().map(|i| i.label.to_string()).collect()
    }

    #[test]
    fn a_pager_keeps_the_ends_and_the_neighbours_and_marks_the_gaps() {
        assert_eq!(labels(&pager_items(0, 2)), ["Previous", "1", "2", "Next"]);
        assert_eq!(labels(&pager_items(4, 10)), ["Previous", "1", "\u{2026}", "4", "5", "6", "\u{2026}", "10", "Next"]);
        // A gap of one page isn't a gap: the page is shown.
        assert_eq!(labels(&pager_items(2, 5)), ["Previous", "1", "2", "3", "4", "5", "Next"]);
    }

    #[test]
    fn the_ends_of_a_pager_can_not_be_pressed_past() {
        let first = pager_items(0, 3);
        assert!(!first[0].enabled && first.last().unwrap().enabled);
        let last = pager_items(2, 3);
        assert!(last[0].enabled && !last.last().unwrap().enabled);
        let current: Vec<_> = last.iter().filter(|i| i.current).map(|i| i.label.to_string()).collect();
        assert_eq!(current, ["3"]);
    }
}

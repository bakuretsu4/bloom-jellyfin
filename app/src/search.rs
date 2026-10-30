//! Search: suggestions under the search field while you type, and a page of results.
//!
//! Typing waits for a pause (180 ms), then asks for at most eight titles and lists them under the
//! field; Enter on a row opens it, Enter with none chosen (or "See all results") opens the page.
//! Fewer than two characters matches nothing.

use crate::cards::{self, PlainCard};
use crate::nav::{self, Route};
use crate::text;
use crate::{App, CardData, SuggestionData};
use bloom_core::downloads::Downloads;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::library;
use bloom_core::Ctx;
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Suggestions to a search.
const SUGGESTIONS: u32 = 8;
/// Albums, artists and songs offered under the titles.
const MUSIC_SUGGESTIONS: u32 = 3;
/// How long the field waits for another keystroke.
const PAUSE: Duration = Duration::from_millis(180);
/// Cards to a page of results. Mirrors the core's page size.
const PAGE: usize = 60;

/// Each search takes the next number; only the latest one's answer is used.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// "Film", "Show", ...
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "Movie" => "Film",
        "Series" => "Show",
        "Episode" => "Episode",
        "Video" => "Video",
        "BoxSet" => "Collection",
        "MusicAlbum" => "Album",
        "MusicArtist" => "Artist",
        "Audio" => "Song",
        _ => "",
    }
}

fn on_ui(weak: &Weak<App>, f: impl FnOnce(&App) + Send + 'static) {
    let _ = weak.upgrade_in_event_loop(move |app| f(&app));
}

fn close(app: &App) {
    app.set_suggest_open(false);
    app.set_suggest_active(-1);
}

/// A keystroke in the field.
pub fn edited(ctx: Ctx, weak: Weak<App>, typed: String) {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let query = typed.trim().to_string();
    let w = weak.clone();
    on_ui(&w, move |app| {
        app.set_suggest_query(query.clone().into());
        app.set_suggest_active(-1);
        match query.chars().count() {
            0 => return close(app),
            1 => {
                app.set_suggest_status("short".into());
                app.set_suggest_open(true);
                return;
            }
            _ => {}
        }
        app.set_suggest_status("searching".into());
        app.set_suggest_open(true);
        let weak = app.as_weak();
        bloom_core::rt::spawn(async move {
            tokio::time::sleep(PAUSE).await;
            if SEQ.load(Ordering::SeqCst) != seq {
                return;
            }
            let jf = ctx.state::<Jellyfin>();
            // Music is a few extra lines after the titles, and never holds them up if it fails.
            let downloads = ctx.state::<Downloads>();
            let (found, music) = tokio::join!(
                library::search(&jf, &downloads, query.clone(), Some(SUGGESTIONS)),
                library::search_music_with(&jf, &query, MUSIC_SUGGESTIONS)
            );
            if SEQ.load(Ordering::SeqCst) != seq {
                return;
            }
            on_ui(&weak, move |app| {
                if SEQ.load(Ordering::SeqCst) != seq {
                    return;
                }
                let rows: Vec<SuggestionData> = found
                    .unwrap_or_default()
                    .into_iter()
                    .chain(music.unwrap_or_default())
                    .map(|c| SuggestionData { initial: text::initial(&c.title).into(), label: kind_label(&c.kind).into(), kind: c.kind.into(), title: c.title.into(), id: c.id.into() })
                    .collect();
                app.set_suggest_status(if rows.is_empty() { "none" } else { "list" }.into());
                app.set_suggestions(ModelRc::new(VecModel::from(rows)));
            });
        });
    });
}

/// Enter: the chosen suggestion, or all results when none is.
pub fn accepted(ctx: Ctx, weak: Weak<App>, typed: String) {
    let w = weak.clone();
    on_ui(&w, move |app| {
        let active = app.get_suggest_active();
        if app.get_suggest_open() && app.get_suggest_status() == "list" && active >= 0 && (active as usize) < app.get_suggestions().row_count() {
            return pick(ctx, weak, active);
        }
        see_all(ctx, weak, typed);
    });
}

/// Down: opens the list, or moves along it (the last row is "See all results").
pub fn down(app: &App) {
    if app.get_suggest_status() != "list" {
        return;
    }
    app.set_suggest_open(true);
    let rows = app.get_suggestions().row_count() as i32 + 1;
    app.set_suggest_active((app.get_suggest_active() + 1).rem_euclid(rows));
}

pub fn up(app: &App) {
    if app.get_suggest_status() != "list" {
        return;
    }
    let rows = app.get_suggestions().row_count() as i32 + 1;
    let now = app.get_suggest_active();
    app.set_suggest_active(if now <= 0 { rows - 1 } else { now - 1 });
}

/// Escape: closes the list; with it already closed, clears the field.
pub fn escape(ctx: Ctx, weak: Weak<App>) {
    on_ui(&weak.clone(), move |app| {
        if app.get_suggest_open() {
            return close(app);
        }
        if !app.get_search_text().is_empty() {
            app.set_search_text("".into());
            clear(ctx, weak);
        }
    });
}

/// A suggestion was chosen.
pub fn pick(ctx: Ctx, weak: Weak<App>, index: i32) {
    on_ui(&weak.clone(), move |app| {
        let Some(row) = usize::try_from(index).ok().and_then(|i| app.get_suggestions().row_data(i)) else { return };
        close(app);
        nav::activate(ctx, weak, row.id.to_string(), row.kind.to_string(), row.title.to_string());
    });
}

/// "See all results for ...", or Enter with nothing chosen.
pub fn see_all(ctx: Ctx, weak: Weak<App>, typed: String) {
    let query = typed.trim().to_string();
    SEQ.fetch_add(1, Ordering::SeqCst);
    on_ui(&weak.clone(), move |app| {
        close(app);
        if query.is_empty() {
            return;
        }
        nav::go(ctx, weak, Route::Search(query));
    });
}

/// "Clear search": back to where the search was started from.
pub fn clear(ctx: Ctx, weak: Weak<App>) {
    SEQ.fetch_add(1, Ordering::SeqCst);
    on_ui(&weak.clone(), move |app| {
        close(app);
        app.set_search_text("".into());
        nav::back(ctx, weak);
    });
}

/// The results page for `query`. UI thread.
pub fn show(ctx: &Ctx, weak: &Weak<App>, app: &App, query: String) {
    app.set_nav_target(crate::NavTarget::None);
    app.set_search_query(query.trim().into());
    app.set_search_error("".into());
    app.set_search_eyebrow("".into());
    app.set_search_cards(ModelRc::new(VecModel::<CardData>::default()));
    app.set_search_music_cards(ModelRc::new(VecModel::<CardData>::default()));
    app.set_search_loading(true);
    app.set_screen(crate::Screen::Search);
    let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let downloads = ctx.state::<Downloads>();
        let (found, music) = tokio::join!(
            library::search(&jf, &downloads, query.trim().to_string(), None),
            library::search_music_with(&jf, query.trim(), PAGE as u32)
        );
        let (ctx2, weak2) = (ctx.clone(), weak.clone());
        // Music that couldn't be searched is only missing; the titles still show.
        let music = music.unwrap_or_default();
        let found = match found {
            // Only music matched, or only the titles search failed: still a result.
            Err(_) if !music.is_empty() => Ok(Vec::new()),
            other => other,
        };
        match found {
            Ok(found) => {
                let plain: Vec<PlainCard> = found.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect();
                let music_plain: Vec<PlainCard> = music.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect();
                on_ui(&weak, move |app| {
                    if cards::generation() != generation {
                        return;
                    }
                    app.set_search_eyebrow(eyebrow(plain.len() + music_plain.len()).into());
                    let (tracker, model) = cards::card_model(&plain);
                    app.set_search_cards(ModelRc::from(model));
                    let (music_tracker, music_model) = cards::card_model(&music_plain);
                    app.set_search_music_cards(ModelRc::from(music_model));
                    app.set_search_loading(false);
                    cards::fetch_rows(&ctx2, &weak2, tracker, cards::jobs(&plain, 0));
                    cards::fetch_rows(&ctx2, &weak2, music_tracker, cards::jobs(&music_plain, 0));
                });
            }
            Err(e) => {
                let message = e.to_string();
                on_ui(&weak, move |app| {
                    if cards::generation() == generation {
                        app.set_search_loading(false);
                        app.set_search_error(message.into());
                    }
                });
            }
        }
    });
}

/// "First 60" when the page is full (there may be more), otherwise "3 results".
fn eyebrow(count: usize) -> String {
    if count == PAGE {
        format!("First {PAGE}")
    } else {
        text::count(count as u64, "result")
    }
}

/// "Try again" on the results page.
pub fn retry(ctx: Ctx, weak: Weak<App>) {
    on_ui(&weak.clone(), move |app| {
        let query = app.get_search_query().to_string();
        show(&ctx, &weak, app, query);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_named_as_the_original_names_them() {
        assert_eq!(kind_label("Movie"), "Film");
        assert_eq!(kind_label("Series"), "Show");
        assert_eq!(kind_label("BoxSet"), "Collection");
        assert_eq!(kind_label("Person"), "");
    }

    #[test]
    fn a_full_page_says_first_and_a_short_one_counts() {
        assert_eq!(eyebrow(60), "First 60");
        assert_eq!(eyebrow(1), "1 result");
        assert_eq!(eyebrow(7), "7 results");
    }
}

//! The hover card: after the pointer has rested on a film, show or episode card, a card with its
//! rating, years, synopsis and quick actions opens over the artwork.
//!
//! The window draws it (`HoverCard` in `ui/cards.slint`) and closes it on its own timing; this
//! fetches what it says and does what its buttons ask. Details are kept for a minute, up to 300
//! of them, so moving across a row doesn't ask the server for each card again.

use crate::title::{meta_parts, Meta};
use crate::watch;
use crate::{App, Hover, HoverData};
use bloom_core::home;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::library::{self, CardDetail};
use bloom_core::Ctx;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const FRESH: Duration = Duration::from_secs(60);
const MOST: usize = 300;

static CACHE: LazyLock<Mutex<HashMap<String, (Instant, CardDetail)>>> = LazyLock::new(Mutex::default);
/// The card the hover card is for, so a late answer for another one is dropped.
static CURRENT: Mutex<String> = Mutex::new(String::new());

fn cache() -> std::sync::MutexGuard<'static, HashMap<String, (Instant, CardDetail)>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

fn current() -> std::sync::MutexGuard<'static, String> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// A card's detail, as it was a moment ago if that's recent enough.
fn fresh(id: &str) -> Option<CardDetail> {
    cache().get(id).filter(|(at, _)| at.elapsed() < FRESH).map(|(_, d)| d.clone())
}

fn keep(id: &str, detail: &CardDetail) {
    let mut cache = cache();
    if cache.len() >= MOST {
        cache.retain(|_, (at, _)| at.elapsed() < FRESH);
        if cache.len() >= MOST {
            cache.clear();
        }
    }
    cache.insert(id.to_string(), (Instant::now(), detail.clone()));
}

/// Forgets what was known about an item, because it has changed.
pub fn forget(id: &str) {
    cache().remove(id);
}

/// "3 Apr 2026" from "2026-04-03".
fn short_date(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut parts = iso.split('-');
    let year: u32 = parts.next()?.parse().ok()?;
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let month = MONTHS.get(month.checked_sub(1)?)?;
    (1..=31).contains(&day).then(|| format!("{day} {month} {year}"))
}

/// "S1 E4" -> "E4".
fn episode_part(code: &str) -> &str {
    code.rsplit(' ').next().unwrap_or(code)
}

/// What the card says, from the detail.
fn data(d: &CardDetail) -> HoverData {
    let is_episode = d.kind == "Episode";
    let code = d.code.clone().unwrap_or_default();
    let meta = Meta {
        kind: d.kind.clone(),
        year: d.year,
        end_year: d.end_year,
        status: d.status.clone(),
        runtime: d.runtime_minutes,
        rating: d.community_rating,
        season_count: d.season_count.unwrap_or(0) as usize,
        // The card is narrow: the number of episodes is on the show's page.
        episode_count: None,
        unplayed: None,
    };
    let play_label = if is_episode && !code.is_empty() {
        let n = episode_part(&code);
        if d.resume {
            format!("Continue {n}")
        } else if d.played {
            format!("Watch again {n}")
        } else {
            format!("Play {n}")
        }
    } else if d.resume {
        "Resume".into()
    } else {
        "Play".into()
    };
    HoverData {
        id: d.id.clone().into(),
        kind: d.kind.clone().into(),
        title: d.title.clone().into(),
        series: d.series_title.clone().unwrap_or_default().into(),
        code: code.into(),
        // An episode's line is its date and whether it's watched; everything else's is the facts.
        meta: ModelRc::new(VecModel::from(if is_episode { Vec::new() } else { meta_parts(&meta) })),
        rating: if is_episode { String::new() } else { d.official_rating.clone().unwrap_or_default() }.into(),
        overview: d.overview.clone().unwrap_or_default().into(),
        aired: d.premiere_date.as_deref().and_then(short_date).map(|s| format!("Aired {s}")).unwrap_or_default().into(),
        favorite: d.favorite,
        played: d.played,
        resume: d.resume,
        play_label: play_label.into(),
        is_episode,
    }
}

/// The pointer has rested on a card: opens the hover card over it and fills it in.
pub fn open(ctx: Ctx, weak: Weak<App>, id: String, title: String, rect: (f32, f32, f32, f32)) {
    let Some(app) = weak.upgrade() else { return };
    let hover = app.global::<Hover>();
    hover.set_x(rect.0);
    hover.set_y(rect.1);
    hover.set_width(rect.2);
    hover.set_height(rect.3);
    hover.set_title(title.into());
    hover.set_failed(false);
    *current() = id.clone();
    if let Some(detail) = fresh(&id) {
        hover.set_data(data(&detail));
        hover.set_loading(false);
        hover.set_visible(true);
        return;
    }
    hover.set_loading(true);
    hover.set_visible(true);
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let result = library::card_detail_with(&jf, &id).await;
        let _ = weak.upgrade_in_event_loop(move |app| {
            if *current() != id {
                return;
            }
            let hover = app.global::<Hover>();
            match result {
                Ok(detail) => {
                    keep(&id, &detail);
                    hover.set_data(data(&detail));
                    hover.set_loading(false);
                }
                Err(_) => {
                    hover.set_loading(false);
                    hover.set_failed(true);
                }
            }
        });
    });
}

/// Away with it (a page change, for one). UI thread.
pub fn close(app: &App) {
    app.global::<Hover>().set_visible(false);
}

pub fn play(ctx: Ctx, weak: Weak<App>, id: String) {
    watch::play(ctx, weak, id);
}

/// The heart: favourite or not, as the server then says.
pub fn toggle_favorite(ctx: Ctx, weak: Weak<App>) {
    let Some(app) = weak.upgrade() else { return };
    let data = app.global::<Hover>().get_data();
    let (id, favorite) = (data.id.to_string(), !data.favorite);
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        if let Ok(now) = home::set_favorite_with(&jf, id.clone(), favorite).await {
            forget(&id);
            let _ = weak.upgrade_in_event_loop(move |app| {
                let hover = app.global::<Hover>();
                let mut data = hover.get_data();
                if data.id == id.as_str() {
                    data.favorite = now;
                    hover.set_data(data);
                }
            });
        }
    });
}

/// The tick: watched or not, as the server then says. Watching it clears a resume point.
pub fn toggle_played(ctx: Ctx, weak: Weak<App>) {
    let Some(app) = weak.upgrade() else { return };
    let data = app.global::<Hover>().get_data();
    let (id, played) = (data.id.to_string(), !data.played);
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        if let Ok(now) = library::set_played_with(&jf, &id, played).await {
            forget(&id);
            let _ = weak.upgrade_in_event_loop(move |app| {
                let hover = app.global::<Hover>();
                let mut data = hover.get_data();
                if data.id == id.as_str() {
                    data.played = now;
                    if now {
                        data.resume = false;
                    }
                    hover.set_data(data);
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_short_and_bad_ones_are_left_out() {
        assert_eq!(short_date("2026-04-03").as_deref(), Some("3 Apr 2026"));
        assert_eq!(short_date("2026-13-03"), None);
        assert_eq!(short_date("nope"), None);
    }

    #[test]
    fn an_episode_says_which_one_to_play() {
        assert_eq!(episode_part("S1 E4"), "E4");
        assert_eq!(episode_part("E4"), "E4");
    }
}

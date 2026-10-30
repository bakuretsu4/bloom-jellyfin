//! Lists of cards on screen, and the artwork that fills them in as it arrives.
//!
//! Every list (a Home section, a library page, search results, the cast, the episodes) is a
//! `VecModel` whose rows get their pictures later. A model is registered once with `track`, which
//! hands back an id; artwork tasks name the model by that id and the row, and the picture is set
//! on the UI thread if the model is still on screen. A list that has been replaced is simply gone
//! from the registry, so late artwork for it is dropped without any bookkeeping.

use crate::{downloads, App, CardData};
use bloom_core::downloads::Downloads;
use bloom_core::home::Card;
use bloom_core::images;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::Ctx;
use slint::{Image, Model, Rgba8Pixel, SharedPixelBuffer, VecModel, Weak};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use tokio::sync::Semaphore;

/// Artwork fetches in flight at once.
const FETCHES: usize = 8;
static PERMITS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(FETCHES));

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// Bumped on every change of page, so a single picture (a backdrop, a logo) that arrives for a
/// page that has since been left is dropped.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn new_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// A card as plain, sendable data, before it becomes a row.
#[derive(Clone)]
pub struct PlainCard {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub progress: f32,
    /// Where the artwork is, as `images::fetch_with` takes it.
    pub image: Option<String>,
    /// For an episode: its show.
    pub series_id: String,
}

impl PlainCard {
    /// `width` is the artwork's width in pixels (the core rounds it up to a cached size).
    pub fn from_core(card: &Card, width: u32) -> Self {
        // A film, show or collection shown with its own poster is one the sign-in wall may use.
        if matches!(card.kind.as_str(), "Movie" | "Series" | "BoxSet") && card.image.is_some() {
            crate::wall::note(&card.id);
        }
        Self {
            id: card.id.clone(),
            kind: card.kind.clone(),
            title: card.title.clone(),
            subtitle: card.meta.clone().unwrap_or_default(),
            progress: card.progress.unwrap_or(0.0) as f32,
            image: card.image.as_ref().map(|i| i.path(width)),
            series_id: card.series_id.clone().unwrap_or_default(),
        }
    }

    pub fn row(&self) -> CardData {
        CardData {
            id: self.id.clone().into(),
            kind: self.kind.clone().into(),
            title: self.title.clone().into(),
            subtitle: self.subtitle.clone().into(),
            image: Image::default(),
            has_image: false,
            has_art: self.image.is_some(),
            progress: self.progress,
            series_id: self.series_id.clone().into(),
            downloaded: downloads::has_downloaded(&self.id),
            tint: tint_for(&self.title),
        }
    }
}

/// Muted tints of one lightness, so title cards read as a set.
const TINTS: [u32; 7] = [0x5A6B78, 0x63705C, 0x54636B, 0x5E6A74, 0x6E6A64, 0x6B5F66, 0x5F6F6A];

/// The tint of a title's card: the same for the same title.
pub fn tint_for(title: &str) -> slint::Color {
    let mut h: u32 = 7;
    for unit in title.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(unit));
    }
    let rgb = TINTS[h as usize % TINTS.len()];
    slint::Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// The artwork width for a card shape.
pub fn card_width(wide: bool) -> u32 {
    if wide {
        480
    } else {
        360
    }
}

struct Setter {
    alive: Box<dyn Fn() -> bool>,
    set: Box<dyn Fn(usize, Image)>,
    /// The picture for a row couldn't be had.
    fail: Box<dyn Fn(usize)>,
}

thread_local! {
    /// Every model of cards on screen, so a download finishing can add or drop the badge. UI thread.
    static CARD_MODELS: RefCell<Vec<std::rc::Weak<VecModel<CardData>>>> = const { RefCell::new(Vec::new()) };
    /// The models on screen that are waiting for pictures. UI thread only.
    static SETTERS: RefCell<HashMap<u64, Setter>> = RefCell::new(HashMap::new());
}

/// Registers `model` for artwork, with `set` putting a picture into a row. UI thread.
pub fn track<T: Clone + 'static>(model: &Rc<VecModel<T>>, set: fn(&mut T, Image)) -> u64 {
    track_with(model, set, |_| {})
}

/// As `track`, and `fail` changes a row whose picture couldn't be had (so it can name itself).
pub fn track_with<T: Clone + 'static>(model: &Rc<VecModel<T>>, set: fn(&mut T, Image), fail: fn(&mut T)) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let (alive, weak, weak_fail) = (Rc::downgrade(model), Rc::downgrade(model), Rc::downgrade(model));
    SETTERS.with(|s| {
        let mut s = s.borrow_mut();
        // Models that have gone off screen are dropped as new ones arrive.
        s.retain(|_, v| (v.alive)());
        s.insert(
            id,
            Setter {
                alive: Box::new(move || alive.strong_count() > 0),
                set: Box::new(move |row, image| {
                    let Some(model) = weak.upgrade() else { return };
                    if let Some(mut data) = model.row_data(row) {
                        set(&mut data, image);
                        model.set_row_data(row, data);
                    }
                }),
                fail: Box::new(move |row| {
                    let Some(model) = weak_fail.upgrade() else { return };
                    if let Some(mut data) = model.row_data(row) {
                        fail(&mut data);
                        model.set_row_data(row, data);
                    }
                }),
            },
        );
    });
    id
}

/// A model of cards, registered for artwork. UI thread.
pub fn card_model(cards: &[PlainCard]) -> (u64, Rc<VecModel<CardData>>) {
    let model = Rc::new(VecModel::from(cards.iter().map(PlainCard::row).collect::<Vec<_>>()));
    let id = track_with(
        &model,
        |row: &mut CardData, image| {
            row.image = image;
            row.has_image = true;
        },
        |row: &mut CardData| row.has_art = false,
    );
    CARD_MODELS.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|w| w.strong_count() > 0);
        m.push(Rc::downgrade(&model));
    });
    (id, model)
}

/// The downloads changed: the cards on screen show or lose their "downloaded" badge. UI thread.
pub fn refresh_downloaded() {
    CARD_MODELS.with(|m| {
        for model in m.borrow().iter().filter_map(std::rc::Weak::upgrade) {
            for i in 0..model.row_count() {
                let Some(mut row) = model.row_data(i) else { continue };
                let now = downloads::has_downloaded(&row.id);
                if row.downloaded != now {
                    row.downloaded = now;
                    model.set_row_data(i, row);
                }
            }
        }
    });
}

/// The server says an item's watched state or progress changed: the cards for it show so. UI thread.
pub fn apply_change(change: &crate::live::Change) {
    CARD_MODELS.with(|m| {
        for model in m.borrow().iter().filter_map(std::rc::Weak::upgrade) {
            for i in 0..model.row_count() {
                let Some(mut row) = model.row_data(i).filter(|r| r.id == change.item_id.as_str()) else { continue };
                // A finished item has no progress bar; an unfinished one shows how far it got.
                row.progress = if change.played { 0.0 } else { change.progress.unwrap_or(0.0) as f32 };
                model.set_row_data(i, row);
            }
        }
    });
}

/// An image's pixels, from the core's cache or the server, decoded on the blocking pool.
pub async fn pixels(ctx: &Ctx, path: &str) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let _permit = PERMITS.acquire().await.ok()?;
    let jf = ctx.state::<Jellyfin>();
    let downloads = ctx.state::<Downloads>();
    let bytes = images::fetch_with(&jf, Some(&downloads), path).await.ok()?;
    bloom_core::rt::spawn_blocking(move || {
        let rgba = image::load_from_memory(&bytes).ok()?.into_rgba8();
        let (w, h) = rgba.dimensions();
        Some(SharedPixelBuffer::clone_from_slice(rgba.as_raw(), w, h))
    })
    .await
    .ok()?
}

/// Fetches the pictures for rows of the model registered as `id`: `(row, path)` for each.
pub fn fetch_rows(ctx: &Ctx, weak: &Weak<App>, id: u64, jobs: Vec<(usize, String)>) {
    for (row, path) in jobs {
        let (ctx, weak) = (ctx.clone(), weak.clone());
        bloom_core::rt::spawn(async move {
            let pixels = pixels(&ctx, &path).await;
            let _ = weak.upgrade_in_event_loop(move |_| {
                SETTERS.with(|s| {
                    if let Some(setter) = s.borrow().get(&id) {
                        match pixels {
                            Some(pixels) => (setter.set)(row, Image::from_rgba8(pixels)),
                            None => (setter.fail)(row),
                        }
                    }
                });
            });
        });
    }
}

/// The jobs for a list of cards: those with artwork, numbered from `first`.
pub fn jobs(cards: &[PlainCard], first: usize) -> Vec<(usize, String)> {
    cards.iter().enumerate().filter_map(|(i, c)| c.image.clone().map(|p| (first + i, p))).collect()
}

/// Fetches one picture and hands it to `apply` on the UI thread, if the page it was for is still
/// the page on screen (`generation` is `generation()` as it was when the page was asked for).
pub fn fetch_one(ctx: &Ctx, weak: &Weak<App>, generation: u64, path: String, apply: impl FnOnce(&App, Image) + Send + 'static) {
    let (ctx, weak) = (ctx.clone(), weak.clone());
    bloom_core::rt::spawn(async move {
        let Some(pixels) = pixels(&ctx, &path).await else { return };
        let _ = weak.upgrade_in_event_loop(move |app| {
            if self::generation() == generation {
                apply(&app, Image::from_rgba8(pixels));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_always_gets_the_same_tint_and_titles_spread_over_them() {
        assert_eq!(tint_for("Tokyo Ghoul"), tint_for("Tokyo Ghoul"));
        let tints: std::collections::HashSet<_> = ["A", "B", "C", "D", "E", "F", "G", "H", "I"].iter().map(|t| format!("{:?}", tint_for(t))).collect();
        assert!(tints.len() > 1, "every title got one tint");
    }
}

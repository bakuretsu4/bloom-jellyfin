//! The command palette (Ctrl+K): jump to a page, a library, a settings section or a title.
//!
//! Rows are ranked by how well what was typed matches the label (and some extra words): a prefix
//! of the label, a prefix of one of its words, anywhere in it, then a prefix of a keyword. Titles
//! come from the server after a pause (180 ms), eight at most.

use crate::nav::{self, Route};
use crate::{App, PaletteRow};
use bloom_core::downloads::Downloads;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::library;
use bloom_core::Ctx;
use slint::{Model, ModelRc, VecModel, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const TITLES: u32 = 8;
const PAUSE: Duration = Duration::from_millis(180);

/// How well a row matches what was typed (lower case): 4 a prefix of the label, 3 a prefix of one
/// of its words, 2 inside it, 1 a prefix of one of the extra words, -1 not at all.
pub fn score(label: &str, words: &str, typed: &str) -> i32 {
    let name = label.to_lowercase();
    if name.starts_with(typed) {
        return 4;
    }
    if name.split_whitespace().any(|w| w.starts_with(typed)) {
        return 3;
    }
    if name.contains(typed) {
        return 2;
    }
    if words.split_whitespace().any(|w| w.starts_with(typed)) {
        return 1;
    }
    -1
}

/// What choosing a row does.
#[derive(Clone)]
enum Action {
    Go(Route),
    Shortcuts,
    Library { id: String, name: String },
    Settings,
    Title { id: String, kind: String, title: String },
}

struct State {
    query: String,
    libraries: Vec<(String, String)>,
    /// Titles that answer `answered`.
    titles: Vec<(String, String, String, String)>,
    answered: String,
    rows: Vec<(PaletteRowPlain, Action)>,
}

#[derive(Clone)]
struct PaletteRowPlain {
    group: &'static str,
    label: String,
    detail: String,
    icon: &'static str,
}

static STATE: Mutex<State> = Mutex::new(State { query: String::new(), libraries: Vec::new(), titles: Vec::new(), answered: String::new(), rows: Vec::new() });
static REQUEST: AtomicU64 = AtomicU64::new(0);

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const PLACES: [(&str, &str, &str, &str); 5] = [
    ("Home", "home", "start continue watching next up spotlight", "home"),
    ("Libraries", "library", "library films shows browse", "libraries"),
    ("Downloads", "downloads", "offline saved storage", "downloads"),
    ("Settings", "settings", "preferences options", "settings"),
    ("Servers", "servers", "server accounts sign in jellyfin", "servers"),
];

const SECTIONS: [(&str, &str); 6] = [
    ("Playback", "decoder hardware quality direct play audio subtitles language autoplay"),
    ("Notifications", "notify new episodes films alerts added"),
    ("Discord", "presence rich status profile"),
    ("Downloads", "location folder quality network at once parallel"),
    ("Appearance", "theme dark light accent colour color zoom reduce motion"),
    ("About", "version server graphics player"),
];

fn kind_label(kind: &str) -> String {
    match kind {
        "Movie" => "Film",
        "Series" => "Show",
        "Episode" => "Episode",
        "Video" => "Video",
        "BoxSet" => "Collection",
        other => other,
    }
    .to_string()
}

/// The rows for what has been typed.
fn rows_for(s: &State) -> Vec<(PaletteRowPlain, Action)> {
    let typed = s.query.trim().to_lowercase();
    let ranked = |items: Vec<(String, String, Action, PaletteRowPlain)>| -> Vec<(PaletteRowPlain, Action)> {
        if typed.is_empty() {
            return items.into_iter().map(|(_, _, a, r)| (r, a)).collect();
        }
        let mut scored: Vec<(i32, (PaletteRowPlain, Action))> =
            items.into_iter().map(|(label, words, a, r)| (score(&label, &words, &typed), (r, a))).filter(|(s, _)| *s >= 0).collect();
        // Best first; equal scores keep their order.
        scored.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        scored.into_iter().map(|(_, row)| row).collect()
    };
    let mut out = Vec::new();

    let places = PLACES
        .iter()
        .map(|(label, icon, words, place)| {
            let route = match *place {
                "home" => Route::Home,
                "libraries" => Route::Libraries,
                "downloads" => Route::Downloads,
                "servers" => Route::Servers,
                _ => Route::Settings,
            };
            (label.to_string(), words.to_string(), Action::Go(route), PaletteRowPlain { group: "Go to", label: label.to_string(), detail: String::new(), icon })
        })
        .collect();
    out.extend(ranked(places));
    if typed.is_empty() || score("Keyboard shortcuts", "keys help", &typed) >= 0 {
        out.push((PaletteRowPlain { group: "Go to", label: "Keyboard shortcuts".into(), detail: String::new(), icon: "help" }, Action::Shortcuts));
    }

    let libraries = s
        .libraries
        .iter()
        .map(|(id, name)| (name.clone(), "library".to_string(), Action::Library { id: id.clone(), name: name.clone() }, PaletteRowPlain { group: "Libraries", label: name.clone(), detail: String::new(), icon: "library" }))
        .collect();
    out.extend(ranked(libraries));

    if !typed.is_empty() {
        let sections = SECTIONS.iter().map(|(label, words)| (label.to_string(), words.to_string(), Action::Settings, PaletteRowPlain { group: "Settings", label: label.to_string(), detail: "Settings".into(), icon: "settings" })).collect();
        out.extend(ranked(sections));
        for (id, kind, title, meta) in &s.titles {
            let detail = [kind_label(kind), meta.clone()].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ");
            out.push((PaletteRowPlain { group: "Titles", label: title.clone(), detail, icon: "title" }, Action::Title { id: id.clone(), kind: kind.clone(), title: title.clone() }));
        }
    }
    out
}

/// What the palette says under its rows when it has something to say.
fn note_for(s: &State, rows: usize) -> String {
    let typed = s.query.trim();
    let chars = typed.chars().count();
    if chars >= 2 && s.answered != typed {
        "Searching titles".into()
    } else if chars == 1 {
        "Keep typing to search titles".into()
    } else if chars >= 2 && rows == 0 {
        format!("Nothing called \u{201c}{typed}\u{201d}")
    } else {
        String::new()
    }
}

/// Puts the rows on screen. UI thread.
fn show_rows(app: &App) {
    let (rows, note) = {
        let mut s = state();
        let rows = rows_for(&s);
        let note = note_for(&s, rows.len());
        s.rows = rows.clone();
        (rows, note)
    };
    let mut previous = "";
    let model: Vec<PaletteRow> = rows
        .iter()
        .map(|(r, _)| {
            let first = r.group != previous;
            previous = r.group;
            PaletteRow { group: r.group.into(), label: r.label.clone().into(), detail: r.detail.clone().into(), icon: r.icon.into(), first_in_group: first }
        })
        .collect();
    app.set_palette_rows(ModelRc::new(VecModel::from(model)));
    app.set_palette_note(note.into());
    let count = rows.len() as i32;
    if app.get_palette_active() >= count {
        app.set_palette_active(0);
    }
}

/// Ctrl+K opened it.
pub fn opened(ctx: Ctx, weak: Weak<App>) {
    REQUEST.fetch_add(1, Ordering::SeqCst);
    {
        let mut s = state();
        s.query.clear();
        s.titles.clear();
        s.answered.clear();
    }
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(|app| {
        app.set_palette_query("".into());
        app.set_palette_active(0);
        show_rows(&app);
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        if let Ok(found) = library::libraries_with(&jf).await {
            state().libraries = found.into_iter().map(|c| (c.id, c.title)).collect();
            let _ = weak.upgrade_in_event_loop(|app| {
                if app.get_palette_open() {
                    show_rows(&app);
                }
            });
        }
    });
}

/// A keystroke in the field.
pub fn edited(ctx: Ctx, weak: Weak<App>, typed: String) {
    let mine = REQUEST.fetch_add(1, Ordering::SeqCst) + 1;
    let wanted = typed.trim().to_string();
    {
        let mut s = state();
        s.query = typed;
        if wanted.chars().count() < 2 {
            s.titles.clear();
            s.answered = wanted.clone();
        }
    }
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(|app| {
        app.set_palette_active(0);
        show_rows(&app);
    });
    if wanted.chars().count() < 2 {
        return;
    }
    bloom_core::rt::spawn(async move {
        tokio::time::sleep(PAUSE).await;
        if REQUEST.load(Ordering::SeqCst) != mine {
            return;
        }
        let jf = ctx.state::<Jellyfin>();
        let found = library::search(&jf, &ctx.state::<Downloads>(), wanted.clone(), Some(TITLES)).await;
        if REQUEST.load(Ordering::SeqCst) != mine {
            return;
        }
        {
            let mut s = state();
            s.titles = found.unwrap_or_default().into_iter().map(|c| (c.id, c.kind, c.title, c.meta.unwrap_or_default())).collect();
            s.answered = wanted;
        }
        let _ = weak.upgrade_in_event_loop(|app| {
            if app.get_palette_open() {
                show_rows(&app);
            }
        });
    });
}

/// Up or down, wrapping.
pub fn move_active(app: &App, delta: i32) {
    let count = app.get_palette_rows().row_count() as i32;
    if count == 0 {
        return;
    }
    app.set_palette_active((app.get_palette_active() + delta).rem_euclid(count));
}

/// Enter, or a press, on row `index`: it closes, then does its thing.
pub fn accept(ctx: Ctx, weak: Weak<App>, index: i32) {
    let action = usize::try_from(index).ok().and_then(|i| state().rows.get(i).map(|(_, a)| a.clone()));
    let Some(action) = action else { return };
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        app.set_palette_open(false);
        match action {
            Action::Go(Route::Home) => nav::reset(ctx, weak, Route::Home),
            Action::Go(route) => nav::section(ctx, weak, route),
            Action::Shortcuts => app.set_shortcuts_open(true),
            Action::Library { id, name } => {
                nav::section(ctx.clone(), weak.clone(), Route::Libraries);
                nav::go(ctx, weak, Route::Library { id, name });
            }
            Action::Settings => nav::section(ctx, weak, Route::Settings),
            Action::Title { id, kind, title } => nav::activate(ctx, weak, id, kind, title),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_beats_a_word_prefix_beats_inside_beats_a_keyword() {
        assert_eq!(score("Downloads", "offline saved", "down"), 4);
        assert_eq!(score("Keyboard shortcuts", "keys help", "sho"), 3);
        assert_eq!(score("Libraries", "films", "bra"), 2);
        assert_eq!(score("Downloads", "offline saved storage", "stor"), 1);
        assert_eq!(score("Home", "start", "zzz"), -1);
    }

    #[test]
    fn nothing_typed_lists_the_pages_and_typing_narrows_them() {
        let mut s = State { query: String::new(), libraries: vec![("1".into(), "Movies".into())], titles: Vec::new(), answered: String::new(), rows: Vec::new() };
        let all = rows_for(&s);
        let labels: Vec<&str> = all.iter().map(|(r, _)| r.label.as_str()).collect();
        assert_eq!(labels, ["Home", "Libraries", "Downloads", "Settings", "Servers", "Keyboard shortcuts", "Movies"]);
        s.query = "mov".into();
        let some: Vec<&str> = rows_for(&s).iter().map(|(r, _)| r.label.as_str()).collect::<Vec<_>>().into_iter().map(|l| if l == "Movies" { "Movies" } else { "other" }).collect();
        assert!(some.contains(&"Movies"));
        s.query = "zzzz".into();
        assert!(rows_for(&s).is_empty());
    }

    #[test]
    fn the_note_says_what_is_going_on() {
        let mut s = State { query: "m".into(), libraries: Vec::new(), titles: Vec::new(), answered: String::new(), rows: Vec::new() };
        assert_eq!(note_for(&s, 3), "Keep typing to search titles");
        s.query = "mo".into();
        assert_eq!(note_for(&s, 3), "Searching titles");
        s.answered = "mo".into();
        assert_eq!(note_for(&s, 0), "Nothing called \u{201c}mo\u{201d}");
        assert_eq!(note_for(&s, 2), "");
    }
}

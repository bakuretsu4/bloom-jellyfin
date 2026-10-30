//! The custom-theme editor in Settings → Appearance: the themes made so far, and the one being
//! made or changed. The draft is kept here and written into the `ThemeEditor` global after every
//! change; the model, checks and CSS are in `themes.rs`.

use crate::{presets, theme, themes, App, SettingsData, ThemeCheck, ThemeChip, ThemeEditor};
use bloom_core::discord::Presence;
use bloom_core::downloads::Downloads;
use bloom_core::playback::Playback;
use bloom_core::settings::{self, CustomTheme, Settings, SettingsStore, ThemeBase};
use bloom_core::Ctx;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;

struct Draft {
    theme: CustomTheme,
    existing: bool,
    as_css: bool,
    problems: Vec<String>,
    copied: bool,
}

static DRAFT: LazyLock<Mutex<Option<Draft>>> = LazyLock::new(Mutex::default);

/// The fonts installed on the computer, read the first time the editor opens.
static FONTS: OnceLock<Vec<String>> = OnceLock::new();

/// The font that is Bloom's own.
const OWN_FONT: &str = "Bloom's own font";

fn colour(hex: &str) -> slint::Color {
    let rgb = themes::parse_hex(hex).unwrap_or([128.0; 3]);
    slint::Color::from_rgb_u8(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8)
}

fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("t{nanos:x}-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn chip(t: &CustomTheme, active: bool) -> ThemeChip {
    ThemeChip { id: t.id.clone().into(), name: t.name.clone().into(), ground: colour(&t.ground), surface: colour(&t.surface), ink: colour(&t.ink), accent: colour(&t.accent), active }
}

/// The themes made so far, in the list, and the ready-made ones to choose from.
pub fn refresh_chips(app: &App, saved: &Settings) {
    let chips: Vec<ThemeChip> = saved.custom_themes.iter().map(|t| chip(t, saved.custom_theme.as_deref() == Some(t.id.as_str()))).collect();
    let editor = app.global::<ThemeEditor>();
    editor.set_chips(ModelRc::new(VecModel::from(chips)));
    let presets: Vec<ThemeChip> = presets::all().iter().map(|p| chip(&p.theme, saved.custom_theme.as_deref() == Some(p.id.as_str()))).collect();
    editor.set_presets(ModelRc::new(VecModel::from(presets)));
}

/// The editor as the screen opens: the list, and no draft.
pub fn show(ctx: &Ctx, app: &App) {
    *DRAFT.lock().unwrap() = None;
    let editor = app.global::<ThemeEditor>();
    editor.set_editing(false);
    refresh_chips(app, &ctx.state::<SettingsStore>().get());
}

/// The options of the font select: Bloom's own, then what is installed, then (when a pasted theme
/// names one that isn't) that one, marked.
fn font_options(font: Option<&str>) -> (Vec<SharedString>, i32) {
    let installed = FONTS.get();
    let mut names = vec![OWN_FONT.to_string()];
    if let Some(fonts) = installed {
        names.extend(fonts.iter().filter(|f| f.as_str() != "Archivo" && f.as_str() != theme::FONT).cloned());
    }
    let index = match font {
        None => 0,
        Some(f) => match names.iter().position(|n| n == f) {
            Some(i) => i,
            None => {
                names.push(format!("{f} (not installed)"));
                names.len() - 1
            }
        },
    };
    (names.into_iter().map(SharedString::from).collect(), index as i32)
}

/// What to leave as the person typed it.
#[derive(Clone, Copy, Default)]
struct Keep {
    hex: Option<usize>,
    css: bool,
}

fn render(app: &App, draft: &Draft, keep: Keep) {
    let e = app.global::<ThemeEditor>();
    let t = &draft.theme;
    let tokens = themes::colour_tokens(t);
    let checks = themes::check_theme(t);
    e.set_editing(true);
    e.set_existing(draft.existing);
    e.set_dark(t.base == ThemeBase::Dark);
    e.set_as_css(draft.as_css);
    e.set_usable(checks.iter().all(themes::Check::ok));
    e.set_copied(draft.copied);
    e.set_name(t.name.clone().into());
    for (i, hex) in [&t.ground, &t.surface, &t.ink, &t.accent].into_iter().enumerate() {
        if keep.hex != Some(i) {
            match i {
                0 => e.set_hex_ground(hex.clone().into()),
                1 => e.set_hex_surface(hex.clone().into()),
                2 => e.set_hex_ink(hex.clone().into()),
                _ => e.set_hex_accent(hex.clone().into()),
            }
        }
    }
    e.set_ground(colour(&t.ground));
    e.set_surface(colour(&t.surface));
    e.set_ink(colour(&t.ink));
    e.set_accent(colour(&t.accent));
    e.set_ink_2(colour(&tokens["--ink-2"]));
    e.set_ink_3(colour(&tokens["--ink-3"]));
    e.set_accent_ink(colour(&tokens["--accent-ink"]));
    e.set_line(colour(&tokens["--line"]));
    if draft.as_css && !keep.css {
        e.set_css_text(themes::to_css(t).into());
    }
    e.set_problems(draft.problems.join("\n").into());
    let by_hand = t.tokens.len();
    e.set_hand_set(if by_hand == 0 { SharedString::new() } else { format!("{by_hand} {} set by hand in CSS.", if by_hand == 1 { "shade" } else { "shades" }).into() });
    let (options, index) = font_options(t.font.as_deref());
    e.set_fonts(ModelRc::new(VecModel::from(options)));
    e.set_font_index(index);
    e.set_fonts_ready(FONTS.get().is_some());
    let shown: Vec<ThemeCheck> = checks
        .iter()
        .map(|c| ThemeCheck {
            label: c.label.into(),
            ratio: if c.ok() { format!("{:.1}:1", c.ratio) } else { format!("{:.1}:1, needs {}:1", c.ratio, c.needed) }.into(),
            ok: c.ok(),
        })
        .collect();
    e.set_checks(ModelRc::new(VecModel::from(shown)));
}

/// Opens a draft and shows it; the fonts are read the first time.
fn open(app: &App, theme: CustomTheme, existing: bool) {
    app.global::<ThemeEditor>().set_picker(-1);
    let draft = Draft { theme, existing, as_css: false, problems: Vec::new(), copied: false };
    render(app, &draft, Keep::default());
    *DRAFT.lock().unwrap() = Some(draft);
    if FONTS.get().is_none() {
        let weak = app.as_weak();
        bloom_core::rt::spawn(async move {
            let found = settings::system_fonts().await;
            let _ = FONTS.set(found);
            let _ = weak.upgrade_in_event_loop(|app| {
                if let Some(draft) = DRAFT.lock().unwrap().as_ref() {
                    render(&app, draft, Keep { hex: None, css: true });
                }
            });
        });
    }
}

/// Changes the draft, and shows it.
fn edit(weak: &Weak<App>, keep: Keep, change: impl FnOnce(&mut Draft)) {
    let Some(app) = weak.upgrade() else { return };
    let mut guard = DRAFT.lock().unwrap();
    if let Some(draft) = guard.as_mut() {
        change(draft);
        render(&app, draft, keep);
    }
}

/// Saves a change of settings, applies the theme it implies, and brings the screen in step.
fn commit(ctx: &Ctx, app: &App, change: impl FnOnce(&mut Settings)) {
    let store = ctx.state::<SettingsStore>();
    let mut next = store.get();
    change(&mut next);
    let saved = settings::update_settings(ctx, &store, &ctx.state::<Presence>(), &ctx.state::<Downloads>(), &ctx.state::<Playback>(), next);
    theme::apply_settings(app, &saved, true);
    refresh_chips(app, &saved);
    let mut shown: SettingsData = app.get_settings_data();
    shown.custom_active = saved.custom_theme.is_some();
    app.set_settings_data(shown);
}

pub fn wire(app: &App, ctx: &Ctx) {
    let e = app.global::<ThemeEditor>();
    let weak = app.as_weak();
    e.on_use_theme({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| {
            if let Some(app) = weak.upgrade() {
                commit(&ctx, &app, |s| s.custom_theme = Some(id.to_string()));
            }
        }
    });
    e.on_use_preset({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| {
            let (Some(app), Some(preset)) = (weak.upgrade(), presets::find(id.as_str())) else { return };
            commit(&ctx, &app, |s| {
                // The last preset taken (and not edited, which would have made it the person's own)
                // gives way, so choosing around doesn't fill the list.
                s.custom_themes.retain(|t| !t.id.starts_with(presets::ID_PREFIX) || t.id == preset.id);
                if !s.custom_themes.iter().any(|t| t.id == preset.id) {
                    s.custom_themes.push(preset.theme.clone());
                }
                s.custom_theme = Some(preset.id.clone());
            });
        }
    });
    e.on_new_theme({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let [ground, surface, ink, accent] = themes::start(ThemeBase::Dark);
            let theme = CustomTheme { id: new_id(), name: "My theme".into(), base: ThemeBase::Dark, ground: ground.into(), surface: surface.into(), ink: ink.into(), accent: accent.into(), ..CustomTheme::default() };
            open(&app, theme, false);
        }
    });
    e.on_edit_theme({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let found = ctx.state::<SettingsStore>().get().custom_themes.into_iter().find(|t| t.id == id.as_str());
            if let Some(theme) = found {
                open(&app, theme, true);
            }
        }
    });
    e.on_name_edited({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let text = app.global::<ThemeEditor>().get_name().to_string();
            if let Some(draft) = DRAFT.lock().unwrap().as_mut() {
                draft.theme.name = text.chars().take(40).collect();
            }
        }
    });
    e.on_set_dark({
        let weak = weak.clone();
        move |dark| {
            let base = if dark { ThemeBase::Dark } else { ThemeBase::Light };
            edit(&weak, Keep::default(), |d| {
                // Light and dark start from Bloom's own colours for that side.
                if d.theme.base != base {
                    let [ground, surface, ink, accent] = themes::start(base);
                    d.theme = CustomTheme { base, ground: ground.into(), surface: surface.into(), ink: ink.into(), accent: accent.into(), ..d.theme.clone() };
                }
            });
        }
    });
    e.on_hex_edited({
        let weak = weak.clone();
        move |which, done| {
            let Some(app) = weak.upgrade() else { return };
            let editor = app.global::<ThemeEditor>();
            let typed = match which {
                0 => editor.get_hex_ground(),
                1 => editor.get_hex_surface(),
                2 => editor.get_hex_ink(),
                _ => editor.get_hex_accent(),
            }
            .to_string();
            let typed = if typed.trim().starts_with('#') { typed } else { format!("#{}", typed.trim()) };
            // While typing, a colour counts once it is whole; Enter takes the short form too.
            let hex = if done { themes::normalise_hex(&typed) } else { Some(typed.clone()).filter(|t| t.trim().len() == 7).and_then(|t| themes::normalise_hex(&t)) };
            let keep = Keep { hex: (!done).then_some(which as usize), css: false };
            edit(&weak, keep, |d| {
                if let Some(hex) = hex {
                    match which {
                        0 => d.theme.ground = hex,
                        1 => d.theme.surface = hex,
                        2 => d.theme.ink = hex,
                        _ => d.theme.accent = hex,
                    }
                }
            });
        }
    });
    e.on_colour_picked({
        let weak = weak.clone();
        move |which, picked| {
            // The picker is dragging a colour: the hex fields follow, and the checks with them.
            let hex = format!("#{:02x}{:02x}{:02x}", picked.red(), picked.green(), picked.blue());
            edit(&weak, Keep::default(), |d| match which {
                0 => d.theme.ground = hex,
                1 => d.theme.surface = hex,
                2 => d.theme.ink = hex,
                _ => d.theme.accent = hex,
            });
        }
    });
    e.on_toggle_css({
        let weak = weak.clone();
        move || {
            edit(&weak, Keep::default(), |d| {
                d.as_css = !d.as_css;
                d.problems.clear();
            });
        }
    });
    e.on_css_edited({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let text = app.global::<ThemeEditor>().get_css_text().to_string();
            edit(&weak, Keep { hex: None, css: true }, |d| {
                let (theme, problems) = themes::from_css(&text, &d.theme);
                d.theme = theme;
                d.problems = problems;
            });
        }
    });
    e.on_clear_hand({
        let weak = weak.clone();
        move || edit(&weak, Keep::default(), |d| d.theme.tokens.clear())
    });
    e.on_pick_font({
        let weak = weak.clone();
        move |index| {
            edit(&weak, Keep::default(), |d| {
                let (options, _) = font_options(d.theme.font.as_deref());
                let picked = usize::try_from(index).ok().and_then(|i| options.get(i)).map(ToString::to_string);
                d.theme.font = match picked {
                    Some(name) if name != OWN_FONT => Some(name.trim_end_matches(" (not installed)").to_string()),
                    _ => None,
                };
            });
        }
    });
    e.on_save({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(draft) = DRAFT.lock().unwrap().take() else { return };
            if !themes::usable(&draft.theme) {
                *DRAFT.lock().unwrap() = Some(draft);
                return;
            }
            let mut saved = draft.theme;
            let name = saved.name.trim();
            saved.name = if name.is_empty() { "Custom theme".into() } else { name.into() };
            // An edited preset is the person's own now: a new id, so choosing another preset
            // doesn't take it away.
            let old_id = saved.id.clone();
            if old_id.starts_with(presets::ID_PREFIX) {
                saved.id = new_id();
            }
            commit(&ctx, &app, |s| {
                match s.custom_themes.iter_mut().find(|t| t.id == old_id) {
                    Some(slot) if draft.existing => *slot = saved.clone(),
                    _ => s.custom_themes.push(saved.clone()),
                }
                s.custom_theme = Some(saved.id.clone());
            });
            app.global::<ThemeEditor>().set_editing(false);
        }
    });
    e.on_copy_css({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let css = DRAFT.lock().unwrap().as_ref().map(|d| themes::to_css(&d.theme)).unwrap_or_default();
            app.global::<ThemeEditor>().set_copy_text(css.into());
            edit(&weak, Keep { hex: None, css: true }, |d| d.copied = true);
            let weak = weak.clone();
            slint::Timer::single_shot(Duration::from_millis(1600), move || edit(&weak, Keep { hex: None, css: true }, |d| d.copied = false));
        }
    });
    e.on_cancel({
        let weak = weak.clone();
        move || {
            *DRAFT.lock().unwrap() = None;
            if let Some(app) = weak.upgrade() {
                app.global::<ThemeEditor>().set_editing(false);
            }
        }
    });
    e.on_delete({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(draft) = DRAFT.lock().unwrap().take() else { return };
            let id = draft.theme.id;
            commit(&ctx, &app, |s| {
                s.custom_themes.retain(|t| t.id != id);
                if s.custom_theme.as_deref() == Some(id.as_str()) {
                    s.custom_theme = None;
                }
            });
            app.global::<ThemeEditor>().set_editing(false);
        }
    });
}

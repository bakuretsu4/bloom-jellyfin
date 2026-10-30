//! The Material 3 colour scheme: generated from a seed colour with HCT (`material-colors`, a port
//! of Google's library) and written into the `Theme` global that every component reads.
//!
//! A Slint global can't animate, so a change between schemes is tweened from here: thirty colours
//! stepped over 600 ms (as the original fades its scheme). What is on screen is the settings'
//! scheme (an accent, or a custom theme) unless a title page is up, which takes its colours from
//! the title's artwork (the seed is its dominant colour), as the original's item page does.

use crate::{themes, App, Theme};
use bloom_core::settings::SettingsStore;
use bloom_core::settings::{Accent, CustomTheme, Settings, Theme as ThemeChoice};
use bloom_core::Ctx;
use material_colors::color::Rgb;
use material_colors::scheme::Scheme;
use material_colors::theme::ThemeBuilder;
use slint::{Color, ComponentHandle, SharedPixelBuffer, Weak};
use std::cell::RefCell;
use std::time::Duration;

/// Bloom's amber, the default accent.
pub const AMBER: u32 = 0xE0862B;

/// The default interface font.
pub const FONT: &str = "Roboto";

/// How long a change of scheme takes.
const FADE: Duration = Duration::from_millis(600);

fn color(c: Rgb) -> Color {
    Color::from_rgb_u8(c.red, c.green, c.blue)
}

/// The seed colour for an accent: the same four as the Svelte app's.
pub fn seed(accent: Accent) -> u32 {
    match accent {
        Accent::Amber => AMBER,
        Accent::Green => 0x2E8B6A,
        Accent::Blue => 0x3E6FB0,
        Accent::Red => 0xC0463D,
    }
}

/// Whether the desktop prefers a dark appearance, from the settings portal
/// (`org.freedesktop.appearance` `color-scheme`: 1 dark, 2 light, 0 no preference). Dark when it
/// can't be asked, as without a session bus.
fn system_dark() -> bool {
    use dbus::arg::Variant;
    let Ok(connection) = dbus::blocking::Connection::new_session() else { return true };
    let proxy = connection.with_proxy("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", std::time::Duration::from_millis(500));
    let args = ("org.freedesktop.appearance", "color-scheme");
    // The answer is a variant, and in newer portals a variant inside one: try that shape first.
    let nested: Result<(Variant<Variant<u32>>,), _> = proxy.method_call("org.freedesktop.portal.Settings", "Read", args);
    let scheme = match nested {
        Ok((v,)) => Some(v.0 .0),
        Err(_) => {
            let single: Result<(Variant<u32>,), _> = proxy.method_call("org.freedesktop.portal.Settings", "Read", args);
            single.ok().map(|(v,)| v.0)
        }
    };
    scheme != Some(2)
}

/// Every colour of the `Theme` global that a scheme sets.
type Palette = [Color; ROLES.len()];

type Get = fn(&Theme<'_>) -> Color;
type Set = fn(&Theme<'_>, Color);

macro_rules! role {
    ($name:literal, $get:ident, $set:ident) => {
        ($name, (|t| t.$get()) as Get, (|t, c| t.$set(c)) as Set)
    };
}

/// The roles, by the names `themes::roles` uses (and the Material names, in kebab case).
const ROLES: [(&str, Get, Set); 30] = [
    role!("primary", get_primary, set_primary),
    role!("on-primary", get_on_primary, set_on_primary),
    role!("primary-container", get_primary_container, set_primary_container),
    role!("on-primary-container", get_on_primary_container, set_on_primary_container),
    role!("secondary", get_secondary, set_secondary),
    role!("on-secondary", get_on_secondary, set_on_secondary),
    role!("secondary-container", get_secondary_container, set_secondary_container),
    role!("on-secondary-container", get_on_secondary_container, set_on_secondary_container),
    role!("tertiary", get_tertiary, set_tertiary),
    role!("on-tertiary", get_on_tertiary, set_on_tertiary),
    role!("tertiary-container", get_tertiary_container, set_tertiary_container),
    role!("on-tertiary-container", get_on_tertiary_container, set_on_tertiary_container),
    role!("error", get_error, set_error),
    role!("on-error", get_on_error, set_on_error),
    role!("error-container", get_error_container, set_error_container),
    role!("on-error-container", get_on_error_container, set_on_error_container),
    role!("surface", get_surface, set_surface),
    role!("on-surface", get_on_surface, set_on_surface),
    role!("on-surface-variant", get_on_surface_variant, set_on_surface_variant),
    role!("surface-container-lowest", get_surface_container_lowest, set_surface_container_lowest),
    role!("surface-container-low", get_surface_container_low, set_surface_container_low),
    role!("surface-container", get_surface_container, set_surface_container),
    role!("surface-container-high", get_surface_container_high, set_surface_container_high),
    role!("surface-container-highest", get_surface_container_highest, set_surface_container_highest),
    role!("outline", get_outline, set_outline),
    role!("outline-variant", get_outline_variant, set_outline_variant),
    role!("inverse-surface", get_inverse_surface, set_inverse_surface),
    role!("inverse-on-surface", get_inverse_on_surface, set_inverse_on_surface),
    role!("scrim", get_scrim, set_scrim),
    role!("good", get_good, set_good),
];

fn index_of(role: &str) -> Option<usize> {
    ROLES.iter().position(|(name, ..)| *name == role)
}

/// The Material scheme for `seed`, dark or light.
fn scheme_palette(seed: u32, dark: bool) -> Palette {
    let theme = ThemeBuilder::with_source(Rgb::from_u32(seed)).build();
    let s: Scheme = if dark { theme.schemes.dark } else { theme.schemes.light };
    let good = if dark { Color::from_rgb_u8(0x7f, 0xcf, 0x9a) } else { Color::from_rgb_u8(0x1f, 0x6b, 0x3d) };
    ROLES.map(|(name, ..)| match name {
        "primary" => color(s.primary),
        "on-primary" => color(s.on_primary),
        "primary-container" => color(s.primary_container),
        "on-primary-container" => color(s.on_primary_container),
        "secondary" => color(s.secondary),
        "on-secondary" => color(s.on_secondary),
        "secondary-container" => color(s.secondary_container),
        "on-secondary-container" => color(s.on_secondary_container),
        "tertiary" => color(s.tertiary),
        "on-tertiary" => color(s.on_tertiary),
        "tertiary-container" => color(s.tertiary_container),
        "on-tertiary-container" => color(s.on_tertiary_container),
        "error" => color(s.error),
        "on-error" => color(s.on_error),
        "error-container" => color(s.error_container),
        "on-error-container" => color(s.on_error_container),
        "surface" => color(s.surface),
        "on-surface" => color(s.on_surface),
        "on-surface-variant" => color(s.on_surface_variant),
        "surface-container-lowest" => color(s.surface_container_lowest),
        "surface-container-low" => color(s.surface_container_low),
        "surface-container" => color(s.surface_container),
        "surface-container-high" => color(s.surface_container_high),
        "surface-container-highest" => color(s.surface_container_highest),
        "outline" => color(s.outline),
        "outline-variant" => color(s.outline_variant),
        "inverse-surface" => color(s.inverse_surface),
        "inverse-on-surface" => color(s.inverse_on_surface),
        "scrim" => color(s.scrim),
        _ => good,
    })
}

/// A custom theme's colours on the roles.
fn custom_palette(custom: &CustomTheme) -> Palette {
    let mut palette = [Color::from_rgb_u8(128, 128, 128); ROLES.len()];
    for (role, hex) in themes::roles(custom) {
        if let Some(i) = index_of(role) {
            let rgb = themes::parse_hex(&hex).unwrap_or([128.0; 3]);
            palette[i] = Color::from_rgb_u8(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8);
        }
    }
    palette
}

/// What the interface should look like, and what is on screen now.
struct Look {
    /// The settings' scheme.
    base: Palette,
    dark: bool,
    /// A custom theme is in use: it brings its own colours, and pages don't tint over it.
    custom: bool,
    /// The seed of the title page on screen, from its artwork.
    page: Option<u32>,
}

thread_local! {
    static LOOK: RefCell<Option<Look>> = const { RefCell::new(None) };
    static FADING: slint::Timer = slint::Timer::default();
}

fn read(app: &App) -> Palette {
    let t = app.global::<Theme>();
    ROLES.map(|(_, get, _)| get(&t))
}

fn write(app: &App, palette: &Palette) {
    let t = app.global::<Theme>();
    for ((_, _, set), c) in ROLES.iter().zip(palette) {
        set(&t, *c);
    }
}

fn mix(a: Color, b: Color, amount: f32) -> Color {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * amount).round().clamp(0.0, 255.0) as u8;
    Color::from_argb_u8(channel(a.alpha(), b.alpha()), channel(a.red(), b.red()), channel(a.green(), b.green()), channel(a.blue(), b.blue()))
}

/// Shows `target`: stepped to over 600 ms, or at once when `animate` is off or motion is reduced.
fn show(app: &App, target: Palette, animate: bool) {
    let instant = !animate || app.global::<Theme>().get_reduce_motion();
    let from = read(app);
    if instant || from == target {
        FADING.with(|f| f.stop());
        write(app, &target);
        return;
    }
    let (weak, started) = (app.as_weak(), std::time::Instant::now());
    FADING.with(|f| {
        f.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = weak.upgrade() else { return };
            let t = (started.elapsed().as_secs_f32() / FADE.as_secs_f32()).min(1.0);
            // Slow at both ends, as a standard Material easing is.
            let eased = t * t * (3.0 - 2.0 * t);
            let step: Palette = std::array::from_fn(|i| mix(from[i], target[i], eased));
            write(&app, &step);
            if t >= 1.0 {
                FADING.with(|f| f.stop());
            }
        });
    });
}

/// Brings the screen to what `LOOK` says it should be.
fn refresh(app: &App, animate: bool) {
    let target = LOOK.with(|l| {
        let l = l.borrow();
        let l = l.as_ref()?;
        Some(match l.page {
            Some(seed) if !l.custom => scheme_palette(seed, l.dark),
            _ => l.base,
        })
    });
    if let Some(target) = target {
        show(app, target, animate);
    }
}

/// Takes the scheme the settings ask for: the custom theme in use when there is one (and it clears
/// its contrast checks), else the accent's, light or dark as chosen (`Auto` asks the desktop).
/// `LIGHT=1` in the environment forces the light scheme, for screenshots. `animate` fades to it.
pub fn apply_settings(app: &App, settings: &Settings, animate: bool) {
    let custom = settings.custom_theme.as_ref().and_then(|id| settings.custom_themes.iter().find(|t| &t.id == id)).filter(|t| themes::usable(t));
    let (base, dark, font) = match custom {
        Some(custom) => (custom_palette(custom), custom.base == bloom_core::settings::ThemeBase::Dark, themes::safe_font(custom.font.as_deref()).unwrap_or_else(|| FONT.to_string())),
        None => {
            let dark = match settings.theme {
                ThemeChoice::Light => false,
                ThemeChoice::Dark => true,
                ThemeChoice::Auto => system_dark(),
            } && std::env::var_os("LIGHT").is_none();
            (scheme_palette(seed(settings.accent), dark), dark, FONT.to_string())
        }
    };
    app.global::<Theme>().set_font(font.into());
    LOOK.with(|l| {
        let mut l = l.borrow_mut();
        let page = l.as_ref().and_then(|l| l.page);
        *l = Some(Look { base, dark, custom: custom.is_some(), page });
    });
    refresh(app, animate);
}

/// The title page on screen takes its colours from `seed` (the dominant colour of its artwork), or
/// lets them go (None) when it is left.
pub fn set_page_seed(app: &App, seed: Option<u32>) {
    let changed = LOOK.with(|l| {
        let mut l = l.borrow_mut();
        match l.as_mut() {
            Some(l) if l.page != seed => {
                l.page = seed;
                true
            }
            _ => false,
        }
    });
    if changed {
        refresh(app, true);
    }
}

/// The colour most suited to build a scheme from, in a picture: what the original calls its
/// dominant colour. Sampled at about ten thousand pixels, as the quantizer wants a small image.
pub fn seed_of(pixels: &SharedPixelBuffer<slint::Rgba8Pixel>) -> Option<u32> {
    let all = pixels.as_slice();
    let step = (all.len() / 10_000).max(1);
    let sampled: Vec<Rgb> = all.iter().step_by(step).filter(|p| p.a > 200).map(|p| Rgb::new(p.r, p.g, p.b)).collect();
    if sampled.is_empty() {
        return None;
    }
    let found = material_colors::image::extract_color(&sampled);
    Some((u32::from(found.red) << 16) | (u32::from(found.green) << 8) | u32::from(found.blue))
}

/// Follows the desktop's light/dark preference while it changes: when the portal says the
/// `color-scheme` moved and the theme is Auto (and no custom theme is in use), the scheme is
/// written again. Quietly nothing without a session bus or a portal.
pub fn follow_desktop(ctx: Ctx, weak: Weak<App>) {
    std::thread::spawn(move || {
        use dbus::message::MatchRule;
        let Ok(connection) = dbus::blocking::Connection::new_session() else { return };
        let rule = MatchRule::new_signal("org.freedesktop.portal.Settings", "SettingChanged");
        let changed = connection.add_match(rule, move |(): (), _, message| {
            let (namespace, key): (Option<String>, Option<String>) = message.get2();
            if namespace.as_deref() == Some("org.freedesktop.appearance") && key.as_deref() == Some("color-scheme") {
                let saved = ctx.state::<SettingsStore>().get();
                if saved.theme == ThemeChoice::Auto && saved.custom_theme.is_none() {
                    let _ = weak.upgrade_in_event_loop(move |app| apply_settings(&app, &saved, true));
                }
            }
            true
        });
        if changed.is_err() {
            return;
        }
        while connection.process(Duration::from_secs(3600)).is_ok() {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pictures_seed_is_its_dominant_colour() {
        let mut pixels = SharedPixelBuffer::<slint::Rgba8Pixel>::new(40, 40);
        for p in pixels.make_mut_slice() {
            *p = slint::Rgba8Pixel { r: 0x3e, g: 0x6f, b: 0xb0, a: 255 };
        }
        let seed = seed_of(&pixels).expect("a seed");
        let (r, g, b) = ((seed >> 16) & 255, (seed >> 8) & 255, seed & 255);
        // Blue stays the strongest channel, and nothing is left of the picture's red.
        assert!(b > g && g > r, "{seed:06x}");
        // A picture with nothing opaque in it has no colour.
        for p in pixels.make_mut_slice() {
            p.a = 0;
        }
        assert_eq!(seed_of(&pixels), None);
    }

    #[test]
    fn a_custom_themes_roles_are_all_roles_of_the_interface() {
        let theme = CustomTheme { ground: "#141615".into(), surface: "#232624".into(), ink: "#e9eceb".into(), accent: "#d98237".into(), ..CustomTheme::default() };
        let named: Vec<&str> = themes::roles(&theme).iter().map(|(role, _)| *role).collect();
        assert_eq!(named.len(), ROLES.len());
        for (name, ..) in ROLES {
            assert!(named.contains(&name), "{name} isn't set by a custom theme");
        }
    }

    #[test]
    fn each_accent_has_its_own_seed() {
        let seeds = [Accent::Amber, Accent::Green, Accent::Blue, Accent::Red].map(seed);
        for (i, a) in seeds.iter().enumerate() {
            assert!(!seeds[i + 1..].contains(a), "two accents share a seed");
        }
        assert_eq!(seed(Accent::Amber), AMBER);
    }

    /// Against the desktop this runs on: `cargo test --release -- --ignored the_desktops`.
    #[test]
    #[ignore]
    fn the_desktops_preference_is_read() {
        println!("system_dark = {}", system_dark());
    }
}

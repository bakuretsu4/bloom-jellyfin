//! Custom themes (Settings → Appearance): four colours someone picked, the shades worked out from
//! them, any shades set by hand as CSS, and optionally a font; checked for contrast before a theme
//! may be used. The same model, rules and CSS as the Svelte app's `src/lib/theme.ts`, so a theme
//! made or pasted in one works in the other.
//!
//! A theme sets Bloom's thirteen colour tokens. They are written onto the Material roles the
//! interface reads (see `apply`): the background is the page, panels are the tonal cards, the text
//! tokens are the on-surface roles, the accent is the primary.

use bloom_core::settings::{CustomTheme, ThemeBase};
use std::collections::BTreeMap;

type Rgb = [f64; 3];

/// The colour tokens a theme may set.
pub const COLOUR_TOKENS: [&str; 13] =
    ["--ground", "--surface", "--surface-2", "--raise", "--line", "--line-soft", "--ink", "--ink-2", "--ink-3", "--accent", "--accent-ink", "--good", "--alert"];

/// The four colours with a field of their own.
const BASE_TOKENS: [&str; 4] = ["--ground", "--surface", "--ink", "--accent"];

pub fn parse_hex(hex: &str) -> Option<Rgb> {
    let digits = hex.trim().strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(digits, 16).ok()?;
    Some([f64::from((n >> 16) & 255), f64::from((n >> 8) & 255), f64::from(n & 255)])
}

/// `#abc` or `#aabbcc` as `#aabbcc`, or None for anything else.
pub fn normalise_hex(value: &str) -> Option<String> {
    let v = value.trim().to_ascii_lowercase();
    let digits = v.strip_prefix('#')?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match digits.len() {
        6 => Some(v),
        3 => Some(format!("#{}", digits.chars().flat_map(|c| [c, c]).collect::<String>())),
        _ => None,
    }
}

/// A font family name that can't close the CSS string or declaration it's written into.
pub fn safe_font(name: Option<&str>) -> Option<String> {
    let n = name.unwrap_or("").trim();
    let bad = |c: char| matches!(c, '"' | '\'' | ';' | '{' | '}' | '\\' | '<' | '>') || c.is_control();
    (!n.is_empty() && n.chars().count() <= 80 && !n.chars().any(bad)).then(|| n.to_string())
}

fn to_hex(rgb: Rgb) -> String {
    let c = |v: f64| v.clamp(0.0, 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(rgb[0]), c(rgb[1]), c(rgb[2]))
}

/// `amount` of the way from `a` to `b`.
fn mix(a: Rgb, b: Rgb, amount: f64) -> Rgb {
    [a[0] + (b[0] - a[0]) * amount, a[1] + (b[1] - a[1]) * amount, a[2] + (b[2] - a[2]) * amount]
}

/// WCAG relative luminance.
fn luminance(rgb: Rgb) -> f64 {
    let channel = |c: f64| {
        let s = c / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

fn ratio(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The WCAG contrast ratio of two `#rrggbb` colours, 1 to 21.
pub fn contrast(a: &str, b: &str) -> f64 {
    match (parse_hex(a), parse_hex(b)) {
        (Some(x), Some(y)) => ratio(x, y),
        _ => 1.0,
    }
}

fn is_dark(theme: &CustomTheme) -> bool {
    theme.base == ThemeBase::Dark
}

/// The colour tokens worked out from the four colours alone. Secondary text is faded toward the
/// background only as far as it still reads at 4.5:1 on the background and on panels; the text on
/// the accent is whichever of near-black or white stands out more.
fn derived(theme: &CustomTheme) -> BTreeMap<&'static str, String> {
    let dark = is_dark(theme);
    let ground = parse_hex(&theme.ground).unwrap_or([20.0, 22.0, 21.0]);
    let surface = parse_hex(&theme.surface).unwrap_or(ground);
    let ink = parse_hex(&theme.ink).unwrap_or([233.0, 236.0, 235.0]);
    let accent = parse_hex(&theme.accent).unwrap_or([217.0, 130.0, 55.0]);
    let white = [255.0; 3];
    let near_black = [17.0; 3];
    let faded = |share: f64| {
        let reads = |c: Rgb| ratio(c, ground).min(ratio(c, surface)) >= 4.5;
        let mut s = share;
        while s > 0.0 && !reads(mix(ink, ground, s)) {
            s = (s - 0.03).max(0.0);
        }
        to_hex(mix(ink, ground, s))
    };
    BTreeMap::from([
        ("--ground", to_hex(ground)),
        ("--surface", to_hex(surface)),
        ("--surface-2", to_hex(mix(surface, ink, 0.07))),
        ("--raise", to_hex(if dark { mix(surface, ink, 0.1) } else { mix(surface, white, 0.7) })),
        ("--line", to_hex(mix(surface, ink, 0.2))),
        ("--line-soft", to_hex(mix(surface, ink, 0.1))),
        ("--ink", to_hex(ink)),
        ("--ink-2", faded(0.3)),
        ("--ink-3", faded(0.42)),
        ("--accent", to_hex(accent)),
        ("--accent-ink", to_hex(if ratio(accent, white) >= ratio(accent, near_black) { white } else { near_black })),
        ("--good", (if dark { "#5e9b72" } else { "#2f6b46" }).to_string()),
        ("--alert", (if dark { "#c0736b" } else { "#9c4a43" }).to_string()),
    ])
}

/// The theme's final colour tokens: worked out, then whatever was set by hand on top.
pub fn colour_tokens(theme: &CustomTheme) -> BTreeMap<&'static str, String> {
    let mut tokens = derived(theme);
    for (name, value) in &theme.tokens {
        if let (Some(hex), Some(token)) = (normalise_hex(value), COLOUR_TOKENS.iter().find(|t| **t == name)) {
            tokens.insert(token, hex);
        }
    }
    tokens
}

pub struct Check {
    pub label: &'static str,
    pub ratio: f64,
    pub needed: f64,
}

impl Check {
    pub fn ok(&self) -> bool {
        self.ratio >= self.needed
    }
}

/// What the final tokens must clear to be used: text of every weight at 4.5:1 (WCAG AA), the
/// accent at 3:1 on panels where it's a control or a mark, and the text on the accent at 4.5:1.
pub fn check_theme(theme: &CustomTheme) -> Vec<Check> {
    let t = colour_tokens(theme);
    let c = |a: &str, b: &str| contrast(&t[a], &t[b]);
    vec![
        Check { label: "Text on the background", ratio: c("--ink", "--ground"), needed: 4.5 },
        Check { label: "Text on panels", ratio: c("--ink", "--surface"), needed: 4.5 },
        Check { label: "Secondary text", ratio: c("--ink-2", "--ground").min(c("--ink-2", "--surface")), needed: 4.5 },
        Check { label: "Faint text", ratio: c("--ink-3", "--ground").min(c("--ink-3", "--surface")), needed: 4.5 },
        Check { label: "Accent on panels", ratio: c("--accent", "--surface"), needed: 3.0 },
        Check { label: "Text on the accent", ratio: c("--accent-ink", "--accent"), needed: 4.5 },
    ]
}

pub fn usable(theme: &CustomTheme) -> bool {
    check_theme(theme).iter().all(Check::ok)
}

/// The theme as the CSS it amounts to, to read, edit or share.
pub fn to_css(theme: &CustomTheme) -> String {
    let tokens = colour_tokens(theme);
    let mut lines = vec![format!("  color-scheme: {};", if is_dark(theme) { "dark" } else { "light" })];
    if let Some(font) = safe_font(theme.font.as_deref()) {
        lines.push(format!("  --f-ui: \"{font}\";"));
    }
    for token in COLOUR_TOKENS {
        lines.push(format!("  {token}: {};", tokens[token]));
    }
    format!(":root {{\n{}\n}}\n", lines.join("\n"))
}

/// `(name, value)` for each `name: value` declaration in `css` (comments dropped). Names take
/// digits too: `--ink-2`.
fn declarations(css: &str) -> Vec<(String, String)> {
    let mut body = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        body.push_str(&rest[..start]);
        rest = match rest[start + 2..].find("*/") {
            Some(end) => &rest[start + 2 + end + 2..],
            None => "",
        };
    }
    body.push_str(rest);
    // Braces separate rules, semicolons declarations; the last declaration may lack its semicolon.
    body.split(['{', '}', ';'])
        .filter_map(|piece| {
            let (name, value) = piece.split_once(':')?;
            let name = name.trim().to_ascii_lowercase();
            let valid = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
            valid.then(|| (name, value.trim().to_string()))
        })
        .collect()
}

/// A theme from CSS someone edited or pasted: `color-scheme` sets light or dark, `--f-ui` the font,
/// and Bloom's colour tokens the colours, as hex. Anything else is reported, not used. Tokens that
/// match what the four colours work out to aren't kept as set by hand, so they keep following the
/// colour fields.
pub fn from_css(css: &str, from: &CustomTheme) -> (CustomTheme, Vec<String>) {
    let mut problems = Vec::new();
    let mut theme = CustomTheme { tokens: BTreeMap::new(), font: None, ..from.clone() };
    let mut declared: BTreeMap<&'static str, String> = BTreeMap::new();
    for (name, value) in declarations(css) {
        if name == "color-scheme" {
            match value.as_str() {
                "light" => theme.base = ThemeBase::Light,
                "dark" => theme.base = ThemeBase::Dark,
                other => problems.push(format!("color-scheme is \"light\" or \"dark\", not \"{other}\"")),
            }
        } else if name == "--f-ui" {
            let family = value.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            match safe_font(Some(&family)) {
                Some(font) if font != "Archivo" => theme.font = Some(font),
                Some(_) => {}
                None => problems.push(format!("--f-ui: \"{family}\" isn't a font name Bloom can use")),
            }
        } else if let Some(token) = COLOUR_TOKENS.iter().find(|t| **t == name) {
            match normalise_hex(&value) {
                Some(hex) => {
                    declared.insert(token, hex);
                }
                None => problems.push(format!("{name}: \"{value}\" isn't a hex colour like #1a1b26")),
            }
        } else if name.starts_with("--") {
            problems.push(format!("{name} isn't one of Bloom's colours"));
        } else {
            problems.push(format!("{name} isn't something a theme can set"));
        }
    }
    for token in BASE_TOKENS {
        if let Some(hex) = declared.get(token) {
            match token {
                "--ground" => theme.ground = hex.clone(),
                "--surface" => theme.surface = hex.clone(),
                "--ink" => theme.ink = hex.clone(),
                _ => theme.accent = hex.clone(),
            }
        }
    }
    let worked = derived(&theme);
    for token in COLOUR_TOKENS {
        if let Some(hex) = declared.get(token) {
            if !BASE_TOKENS.contains(&token) && *hex != worked[token] {
                theme.tokens.insert(token.to_string(), hex.clone());
            }
        }
    }
    (theme, problems)
}

/// Bloom's own colours, as the starting point for a new theme: background, panels, text, accent.
pub fn start(base: ThemeBase) -> [&'static str; 4] {
    match base {
        ThemeBase::Dark => ["#141615", "#232624", "#e9eceb", "#d98237"],
        ThemeBase::Light => ["#dee2e0", "#f4f6f5", "#161918", "#9c4e14"],
    }
}

/// The scheme a theme amounts to, for the interface's Material roles, as `(role, #rrggbb)`.
pub fn roles(theme: &CustomTheme) -> Vec<(&'static str, String)> {
    let t = colour_tokens(theme);
    let rgb = |token: &str| parse_hex(&t[token]).unwrap_or([128.0; 3]);
    let (ground, surface, ink, accent) = (rgb("--ground"), rgb("--surface"), rgb("--ink"), rgb("--accent"));
    let (good, alert) = (rgb("--good"), rgb("--alert"));
    vec![
        ("surface", to_hex(ground)),
        ("on-surface", to_hex(ink)),
        ("on-surface-variant", t["--ink-2"].clone()),
        ("outline", t["--ink-3"].clone()),
        ("outline-variant", t["--line"].clone()),
        ("surface-container-lowest", to_hex(mix(ground, surface, 0.4))),
        ("surface-container-low", to_hex(surface)),
        ("surface-container", t["--surface-2"].clone()),
        ("surface-container-high", t["--raise"].clone()),
        ("surface-container-highest", to_hex(mix(rgb("--raise"), ink, 0.08))),
        ("primary", to_hex(accent)),
        ("on-primary", t["--accent-ink"].clone()),
        ("primary-container", to_hex(mix(surface, accent, 0.3))),
        ("on-primary-container", to_hex(ink)),
        ("secondary", t["--ink-2"].clone()),
        ("on-secondary", to_hex(ground)),
        ("secondary-container", to_hex(mix(surface, ink, 0.18))),
        ("on-secondary-container", to_hex(ink)),
        ("tertiary", to_hex(good)),
        ("on-tertiary", to_hex(ground)),
        ("tertiary-container", to_hex(mix(surface, good, 0.3))),
        ("on-tertiary-container", to_hex(ink)),
        ("error", to_hex(alert)),
        ("on-error", to_hex(ground)),
        ("error-container", to_hex(mix(surface, alert, 0.3))),
        ("on-error-container", to_hex(ink)),
        ("inverse-surface", to_hex(ink)),
        ("inverse-on-surface", to_hex(ground)),
        ("scrim", "#000000".to_string()),
        ("good", to_hex(good)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(base: ThemeBase) -> CustomTheme {
        let [ground, surface, ink, accent] = start(base);
        CustomTheme { id: "t".into(), name: "T".into(), base, ground: ground.into(), surface: surface.into(), ink: ink.into(), accent: accent.into(), ..CustomTheme::default() }
    }

    #[test]
    fn bloom_s_own_colours_are_usable_in_both_directions() {
        assert!(usable(&theme(ThemeBase::Dark)));
        assert!(usable(&theme(ThemeBase::Light)));
    }

    #[test]
    fn hex_is_taken_in_short_and_long_form_only() {
        assert_eq!(normalise_hex(" #ABC "), Some("#aabbcc".into()));
        assert_eq!(normalise_hex("#1a1b26"), Some("#1a1b26".into()));
        assert_eq!(normalise_hex("1a1b26"), None);
        assert_eq!(normalise_hex("#12345"), None);
        assert_eq!(normalise_hex("#gggggg"), None);
    }

    #[test]
    fn text_that_cannot_be_read_fails_its_check() {
        let mut t = theme(ThemeBase::Dark);
        t.ink = "#232624".into();
        let checks = check_theme(&t);
        assert!(!checks[0].ok() && !checks[1].ok());
        assert!(!usable(&t));
    }

    #[test]
    fn faded_text_stops_fading_where_it_would_stop_reading() {
        let t = theme(ThemeBase::Dark);
        let tokens = colour_tokens(&t);
        assert!(contrast(&tokens["--ink-3"], &tokens["--ground"]) >= 4.5);
        assert!(contrast(&tokens["--ink-3"], &tokens["--surface"]) >= 4.5);
    }

    #[test]
    fn the_css_round_trips_and_hand_set_shades_are_kept_apart() {
        let mut t = theme(ThemeBase::Dark);
        t.font = Some("Inter".into());
        t.tokens.insert("--line".into(), "#445566".into());
        let (back, problems) = from_css(&to_css(&t), &theme(ThemeBase::Light));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(back.base, ThemeBase::Dark);
        assert_eq!(back.font.as_deref(), Some("Inter"));
        assert_eq!((&back.ground, &back.accent), (&t.ground, &t.accent));
        // Only the shade that differs from what the four colours give is "by hand".
        assert_eq!(back.tokens.len(), 1);
        assert_eq!(back.tokens.get("--line").map(String::as_str), Some("#445566"));
    }

    #[test]
    fn css_that_a_theme_cannot_use_is_reported_not_applied() {
        let (t, problems) = from_css(":root { color-scheme: sepia; --ink: red; --nope: #fff; margin: 0; --f-ui: \"Bad\\Name\"; --accent: #123 }", &theme(ThemeBase::Dark));
        assert_eq!(problems.len(), 5, "{problems:?}");
        assert_eq!(t.accent, "#112233");
        assert_eq!(t.base, ThemeBase::Dark);
        assert_eq!(t.ink, theme(ThemeBase::Dark).ink);
    }

    /// The themes in docs/themes are for pasting into the editor; each must come in whole.
    #[test]
    fn every_shipped_theme_pastes_in_cleanly() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/themes");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect("docs/themes").flatten() {
            let css = std::fs::read_to_string(entry.path()).unwrap();
            let (t, problems) = from_css(&css, &theme(ThemeBase::Dark));
            assert!(problems.is_empty(), "{}: {problems:?}", entry.path().display());
            assert!(usable(&t), "{} fails a contrast check", entry.path().display());
            seen += 1;
        }
        assert_eq!(seen, 30);
    }

    #[test]
    fn every_role_gets_a_colour() {
        for (role, hex) in roles(&theme(ThemeBase::Light)) {
            assert!(parse_hex(&hex).is_some(), "{role} = {hex}");
        }
    }
}

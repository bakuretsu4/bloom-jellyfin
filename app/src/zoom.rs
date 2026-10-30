//! Interface zoom, stepped like a browser's: Ctrl +, Ctrl -, Ctrl 0, with a readout that flashes
//! the level. It's the window's scale factor, so layout (not pixels) is what scales, and mpv's
//! picture underneath keeps its full resolution. The level is remembered in the settings.

use crate::App;
use bloom_core::settings::SettingsStore;
use bloom_core::Ctx;
use slint::{ComponentHandle, Weak};
use std::sync::OnceLock;
use std::time::Duration;

const STEPS: [f64; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

/// The window's scale factor before any zoom (the desktop's own, such as 1.0 or 2.0).
static BASE: OnceLock<f32> = OnceLock::new();

fn nearest(zoom: f64) -> usize {
    STEPS.iter().enumerate().min_by(|a, b| (a.1 - zoom).abs().total_cmp(&(b.1 - zoom).abs())).map_or(5, |(i, _)| i)
}

/// The level after `direction` (1 in a step, -1 out a step, 0 back to 100%) from `current`.
pub fn next_level(current: f64, direction: i32) -> f64 {
    let i = nearest(current);
    match direction.signum() {
        1 => STEPS[(i + 1).min(STEPS.len() - 1)],
        -1 => STEPS[i.saturating_sub(1)],
        _ => 1.0,
    }
}

fn apply(app: &App, level: f64) {
    use slint::platform::WindowEvent;
    let base = *BASE.get_or_init(|| app.window().scale_factor());
    app.window().dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: base * level as f32 });
}

/// The saved level, in place before the first frame. UI thread.
pub fn restore(app: &App, saved: f64) {
    let level = STEPS[nearest(saved)];
    BASE.get_or_init(|| app.window().scale_factor());
    if (level - 1.0).abs() > f64::EPSILON {
        apply(app, level);
    }
}

/// Ctrl +, Ctrl - or Ctrl 0.
pub fn step(ctx: &Ctx, weak: &Weak<App>, direction: i32) {
    let store = ctx.state::<SettingsStore>();
    let next = next_level(store.get().zoom, direction);
    store.update(|s| s.zoom = next);
    if let Some(app) = weak.upgrade() {
        apply(&app, next);
        crate::settings::zoom_changed(&app, next);
        app.set_zoom_readout(format!("{}%", (next * 100.0).round()).into());
        let weak = weak.clone();
        // The readout stays for a moment; a newer change keeps it up for its own moment.
        slint::Timer::single_shot(Duration::from_millis(1200), move || {
            if let Some(app) = weak.upgrade() {
                if app.get_zoom_readout().as_str() == format!("{}%", (next * 100.0).round()) {
                    app.set_zoom_readout("".into());
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_moves_one_step_and_stops_at_the_ends() {
        assert_eq!(next_level(1.0, 1), 1.1);
        assert_eq!(next_level(1.0, -1), 0.9);
        assert_eq!(next_level(2.0, 1), 2.0);
        assert_eq!(next_level(0.5, -1), 0.5);
        assert_eq!(next_level(1.5, 0), 1.0);
    }

    #[test]
    fn a_saved_level_off_the_steps_snaps_to_the_nearest() {
        assert_eq!(next_level(1.2, 0), 1.0);
        assert_eq!(next_level(1.2, 1), 1.5);
    }
}

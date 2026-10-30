//! The parts of the mouse Slint's own events don't carry: the back button (the thumb button on most
//! mice) goes back a page, or out of the player; Ctrl with the wheel zooms the interface as Ctrl with
//! the plus and minus keys do. Both are read from the window system's events before Slint sees them, so nothing a
//! page draws can swallow them; every other event goes on as usual.

use crate::{zoom, App};
use bloom_core::Ctx;
use slint::winit_030::winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Weak};
use std::cell::Cell;

/// How far a wheel has turned, in "lines": a notch is one, and touchpads send pixels (about 40 a line).
fn lines(delta: &MouseScrollDelta) -> f64 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(*y),
        MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
    }
}

/// A zoom step for some wheel travel, kept so a touchpad's stream of tiny moves adds up to a step
/// instead of one step each. Returns the steps to take (+1 in, -1 out), usually 0 or 1.
fn steps(carry: &mut f64, travel: f64) -> i32 {
    *carry += travel;
    let whole = carry.trunc();
    *carry -= whole;
    whole as i32
}

pub fn install(app: &App, ctx: &Ctx, weak: &Weak<App>) {
    let (ctx, weak) = (ctx.clone(), weak.clone());
    let ctrl = Cell::new(false);
    let carry = Cell::new(0.0_f64);
    app.window().on_winit_window_event(move |_, event| match event {
        WindowEvent::ModifiersChanged(modifiers) => {
            ctrl.set(modifiers.state().control_key());
            EventResult::Propagate
        }
        WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Back, .. } => {
            if let Some(app) = weak.upgrade() {
                // In the player the button leaves it, as Esc does; elsewhere it is Back.
                if app.get_screen() == crate::Screen::Watch {
                    app.invoke_back();
                } else {
                    app.invoke_go_back();
                }
            }
            EventResult::PreventDefault
        }
        WindowEvent::MouseWheel { delta, .. } if ctrl.get() => {
            let mut c = carry.get();
            let n = steps(&mut c, lines(delta));
            carry.set(c);
            if n != 0 {
                zoom::step(&ctx, &weak, n.signum());
            }
            EventResult::PreventDefault
        }
        _ => EventResult::Propagate,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notch_of_the_wheel_is_one_step_either_way() {
        let mut carry = 0.0;
        assert_eq!(steps(&mut carry, 1.0), 1);
        assert_eq!(steps(&mut carry, -1.0), -1);
    }

    #[test]
    fn a_touchpads_small_moves_add_up_to_a_step() {
        let mut carry = 0.0;
        let total: i32 = (0..10).map(|_| steps(&mut carry, 0.25)).sum();
        // Ten quarter-lines are two whole steps and a half left over.
        assert_eq!(total, 2);
        assert!((carry - 0.5).abs() < 1e-9);
    }

    #[test]
    fn pixels_are_forty_to_a_line() {
        let delta = MouseScrollDelta::PixelDelta(slint::winit_030::winit::dpi::PhysicalPosition::new(0.0, 80.0));
        assert!((lines(&delta) - 2.0).abs() < 1e-9);
    }
}

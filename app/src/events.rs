//! Where the core's events go. Called from whichever thread emitted them.

use crate::{downloads, live, nav, watch, App, Screen};
use slint::ComponentHandle;
use bloom_core::Ctx;
use serde_json::Value;
use slint::Weak;

pub fn on_event(ctx: Option<Ctx>, weak: &Weak<App>, event: &str, payload: Value) {
    match event {
        e if e.starts_with("player:") => watch::on_event(ctx, weak, event, payload),
        "downloads:changed" => downloads::on_changed(ctx, weak, payload),
        "sync:userdata" => live::on_userdata(ctx, weak, payload),
        "sync:library" => live::on_library(ctx, weak),
        "sync:connected" => live::on_connected(ctx, weak),
        "notify:open" => open_from_notification(ctx, weak, payload),
        _ => {}
    }
}

/// A new-media notification was clicked: the window comes back and shows the title. Not while
/// something is playing or nobody is signed in, where it would only get in the way.
fn open_from_notification(ctx: Option<Ctx>, weak: &Weak<App>, payload: Value) {
    let (Some(ctx), Some(id)) = (ctx, payload.get("id").and_then(Value::as_str).map(str::to_string)) else { return };
    let w = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |app| {
        app.window().set_minimized(false);
        if !matches!(app.get_screen(), Screen::Watch | Screen::SignIn | Screen::Loading) {
            nav::go(ctx, w, nav::Route::Title(id));
        }
    });
}

//! The path from launch to Home: resume a saved sign-in, or ask for a server and an account.
//!
//! Each step runs on the core's runtime and reports back through `slint::Weak`, so the UI thread
//! never waits on the network.

use crate::cards;
use crate::nav::{self, Route};
use crate::{watch, AccountItem, App, NearbyData, SavedAccountData, Screen};
use bloom_core::discovery;
use bloom_core::jellyfin::{self, Account, Jellyfin};
use bloom_core::Ctx;
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Runs `f` on the UI thread.
fn ui(weak: &Weak<App>, f: impl FnOnce(&App) + Send + 'static) {
    let _ = weak.upgrade_in_event_loop(move |app| f(&app));
}

fn show_error(weak: &Weak<App>, message: String) {
    ui(weak, move |app| {
        app.set_busy(false);
        app.set_error(message.into());
    });
}

/// The first letter of a name, upper case, for an avatar.
fn initial(name: &str) -> slint::SharedString {
    name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into()
}

/// "192.168.1.20:8096" from "http://192.168.1.20:8096/".
fn host(address: &str) -> &str {
    address.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/')
}

/// First thing after launch.
pub fn start(ctx: Ctx, weak: Weak<App>) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::startup_with(&jf).await {
            Ok(startup) => match startup.account {
                Some(account) => {
                    crate::session::set_offline(&weak, startup.offline);
                    enter_home(&ctx, &weak, account)
                }
                None => ask_for_server(&ctx, &weak, startup.last_address.unwrap_or_default()),
            },
            Err(e) => {
                ask_for_server(&ctx, &weak, String::new());
                show_error(&weak, e.to_string());
            }
        }
        dev_sign_in(&ctx, &weak).await;
    });
}

/// Development only: `BLOOM_SERVER`, `BLOOM_USER` and `BLOOM_PASS` (the test account's, from the
/// environment, never a real one) sign in without typing.
async fn dev_sign_in(ctx: &Ctx, weak: &Weak<App>) {
    let (Ok(address), Ok(user), Ok(pass)) = (std::env::var("BLOOM_SERVER"), std::env::var("BLOOM_USER"), std::env::var("BLOOM_PASS")) else {
        return;
    };
    let jf = ctx.state::<Jellyfin>();
    if let Ok(startup) = jellyfin::startup_with(&jf).await {
        if startup.account.is_some() {
            return;
        }
    }
    if jellyfin::check_server_with(&jf, address).await.is_ok() {
        match jellyfin::sign_in_with(&jf, user, pass, true).await {
            Ok(account) => enter_home(ctx, weak, account),
            Err(e) => show_error(weak, e.to_string()),
        }
    }
}

/// The server step: saved accounts, servers on the network, and the address field.
fn ask_for_server(ctx: &Ctx, weak: &Weak<App>, address: String) {
    qc_stop(ctx);
    ui(weak, move |app| {
        app.set_offline(false);
        app.set_server_step(true);
        app.set_busy(false);
        app.set_error("".into());
        // Signing out keeps the address of the server that was in use.
        if !address.is_empty() {
            app.set_address(address.into());
        }
        app.set_qc_available(false);
        crate::theme::set_page_seed(app, None);
        app.set_screen(Screen::SignIn);
    });
    load_saved(ctx, weak);
    load_wall(ctx, weak);
    search_again(ctx.clone(), weak.clone());
}

/// Posters already in the artwork cache (which needs no sign-in to read), for the wall beside the
/// form. A small cache repeats to fill sixteen places; none cached, no wall.
fn load_wall(ctx: &Ctx, weak: &Weak<App>) {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Poster {
        server_id: String,
        image: PosterImage,
        width: u32,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PosterImage {
        item_id: String,
        kind: String,
        tag: String,
    }
    let (ctx, weak) = (ctx.clone(), weak.clone());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        crate::wall::flush();
        let found = bloom_core::images::cached_title_posters(&jf, crate::wall::titles()).await.unwrap_or_default();
        let posters: Vec<Poster> = serde_json::to_value(&found).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        let mut pictures = Vec::new();
        for p in posters {
            let path = format!("/{}/{}/{}/{}/{}", p.server_id, p.image.item_id, p.image.kind, p.image.tag, p.width);
            if let Some(pixels) = crate::cards::pixels(&ctx, &path).await {
                pictures.push(pixels);
            }
        }
        if pictures.is_empty() {
            return;
        }
        let wall: Vec<_> = (0..16).map(|i| pictures[i % pictures.len()].clone()).collect();
        ui(&weak, move |app| {
            let images: Vec<slint::Image> = wall.into_iter().map(slint::Image::from_rgba8).collect();
            app.set_wall(ModelRc::new(VecModel::from(images)));
        });
    });
}

/// The saved sign-ins, for "Welcome back".
fn load_saved(ctx: &Ctx, weak: &Weak<App>) {
    let jf = ctx.state::<Jellyfin>();
    // (server id, user id, name, "Home Media, 192.168.1.20:8096"): pictures can't be made off the UI thread.
    let plain: Vec<(String, String, String, String)> = jellyfin::saved_accounts_with(&jf)
        .into_iter()
        .map(|a| (a.server.id.clone(), a.user.id.clone(), a.user.name.clone(), format!("{}, {}", a.server.name, host(&a.server.address))))
        .collect();
    let wanted: Vec<(String, String)> = plain.iter().map(|(s, u, _, _)| (s.clone(), u.clone())).collect();
    ui(weak, move |app| {
        let rows: Vec<SavedAccountData> = plain
            .into_iter()
            .map(|(server, user, name, detail)| SavedAccountData {
                server_id: server.into(),
                user_id: user.into(),
                initial: initial(&name),
                name: name.into(),
                detail: detail.into(),
                image: slint::Image::default(),
                has_image: false,
            })
            .collect();
        app.set_saved_accounts(ModelRc::new(VecModel::from(rows)));
    });
    load_pictures(ctx, weak, wanted, Place::Chooser);
}

/// Looks for Jellyfin servers on this network.
pub fn search_again(_ctx: Ctx, weak: Weak<App>) {
    ui(&weak, |app| {
        app.set_looking(true);
        app.set_nearby_note("Looking for Jellyfin servers on this network".into());
    });
    bloom_core::rt::spawn(async move {
        let found = discovery::discover_servers().await;
        let rows: Vec<NearbyData> = found.into_iter().map(|s| NearbyData { initial: initial(&s.name), name: s.name.into(), address: s.address.into() }).collect();
        ui(&weak, move |app| {
            app.set_looking(false);
            let note = if rows.is_empty() {
                "None answered. A server in Docker without port 7359 published, or on another network, won't show here; type its address below."
            } else {
                ""
            };
            app.set_nearby_note(note.into());
            app.set_nearby(ModelRc::new(VecModel::from(rows)));
        });
    });
}

/// A server that answered goes through the same check as a typed address.
pub fn connect_to(ctx: Ctx, weak: Weak<App>, address: String) {
    let shown = host(&address).to_string();
    ui(&weak, move |app| app.set_address(shown.into()));
    check_server(ctx, weak, address);
}

pub fn check_server(ctx: Ctx, weak: Weak<App>, address: String) {
    ui(&weak, |app| {
        app.set_busy(true);
        app.set_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::check_server_with(&jf, address).await {
            Ok(check) => {
                let detail = format!("{}, Jellyfin {}", host(&check.server.address), check.server.version);
                let name = check.server.name;
                let users: Vec<slint::SharedString> = check.users.into_iter().map(|u| u.name.into()).collect();
                let quick_connect = check.quick_connect;
                ui(&weak, move |app| {
                    app.set_server_name(name.into());
                    app.set_server_detail(detail.into());
                    app.set_public_users(ModelRc::new(VecModel::from(users)));
                    app.set_qc_available(quick_connect);
                    app.set_server_step(false);
                    app.set_busy(false);
                });
                if quick_connect {
                    qc_start(ctx, weak);
                }
            }
            Err(e) => show_error(&weak, e.to_string()),
        }
    });
}

/// "Use a different server": back to the address step.
pub fn change_server(ctx: Ctx, weak: Weak<App>) {
    qc_stop(&ctx);
    jellyfin::forget_server(&ctx.state::<Jellyfin>());
    ui(&weak, |app| {
        app.set_server_step(true);
        app.set_qc_available(false);
        app.set_error("".into());
        app.set_password("".into());
    });
}

pub fn sign_in(ctx: Ctx, weak: Weak<App>, username: String, password: String) {
    if username.trim().is_empty() {
        return show_error(&weak, "Enter your username.".into());
    }
    ui(&weak, |app| {
        app.set_busy(true);
        app.set_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let remember = remember(&weak).await;
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::sign_in_with(&jf, username, password, remember).await {
            Ok(account) => {
                launch(&ctx, &weak);
                enter_home(&ctx, &weak, account)
            }
            Err(e) => {
                ui(&weak, |app| app.set_password("".into()));
                show_error(&weak, e.to_string());
            }
        }
    });
}

/// Whether "Stay signed in" is on, asked of the UI thread.
async fn remember(weak: &Weak<App>) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel();
    ui(weak, move |app| {
        let _ = tx.send(app.get_remember());
    });
    rx.await.unwrap_or(true)
}

// ---- Quick Connect: a code is asked for as soon as the server allows it, and checked every few
// seconds until it's approved, expires, or the screen is left.

/// Which Quick Connect attempt is current; a loop that finds it changed has been replaced.
static QC_ATTEMPT: AtomicU64 = AtomicU64::new(0);

fn qc_stop(ctx: &Ctx) {
    QC_ATTEMPT.fetch_add(1, Ordering::SeqCst);
    jellyfin::quick_connect_cancel(&ctx.state::<Jellyfin>());
}

pub fn qc_start(ctx: Ctx, weak: Weak<App>) {
    let attempt = QC_ATTEMPT.fetch_add(1, Ordering::SeqCst) + 1;
    let current = move || QC_ATTEMPT.load(Ordering::SeqCst) == attempt;
    ui(&weak, |app| {
        app.set_qc_code("".into());
        app.set_qc_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::quick_connect_start_with(&jf).await {
            Ok(code) if current() => {
                // Development: lets a test approve the code from another "device".
                if std::env::var_os("BLOOM_LOG_EVENTS").is_some() {
                    eprintln!("bloom: quick connect code {code}");
                }
                ui(&weak, move |app| app.set_qc_code(code.into()));
            }
            Ok(_) => return,
            Err(e) => {
                if current() {
                    ui(&weak, move |app| app.set_qc_error(e.to_string().into()));
                }
                return;
            }
        }
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if !current() {
                return;
            }
            match jellyfin::quick_connect_poll_with(&jf, remember(&weak).await).await {
                Ok(Some(account)) => {
                    if current() {
                        launch(&ctx, &weak);
                        enter_home(&ctx, &weak, account);
                    }
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    if current() {
                        ui(&weak, move |app| {
                            app.set_qc_code("".into());
                            app.set_qc_error(e.to_string().into());
                        });
                    }
                    return;
                }
            }
        }
    });
}

// ---- Saved accounts

/// "Continue" on a saved account.
pub fn resume(ctx: Ctx, weak: Weak<App>, server_id: String, user_id: String) {
    ui(&weak, |app| {
        app.set_busy(true);
        app.set_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::switch_account_with(&jf, &server_id, &user_id).await {
            Ok(account) => {
                launch(&ctx, &weak);
                enter_home(&ctx, &weak, account)
            }
            Err(e) => {
                // The account may have ended on the server, which forgets it here.
                load_saved(&ctx, &weak);
                show_error(&weak, e.to_string());
            }
        }
    });
}

/// "Forget" on a saved account.
pub fn forget(ctx: Ctx, weak: Weak<App>, server_id: String, user_id: String) {
    ui(&weak, |app| {
        app.set_busy(true);
        app.set_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::forget_account_with(&jf, &server_id, &user_id).await {
            Ok(_) => {
                load_saved(&ctx, &weak);
                ui(&weak, |app| app.set_busy(false));
            }
            Err(e) => show_error(&weak, e.to_string()),
        }
    });
}

/// The account menu opened: the other accounts saved for this server. UI thread.
pub fn account_menu_opened(ctx: Ctx, weak: Weak<App>) {
    let jf = ctx.state::<Jellyfin>();
    let saved = jellyfin::saved_accounts_with(&jf);
    let server = saved.iter().find(|a| a.current).map(|a| a.server.id.clone());
    let others: Vec<AccountItem> = saved
        .into_iter()
        .filter(|a| !a.current && Some(&a.server.id) == server.as_ref())
        .map(|a| AccountItem { server_id: a.server.id.into(), user_id: a.user.id.into(), initial: initial(&a.user.name), name: a.user.name.into(), image: slint::Image::default(), has_image: false })
        .collect();
    let wanted: Vec<(String, String)> = others.iter().map(|o| (o.server_id.to_string(), o.user_id.to_string())).collect();
    // Now, not when the event loop next turns: the menu sizes itself as it opens, a moment after this.
    if let Some(app) = weak.upgrade() {
        app.set_other_accounts(ModelRc::new(VecModel::from(others)));
    }
    load_pictures(&ctx, &weak, wanted, Place::Menu);
}

/// Which list of saved accounts a picture is for.
#[derive(Clone, Copy)]
enum Place {
    Menu,
    Chooser,
}

/// What each saved account's picture was the last time it was asked for in this run (None: no
/// picture), so reopening the menu doesn't ask the server again.
static PICTURES: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Option<Vec<u8>>>>> = std::sync::LazyLock::new(Default::default);

/// Fetches the pictures of saved accounts, each asked as itself, and puts them in their rows.
fn load_pictures(ctx: &Ctx, weak: &Weak<App>, wanted: Vec<(String, String)>, place: Place) {
    for (server, user) in wanted {
        let (ctx, weak) = (ctx.clone(), weak.clone());
        bloom_core::rt::spawn(async move {
            let key = format!("{server}/{user}");
            let cached = PICTURES.lock().unwrap_or_else(|e| e.into_inner()).get(&key).cloned();
            let bytes = match cached {
                Some(found) => found,
                None => {
                    let jf = ctx.state::<Jellyfin>();
                    // A failure isn't remembered: the next look asks again.
                    let Ok(found) = jellyfin::saved_avatar_with(&jf, &server, &user).await else { return };
                    PICTURES.lock().unwrap_or_else(|e| e.into_inner()).insert(key, found.clone());
                    found
                }
            };
            let Some(bytes) = bytes else { return };
            let decoded = bloom_core::rt::spawn_blocking(move || {
                let rgba = image::load_from_memory(&bytes).ok()?.into_rgba8();
                let (w, h) = rgba.dimensions();
                Some(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h))
            })
            .await
            .ok()
            .flatten();
            let Some(pixels) = decoded else { return };
            ui(&weak, move |app| {
                let picture = slint::Image::from_rgba8(pixels);
                match place {
                    Place::Menu => {
                        let rows = app.get_other_accounts();
                        if let Some(i) = (0..rows.row_count()).find(|i| rows.row_data(*i).is_some_and(|r| r.server_id == server.as_str() && r.user_id == user.as_str())) {
                            if let Some(mut row) = rows.row_data(i) {
                                row.image = picture;
                                row.has_image = true;
                                rows.set_row_data(i, row);
                            }
                        }
                    }
                    Place::Chooser => {
                        let rows = app.get_saved_accounts();
                        if let Some(i) = (0..rows.row_count()).find(|i| rows.row_data(*i).is_some_and(|r| r.server_id == server.as_str() && r.user_id == user.as_str())) {
                            if let Some(mut row) = rows.row_data(i) {
                                row.image = picture;
                                row.has_image = true;
                                rows.set_row_data(i, row);
                            }
                        }
                    }
                }
            });
        });
    }
}

/// "Switch to" in the account menu.
pub fn switch_account(ctx: Ctx, weak: Weak<App>, server_id: String, user_id: String) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::switch_account_with(&jf, &server_id, &user_id).await {
            Ok(account) => enter_home(&ctx, &weak, account),
            Err(e) => watch::say(&weak, e.to_string()),
        }
    });
}

/// "Add another account": the sign-in screen at this server's account step, with a way back.
pub fn add_account(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    ui(&w, move |app| {
        let address = app.get_address().to_string();
        sign_in_at(ctx, weak, address);
    });
}

/// The sign-in screen at the account step of the server at `address`, with a way back. UI thread
/// or not: it hands the work on.
pub fn sign_in_at(ctx: Ctx, weak: Weak<App>, address: String) {
    let w = weak.clone();
    ui(&w, move |app| {
        app.set_can_cancel_sign_in(true);
        app.set_address(address.clone().into());
        app.set_username("".into());
        app.set_password("".into());
        app.set_error("".into());
        app.set_screen(Screen::SignIn);
        load_wall(&ctx, &weak);
        check_server(ctx, weak, address);
    });
}

/// "Back to Bloom" from adding an account.
pub fn cancel_sign_in(ctx: Ctx, weak: Weak<App>) {
    qc_stop(&ctx);
    ui(&weak, |app| app.set_can_cancel_sign_in(false));
    nav::reset(ctx, weak, Route::Home);
}

pub fn sign_out(ctx: Ctx, weak: Weak<App>) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let _ = jellyfin::sign_out_with(&jf).await;
        signed_out(&ctx, &weak);
    });
}

/// The server says the sign-in has ended: the sign-in screen, saying so.
pub fn session_ended(ctx: &Ctx, weak: &Weak<App>) {
    crate::home::forget();
    crate::live::forget();
    ask_for_server(ctx, weak, String::new());
    ui(weak, |app| app.set_signin_notice("Your sign-in ended on the server. Sign in again to carry on.".into()));
}

/// Bloom has no account any more: back to the server step.
pub fn signed_out(ctx: &Ctx, weak: &Weak<App>) {
    crate::home::forget();
    crate::live::forget();
    ask_for_server(ctx, weak, String::new());
}

/// How long the loader shows at least, so a fast load doesn't make it flash.
const LOADER_MIN: Duration = Duration::from_millis(900);
/// And at most, in case the page never says it is ready.
const LOADER_MAX: Duration = Duration::from_secs(12);

thread_local! {
    static LOADER_SINCE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// The loader that shows after signing in, until Home has loaded (`loader_ready`).
fn launch(_ctx: &Ctx, weak: &Weak<App>) {
    ui(weak, |app| {
        app.set_launch_ending(false);
        app.set_launching(true);
        LOADER_SINCE.with(|s| s.set(Some(std::time::Instant::now())));
        let weak = app.as_weak();
        slint::Timer::single_shot(LOADER_MAX, move || {
            if let Some(app) = weak.upgrade() {
                loader_ready(&app);
            }
        });
    });
}

/// The first page is up: the loader fades out (once it has shown for a moment). UI thread.
pub fn loader_ready(app: &App) {
    if !app.get_launching() || app.get_launch_ending() {
        return;
    }
    let shown = LOADER_SINCE.with(std::cell::Cell::take).map_or(LOADER_MIN, |at| at.elapsed());
    let wait = LOADER_MIN.saturating_sub(shown);
    let weak = app.as_weak();
    slint::Timer::single_shot(wait, move || {
        if let Some(app) = weak.upgrade() {
            app.set_launch_ending(true);
        }
    });
}

fn enter_home(ctx: &Ctx, weak: &Weak<App>, account: Account) {
    qc_stop(ctx);
    crate::home::forget();
    let (user, server, address) = (account.user.name, account.server.name, account.server.address);
    let version = account.server.version.clone();
    let line = format!("{}, Jellyfin {}", host(&address), version);
    crate::downloads::load_initial(ctx.clone(), weak.clone());
    ui(weak, move |app| {
        app.set_signin_notice("".into());
        app.set_notice("".into());
        app.set_user_initial(user.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into());
        app.set_user_name(user.into());
        app.set_has_user_image(false);
        app.set_server_line(line.into());
        app.set_server_name(server.into());
        app.set_server_version(version.into());
        // Kept so "Add another account" can start at this server.
        app.set_address(address.into());
        app.set_username("".into());
        app.set_password("".into());
        app.set_busy(false);
        app.set_error("".into());
        app.set_can_cancel_sign_in(false);
    });
    nav::reset(ctx.clone(), weak.clone(), Route::Home);
    load_avatar(ctx.clone(), weak.clone());
}

/// Counts sign-ins, so a picture that arrives after someone else signed in is dropped.
static AVATAR_RUN: AtomicU64 = AtomicU64::new(0);

/// The signed-in person's picture, if they have one, for the top bar and the account menu.
fn load_avatar(ctx: Ctx, weak: Weak<App>) {
    let mine = AVATAR_RUN.fetch_add(1, Ordering::SeqCst) + 1;
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let Ok(Some(image)) = bloom_core::images::avatar_with(&jf).await else { return };
        let Some(pixels) = cards::pixels(&ctx, &image.path(160)).await else { return };
        ui(&weak, move |app| {
            if AVATAR_RUN.load(Ordering::SeqCst) == mine {
                app.set_user_image(slint::Image::from_rgba8(pixels));
                app.set_has_user_image(true);
            }
        });
    });
}

//! The session beyond the page on screen: whether the server answers (the offline banner and its
//! retry), the server menu, and what happens when the server says the sign-in has ended.

use crate::flow;
use crate::{App, ServerItem};
use bloom_core::downloads::Downloads;
use bloom_core::jellyfin::{self, Error, Jellyfin};
use bloom_core::Ctx;
use slint::{ModelRc, VecModel, Weak};
use std::sync::OnceLock;

/// Set once at launch, for the places that learn of a lost sign-in without a context in hand.
static CTX: OnceLock<Ctx> = OnceLock::new();

pub fn init(ctx: &Ctx) {
    let _ = CTX.set(ctx.clone());
}

/// Whether `message` is what the core says when the sign-in has ended.
pub fn is_signed_out(message: &str) -> bool {
    message == Error::SignedOut.to_string()
}

/// The server has ended this sign-in: back to the sign-in screen, saying so.
pub fn ended(weak: &Weak<App>) {
    let Some(ctx) = CTX.get().cloned() else { return };
    flow::session_ended(&ctx, weak);
}

/// The banner "Can't reach ..." is up, or isn't.
pub fn set_offline(weak: &Weak<App>, offline: bool) {
    let _ = weak.upgrade_in_event_loop(move |app| app.set_offline(offline));
}

/// "Retry" on the offline banner.
pub fn retry(ctx: Ctx, weak: Weak<App>) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let Some(account) = jellyfin::saved_accounts_with(&jf).into_iter().find(|a| a.current) else { return };
        let reachable = jellyfin::ping_server_with(&jf, &account.server.id).await.map(|r| r.reachable).unwrap_or(false);
        if reachable {
            let _ = bloom_core::downloads::sync_with(&jf, &ctx.state::<Downloads>()).await;
            let _ = weak.upgrade_in_event_loop(move |app| {
                app.set_offline(false);
                app.set_notice("".into());
            });
            crate::nav::reload(ctx, weak);
        } else {
            let name = account.server.name;
            let _ = weak.upgrade_in_event_loop(move |app| app.set_notice(format!("{name} still isn't answering.").into()));
        }
    });
}

/// The server menu opened: the other servers saved on this computer.
pub fn server_menu_opened(ctx: Ctx, weak: Weak<App>) {
    let jf = ctx.state::<Jellyfin>();
    let others: Vec<ServerItem> = jellyfin::servers_with(&jf)
        .into_iter()
        .filter(|s| !s.current)
        .map(|s| ServerItem {
            id: s.server.id.clone().into(),
            name: s.server.name.clone().into(),
            sub: s.accounts.first().map(|a| format!("As {}", a.name)).unwrap_or_else(|| "Sign in".into()).into(),
        })
        .collect();
    let _ = weak.upgrade_in_event_loop(move |app| app.set_other_servers(ModelRc::new(VecModel::from(others))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_cores_signed_out_message_counts_as_a_lost_sign_in() {
        assert!(is_signed_out(&Error::SignedOut.to_string()));
        assert!(!is_signed_out(&Error::AccountEnded("alyx".into()).to_string()));
        assert!(!is_signed_out("The server refused the request (500)."));
    }
}

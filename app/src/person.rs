//! A person's page: who they are, above the films and shows they're in. It's the list page with an
//! introduction on top, so paging and cards work as they do in a library.

use crate::cards::{self, PlainCard};
use crate::nav::{self, Source};
use crate::App;
use bloom_core::jellyfin::Jellyfin;
use bloom_core::library::{self, PersonInfo};
use bloom_core::Ctx;
use slint::Weak;

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// "31 January 1970" from "1970-01-31".
fn long_date(iso: &str) -> Option<String> {
    let mut parts = iso.split('-');
    let year: u32 = parts.next()?.parse().ok()?;
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let month = MONTHS.get(month.checked_sub(1)?)?;
    (1..=31).contains(&day).then(|| format!("{day} {month} {year}"))
}

/// "Born 31 January 1970 in Springfield" and "Died 2 March 2020", whichever are known.
fn life_lines(info: &PersonInfo) -> String {
    let mut lines = Vec::new();
    let born = info.born.as_deref().and_then(long_date);
    match (born, info.place.as_deref()) {
        (Some(date), Some(place)) => lines.push(format!("Born {date} in {place}")),
        (Some(date), None) => lines.push(format!("Born {date}")),
        (None, Some(place)) => lines.push(format!("Born in {place}")),
        (None, None) => {}
    }
    if let Some(date) = info.died.as_deref().and_then(long_date) {
        lines.push(format!("Died {date}"));
    }
    lines.join("\n")
}

/// Shows the page for `id`: the name at once, the rest as it arrives. UI thread.
pub fn show(ctx: &Ctx, weak: &Weak<App>, app: &App, id: String, name: String) {
    nav::list_page(app, &name, false, false);
    app.set_lib_has_intro(true);
    app.set_lib_empty_title("Nothing in your libraries with them".into());
    app.set_lib_empty_text("".into());
    app.set_lib_intro_initial(name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into());

    let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match library::person_with(&jf, &id).await {
            Ok(info) => {
                let (lines, text) = (life_lines(&info), info.overview.clone().unwrap_or_default());
                let photo = info.image.as_ref().map(|i| i.path(480));
                let (ctx2, weak2) = (ctx.clone(), weak.clone());
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if cards::generation() != generation {
                        return;
                    }
                    app.set_lib_intro_lines(lines.into());
                    app.set_lib_intro_text(text.into());
                    if let Some(path) = photo {
                        cards::fetch_one(&ctx2, &weak2, generation, path, |app, image| {
                            app.set_lib_intro_image(image);
                            app.set_lib_intro_has_image(true);
                        });
                    }
                });
            }
            // Credits can still be listed without the biography.
            Err(e) => crate::watch::say(&weak, e.to_string()),
        }
        load_credits(&ctx, &weak, id, 0);
    });
}

/// A page (0-based) of the person's films and shows.
pub fn load_credits(ctx: &Ctx, weak: &Weak<App>, id: String, page: u32) {
    let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match library::person_credits_with(&jf, &id, page * 60).await {
            Ok(grid) => {
                let plain: Vec<PlainCard> = grid.cards.iter().map(|c| PlainCard::from_core(c, cards::card_width(false))).collect();
                nav::apply_list(&ctx, &weak, generation, plain, grid.total as usize, false, Some(Source { id, sort: 0, person: true, downloaded: false }), page);
            }
            Err(e) => nav::failed(&weak, generation, e.to_string()),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(born: Option<&str>, died: Option<&str>, place: Option<&str>) -> PersonInfo {
        PersonInfo { id: "p".into(), name: "N".into(), overview: None, born: born.map(Into::into), died: died.map(Into::into), place: place.map(Into::into), image: None }
    }

    #[test]
    fn dates_read_in_words() {
        assert_eq!(long_date("1970-01-31").as_deref(), Some("31 January 1970"));
        assert_eq!(long_date("2001-12-05").as_deref(), Some("5 December 2001"));
        for bad in ["", "1970", "1970-13-01", "1970-00-10", "1970-05-00", "1970-05-32", "x-y-z"] {
            assert_eq!(long_date(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_life_is_told_with_what_is_known() {
        assert_eq!(life_lines(&info(Some("1970-01-31"), None, Some("Springfield"))), "Born 31 January 1970 in Springfield");
        assert_eq!(life_lines(&info(Some("1970-01-31"), Some("2020-03-02"), None)), "Born 31 January 1970\nDied 2 March 2020");
        assert_eq!(life_lines(&info(None, None, Some("Oslo"))), "Born in Oslo");
        assert_eq!(life_lines(&info(None, None, None)), "");
        // A date the server gives badly is left out rather than shown garbled.
        assert_eq!(life_lines(&info(Some("garbage"), None, None)), "");
    }
}

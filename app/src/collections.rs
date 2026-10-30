//! Adding a title to collections, taking it out, making a new one, and taking titles out of the
//! collection whose page is open.

use crate::nav;
use crate::{App, CollectionChoiceData};
use bloom_core::jellyfin::{Error, Jellyfin};
use bloom_core::library;
use bloom_core::Ctx;
use slint::{Model, ModelRc, SharedString, VecModel, Weak};
use std::cell::{Cell, RefCell};

thread_local! {
    /// The title the dialog is about. UI thread only.
    static ITEM: RefCell<String> = const { RefCell::new(String::new()) };
    /// Something changed since the dialog opened, so the page behind it is stale. UI thread only.
    static CHANGED: Cell<bool> = const { Cell::new(false) };
}

/// What the person is told when the server turns an edit down or fails.
fn message(e: &Error) -> String {
    match e {
        Error::Status(401) | Error::Status(403) => "This account isn't allowed to change collections.".into(),
        e => e.to_string(),
    }
}

fn rows_of(choices: Vec<library::CollectionChoice>) -> ModelRc<CollectionChoiceData> {
    let rows: Vec<CollectionChoiceData> = choices
        .into_iter()
        .map(|c| CollectionChoiceData { id: SharedString::from(c.id), title: SharedString::from(c.title), contains: c.contains })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Opens the dialog for the title page on screen.
pub fn open(ctx: Ctx, weak: Weak<App>) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let data = app.get_title_data();
        let id = data.id.to_string();
        ITEM.with(|i| *i.borrow_mut() = id.clone());
        CHANGED.with(|c| c.set(false));
        app.set_collections_heading(data.title.clone());
        app.set_collections_error("".into());
        app.set_collections_rows(ModelRc::new(VecModel::<CollectionChoiceData>::default()));
        app.set_collections_loading(true);
        app.set_collections_open(true);
        bloom_core::rt::spawn(async move { refresh(ctx, weak, id).await });
    });
}

async fn refresh(ctx: Ctx, weak: Weak<App>, item_id: String) {
    let jf = ctx.state::<Jellyfin>();
    let found = library::collection_choices_with(&jf, &item_id).await;
    let _ = weak.upgrade_in_event_loop(move |app| {
        if !app.get_collections_open() {
            return;
        }
        app.set_collections_loading(false);
        match found {
            Ok(choices) => app.set_collections_rows(rows_of(choices)),
            Err(e) => app.set_collections_error(message(&e).into()),
        }
    });
}

/// A row was pressed: the title goes into the collection, or out of it when it was in.
pub fn toggle(ctx: Ctx, weak: Weak<App>, collection_id: String, contains: bool) {
    let item_id = ITEM.with(|i| i.borrow().clone());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let done = if contains {
            library::collection_remove_with(&jf, &collection_id, &item_id).await
        } else {
            library::collection_add_with(&jf, &collection_id, &item_id).await
        };
        let _ = weak.upgrade_in_event_loop(move |app| match done {
            Ok(()) => {
                CHANGED.with(|c| c.set(true));
                app.set_collections_error("".into());
                let rows = app.get_collections_rows();
                if let Some(i) = rows.iter().position(|r| r.id == collection_id.as_str()) {
                    if let Some(mut row) = rows.row_data(i) {
                        row.contains = !contains;
                        rows.set_row_data(i, row);
                    }
                }
            }
            Err(e) => app.set_collections_error(message(&e).into()),
        });
    });
}

/// "Create": a new collection holding this title.
pub fn create(ctx: Ctx, weak: Weak<App>, name: String) {
    let item_id = ITEM.with(|i| i.borrow().clone());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match library::collection_create_with(&jf, &name, &item_id).await {
            Ok(_) => {
                let _ = weak.upgrade_in_event_loop(|_| CHANGED.with(|c| c.set(true)));
                refresh(ctx, weak, item_id).await;
            }
            Err(e) => {
                let _ = weak.upgrade_in_event_loop(move |app| app.set_collections_error(message(&e).into()));
            }
        }
    });
}

/// The dialog was closed: the page behind it is drawn again if anything changed.
pub fn closed(ctx: Ctx, weak: Weak<App>) {
    if CHANGED.with(|c| c.replace(false)) {
        nav::reload(ctx, weak);
    }
}

/// Takes a title out of the collection whose page is open.
pub fn remove_from_page(ctx: Ctx, weak: Weak<App>, item_id: String) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |app| {
        let collection_id = app.get_title_data().id.to_string();
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            match library::collection_remove_with(&jf, &collection_id, &item_id).await {
                Ok(()) => nav::reload(ctx, weak),
                Err(e) => {
                    let _ = weak.upgrade_in_event_loop(move |app| app.set_title_action_error(message(&e).into()));
                }
            }
        });
    });
}

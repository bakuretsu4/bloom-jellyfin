//! What the core's parts share: the UI's `Shell`, and one instance of each service.
//!
//! This stands where Tauri's `AppHandle` and managed state stood. `state::<T>()` finds a service
//! by its type, as `app.state::<T>()` did, so the services that reach for each other (playback
//! asks Jellyfin, downloads ask playback) still do, with nothing about the UI in between.

use crate::shell::Shell;
use serde::Serialize;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct Ctx(Arc<Inner>);

struct Inner {
    shell: Arc<dyn Shell>,
    services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl Ctx {
    pub fn new(shell: Arc<dyn Shell>) -> Self {
        Self(Arc::new(Inner { shell, services: RwLock::default() }))
    }

    /// Registers the one instance of `T`. Registering a type twice replaces the first.
    pub fn manage<T: Send + Sync + 'static>(&self, service: T) {
        self.0.services.write().expect("service registry poisoned").insert(TypeId::of::<T>(), Arc::new(service));
    }

    /// The registered `T`. Panics if there is none: a service is registered at startup or the
    /// program is wrong, the same contract as Tauri's `State`.
    pub fn state<T: Send + Sync + 'static>(&self) -> Arc<T> {
        self.try_state().unwrap_or_else(|| panic!("no {} registered", std::any::type_name::<T>()))
    }

    pub fn try_state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let service = self.0.services.read().expect("service registry poisoned").get(&TypeId::of::<T>())?.clone();
        service.downcast::<T>().ok()
    }

    pub fn shell(&self) -> &dyn Shell {
        &*self.0.shell
    }

    /// Tells the page (or whatever the UI shows) something changed. The payload's shape is the
    /// contract with the UI; a payload that can't be serialised is dropped and reported.
    pub fn emit<S: Serialize>(&self, event: &str, payload: S) -> Result<(), serde_json::Error> {
        let payload = serde_json::to_value(payload)?;
        self.0.shell.emit(event, payload);
        Ok(())
    }
}

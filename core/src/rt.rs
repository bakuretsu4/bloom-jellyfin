//! The async runtime the core runs on.
//!
//! The core is started by a user interface that owns a tokio runtime (Tauri's, or the one the
//! Slint app makes). It hands over a handle with `init`, so code on any thread (a GTK callback,
//! mpv's event thread) can spawn work without being inside the runtime. Without an `init` the core
//! makes a runtime of its own on first use, as Tauri's did, which suits tests and small tools.

use std::future::Future;
use std::sync::{LazyLock, OnceLock};
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinHandle;

static HANDLE: OnceLock<Handle> = OnceLock::new();
/// The runtime made on first use when the UI never handed one over.
static OWN: LazyLock<Runtime> = LazyLock::new(|| Builder::new_multi_thread().enable_all().build().expect("couldn't start a tokio runtime"));

/// Hands the core the UI's runtime. Call it once, before anything else in the core runs; a second
/// call, or one after the core made its own runtime, is ignored.
pub fn init(handle: Handle) {
    let _ = HANDLE.set(handle);
}

fn handle() -> &'static Handle {
    HANDLE.get().unwrap_or_else(|| OWN.handle())
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    handle().spawn(future)
}

pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    handle().spawn_blocking(f)
}

/// Runs a future to completion from a thread that is not part of the runtime.
pub fn block_on<F: Future>(future: F) -> F::Output {
    handle().block_on(future)
}

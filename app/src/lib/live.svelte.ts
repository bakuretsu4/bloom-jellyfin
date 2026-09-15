// What the server has said changed since pages loaded (src-tauri/src/live.rs): the latest
// watched state, progress and favourite per item, which pages lay over what they fetched.
import { SvelteMap } from "svelte/reactivity";
import * as api from "./api";
import { forgetCardDetail } from "./hover.svelte";

export const live = new SvelteMap<string, api.UserDataChange>();

let installed = false;

/** Follows the server's updates. `onChange` hears that something changed, to reload rows. */
export function installLive(onChange: (what: "userdata" | "library") => void) {
  if (installed) return;
  installed = true;
  void api.onUserDataChanged((changes) => {
    for (const change of changes) {
      live.delete(change.itemId);
      live.set(change.itemId, change);
      // The oldest go first once it's large; a page reload has fetched those afresh by then.
      if (live.size > 1000) live.delete(live.keys().next().value!);
      // A hover card fetched before the change would show it stale.
      forgetCardDetail(change.itemId);
    }
    onChange("userdata");
  });
  void api.onLibraryChanged(() => onChange("library"));
  let connections = 0;
  void api.onLiveConnected(() => {
    // Whatever changed while the connection was down went unheard, so what was heard before may be
    // out of date: drop it and reload the rows. Not for the first connection, when pages have only
    // just loaded.
    live.clear();
    if (connections++ > 0) onChange("userdata");
  });
}

/** A change made here wins over an older one heard from the server. */
export function forgetLive(itemId: string) {
  live.delete(itemId);
}

/** Another account's updates don't apply. */
export function clearLive() {
  live.clear();
}

// Language names for the codes Jellyfin and mpv use ("jpn", "en"), in the viewer's language.
const names = (() => {
  try {
    return new Intl.DisplayNames(undefined, { type: "language" });
  } catch {
    return null;
  }
})();

export function languageName(code: string | null | undefined): string | null {
  if (!code || code === "und") return null;
  try {
    return names?.of(code) ?? code;
  } catch {
    return code;
  }
}

/** "Japanese, English", or the first `max` and a count past that. */
export function languageList(codes: string[], max = 2): string {
  const list = codes.map((c) => languageName(c) ?? c);
  return list.length > max ? `${list.slice(0, max).join(", ")} +${list.length - max}` : list.join(", ");
}

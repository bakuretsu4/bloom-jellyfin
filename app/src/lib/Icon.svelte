<script lang="ts" module>
  // Material Symbols (Rounded, weight 400), imported one by one so only the icons in use are
  // bundled. Each has an outlined and a filled form; `filled` picks the filled one, which M3
  // uses for the selected item in navigation and for toggles that are on.
  import o0 from "@material-symbols/svg-400/rounded/person.svg?raw";
  import f0 from "@material-symbols/svg-400/rounded/person-fill.svg?raw";
  import o1 from "@material-symbols/svg-400/rounded/menu.svg?raw";
  import f1 from "@material-symbols/svg-400/rounded/menu-fill.svg?raw";
  import o2 from "@material-symbols/svg-400/rounded/home.svg?raw";
  import f2 from "@material-symbols/svg-400/rounded/home-fill.svg?raw";
  import o3 from "@material-symbols/svg-400/rounded/video_library.svg?raw";
  import f3 from "@material-symbols/svg-400/rounded/video_library-fill.svg?raw";
  import o4 from "@material-symbols/svg-400/rounded/search.svg?raw";
  import f4 from "@material-symbols/svg-400/rounded/search-fill.svg?raw";
  import o5 from "@material-symbols/svg-400/rounded/play_circle.svg?raw";
  import f5 from "@material-symbols/svg-400/rounded/play_circle-fill.svg?raw";
  import o6 from "@material-symbols/svg-400/rounded/power_settings_new.svg?raw";
  import f6 from "@material-symbols/svg-400/rounded/power_settings_new-fill.svg?raw";
  import o7 from "@material-symbols/svg-400/rounded/play_arrow.svg?raw";
  import f7 from "@material-symbols/svg-400/rounded/play_arrow-fill.svg?raw";
  import o8 from "@material-symbols/svg-400/rounded/pause.svg?raw";
  import f8 from "@material-symbols/svg-400/rounded/pause-fill.svg?raw";
  import o9 from "@material-symbols/svg-400/rounded/skip_next.svg?raw";
  import f9 from "@material-symbols/svg-400/rounded/skip_next-fill.svg?raw";
  import o10 from "@material-symbols/svg-400/rounded/fast_forward.svg?raw";
  import f10 from "@material-symbols/svg-400/rounded/fast_forward-fill.svg?raw";
  import o11 from "@material-symbols/svg-400/rounded/skip_previous.svg?raw";
  import f11 from "@material-symbols/svg-400/rounded/skip_previous-fill.svg?raw";
  import o12 from "@material-symbols/svg-400/rounded/volume_up.svg?raw";
  import f12 from "@material-symbols/svg-400/rounded/volume_up-fill.svg?raw";
  import o13 from "@material-symbols/svg-400/rounded/volume_off.svg?raw";
  import f13 from "@material-symbols/svg-400/rounded/volume_off-fill.svg?raw";
  import o14 from "@material-symbols/svg-400/rounded/closed_caption.svg?raw";
  import f14 from "@material-symbols/svg-400/rounded/closed_caption-fill.svg?raw";
  import o15 from "@material-symbols/svg-400/rounded/settings.svg?raw";
  import f15 from "@material-symbols/svg-400/rounded/settings-fill.svg?raw";
  import o16 from "@material-symbols/svg-400/rounded/theaters.svg?raw";
  import f16 from "@material-symbols/svg-400/rounded/theaters-fill.svg?raw";
  import o17 from "@material-symbols/svg-400/rounded/fullscreen.svg?raw";
  import f17 from "@material-symbols/svg-400/rounded/fullscreen-fill.svg?raw";
  import o18 from "@material-symbols/svg-400/rounded/check.svg?raw";
  import f18 from "@material-symbols/svg-400/rounded/check-fill.svg?raw";
  import o19 from "@material-symbols/svg-400/rounded/favorite.svg?raw";
  import f19 from "@material-symbols/svg-400/rounded/favorite-fill.svg?raw";
  import o20 from "@material-symbols/svg-400/rounded/keyboard.svg?raw";
  import f20 from "@material-symbols/svg-400/rounded/keyboard-fill.svg?raw";
  import o21 from "@material-symbols/svg-400/rounded/close.svg?raw";
  import f21 from "@material-symbols/svg-400/rounded/close-fill.svg?raw";
  import o22 from "@material-symbols/svg-400/rounded/more_vert.svg?raw";
  import f22 from "@material-symbols/svg-400/rounded/more_vert-fill.svg?raw";
  import o23 from "@material-symbols/svg-400/rounded/sort.svg?raw";
  import f23 from "@material-symbols/svg-400/rounded/sort-fill.svg?raw";
  import o24 from "@material-symbols/svg-400/rounded/download.svg?raw";
  import f24 from "@material-symbols/svg-400/rounded/download-fill.svg?raw";
  import o25 from "@material-symbols/svg-400/rounded/delete.svg?raw";
  import f25 from "@material-symbols/svg-400/rounded/delete-fill.svg?raw";
  import o26 from "@material-symbols/svg-400/rounded/chevron_left.svg?raw";
  import f26 from "@material-symbols/svg-400/rounded/chevron_left-fill.svg?raw";
  import o27 from "@material-symbols/svg-400/rounded/chevron_right.svg?raw";
  import f27 from "@material-symbols/svg-400/rounded/chevron_right-fill.svg?raw";
  import o28 from "@material-symbols/svg-400/rounded/add.svg?raw";
  import f28 from "@material-symbols/svg-400/rounded/add-fill.svg?raw";
  import o29 from "@material-symbols/svg-400/rounded/arrow_back.svg?raw";
  import f29 from "@material-symbols/svg-400/rounded/arrow_back-fill.svg?raw";
  import o30 from "@material-symbols/svg-400/rounded/tune.svg?raw";
  import f30 from "@material-symbols/svg-400/rounded/tune-fill.svg?raw";
  import o31 from "@material-symbols/svg-400/rounded/filter_list.svg?raw";
  import f31 from "@material-symbols/svg-400/rounded/filter_list-fill.svg?raw";
  import o32 from "@material-symbols/svg-400/rounded/info.svg?raw";
  import f32 from "@material-symbols/svg-400/rounded/info-fill.svg?raw";
  import o33 from "@material-symbols/svg-400/rounded/star.svg?raw";
  import f33 from "@material-symbols/svg-400/rounded/star-fill.svg?raw";
  import o34 from "@material-symbols/svg-400/rounded/edit.svg?raw";
  import f34 from "@material-symbols/svg-400/rounded/edit-fill.svg?raw";
  import o35 from "@material-symbols/svg-400/rounded/logout.svg?raw";
  import f35 from "@material-symbols/svg-400/rounded/logout-fill.svg?raw";
  import o36 from "@material-symbols/svg-400/rounded/dns.svg?raw";
  import f36 from "@material-symbols/svg-400/rounded/dns-fill.svg?raw";
  import o37 from "@material-symbols/svg-400/rounded/palette.svg?raw";
  import f37 from "@material-symbols/svg-400/rounded/palette-fill.svg?raw";
  import o38 from "@material-symbols/svg-400/rounded/dark_mode.svg?raw";
  import f38 from "@material-symbols/svg-400/rounded/dark_mode-fill.svg?raw";
  import o39 from "@material-symbols/svg-400/rounded/light_mode.svg?raw";
  import f39 from "@material-symbols/svg-400/rounded/light_mode-fill.svg?raw";
  import o40 from "@material-symbols/svg-400/rounded/movie.svg?raw";
  import f40 from "@material-symbols/svg-400/rounded/movie-fill.svg?raw";
  import o41 from "@material-symbols/svg-400/rounded/keyboard_arrow_down.svg?raw";
  import f41 from "@material-symbols/svg-400/rounded/keyboard_arrow_down-fill.svg?raw";
  import o42 from "@material-symbols/svg-400/rounded/fullscreen_exit.svg?raw";
  import f42 from "@material-symbols/svg-400/rounded/fullscreen_exit-fill.svg?raw";
  import o43 from "@material-symbols/svg-400/rounded/cloud_download.svg?raw";
  import f43 from "@material-symbols/svg-400/rounded/cloud_download-fill.svg?raw";
  import o44 from "@material-symbols/svg-400/rounded/check_circle.svg?raw";
  import f44 from "@material-symbols/svg-400/rounded/check_circle-fill.svg?raw";

  const d = (svg: string) => /<path d="([^"]+)"/.exec(svg)?.[1] ?? "";
  const shapes = {
    "user": [d(o0), d(f0)],
    "menu": [d(o1), d(f1)],
    "home": [d(o2), d(f2)],
    "library": [d(o3), d(f3)],
    "search": [d(o4), d(f4)],
    "play-circle": [d(o5), d(f5)],
    "power": [d(o6), d(f6)],
    "play": [d(o7), d(f7)],
    "pause": [d(o8), d(f8)],
    "next": [d(o9), d(f9)],
    "next-episode": [d(o10), d(f10)],
    "prev": [d(o11), d(f11)],
    "vol": [d(o12), d(f12)],
    "mute": [d(o13), d(f13)],
    "cc": [d(o14), d(f14)],
    "gear": [d(o15), d(f15)],
    "theater": [d(o16), d(f16)],
    "expand": [d(o17), d(f17)],
    "check": [d(o18), d(f18)],
    "heart": [d(o19), d(f19)],
    "help": [d(o20), d(f20)],
    "close": [d(o21), d(f21)],
    "more": [d(o22), d(f22)],
    "sort": [d(o23), d(f23)],
    "download": [d(o24), d(f24)],
    "trash": [d(o25), d(f25)],
    "chevl": [d(o26), d(f26)],
    "chevr": [d(o27), d(f27)],
    "sym:add": [d(o28), d(f28)],
    "sym:arrow_back": [d(o29), d(f29)],
    "sym:tune": [d(o30), d(f30)],
    "sym:filter_list": [d(o31), d(f31)],
    "sym:info": [d(o32), d(f32)],
    "sym:star": [d(o33), d(f33)],
    "sym:edit": [d(o34), d(f34)],
    "sym:logout": [d(o35), d(f35)],
    "sym:dns": [d(o36), d(f36)],
    "sym:palette": [d(o37), d(f37)],
    "sym:dark_mode": [d(o38), d(f38)],
    "sym:light_mode": [d(o39), d(f39)],
    "sym:movie": [d(o40), d(f40)],
    "sym:expand_more": [d(o41), d(f41)],
    "sym:fullscreen_exit": [d(o42), d(f42)],
    "sym:cloud_download": [d(o43), d(f43)],
    "sym:check_circle": [d(o44), d(f44)],
  } as const;
  export type IconName = keyof typeof shapes;
  /** Transport glyphs are solid shapes in Material Symbols; their outlined forms look hollow. */
  const SOLID = new Set<IconName>(["play", "pause", "next", "prev", "next-episode", "vol", "mute", "play-circle"]);
</script>

<script lang="ts">
  let { name, size = 24, filled = false }: { name: IconName; size?: number; filled?: boolean } = $props();
  let path = $derived(shapes[name][filled || SOLID.has(name) ? 1 : 0]);
</script>

<svg width={size} height={size} viewBox="0 -960 960 960" fill="currentColor" aria-hidden="true">
  <path d={path} />
</svg>

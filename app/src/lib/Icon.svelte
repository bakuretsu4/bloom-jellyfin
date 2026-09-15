<script lang="ts" module>
  // Drawn on a 20 grid with flat, square-ended shapes, from the design prototype. Not an
  // icon library: generic rounded-stroke sets are one of the tells DESIGN.md rules out.
  const shapes = {
    user: '<circle cx="10" cy="6.4" r="3.4"/><path d="M3.3 17.8a6.7 6.7 0 0 1 13.4 0z"/>',
    menu: '<rect x="2.4" y="4.2" width="15.2" height="2"/><rect x="2.4" y="9" width="15.2" height="2"/><rect x="2.4" y="13.8" width="15.2" height="2"/>',
    home: '<polygon points="10,1.9 18.2,8.6 18.2,17.8 12.3,17.8 12.3,11.9 7.7,11.9 7.7,17.8 1.8,17.8 1.8,8.6"/>',
    library:
      '<rect x="2" y="2.4" width="7.2" height="7.2"/><rect x="10.8" y="2.4" width="7.2" height="7.2"/><rect x="2" y="11.2" width="7.2" height="7.2"/><rect x="10.8" y="11.2" width="7.2" height="7.2"/>',
    search:
      '<circle cx="8.9" cy="8.9" r="5.3" fill="none" stroke="currentColor" stroke-width="2.3"/><path d="M13.3 13.3 18.2 18.2" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="square"/>',
    "play-circle": '<polygon points="5.4,3 5.4,17 17,10"/>',
    power:
      '<path d="M5.5 5.4a6.9 6.9 0 1 0 9 0" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="square"/><rect x="8.9" y="1.6" width="2.2" height="8.2"/>',
    play: '<polygon points="5.6,3.2 5.6,16.8 16.8,10"/>',
    pause: '<rect x="5.2" y="3.4" width="3.6" height="13.2"/><rect x="11.2" y="3.4" width="3.6" height="13.2"/>',
    next: '<rect x="13.4" y="3.4" width="2.6" height="13.2"/><polygon points="3.4,3.6 3.4,16.4 12.4,10"/>',
    // Two steps and a bar: past the end of this one, unlike the chapter skip beside it.
    "next-episode": '<polygon points="1.8,3.6 1.8,16.4 8.6,10"/><polygon points="8.4,3.6 8.4,16.4 15.2,10"/><rect x="15.6" y="3.4" width="2.6" height="13.2"/>',
    prev: '<rect x="4" y="3.4" width="2.6" height="13.2"/><polygon points="16.6,3.6 16.6,16.4 7.6,10"/>',
    // The two bars scale with the volume (see .lv in the watch screen), so the icon is a meter.
    vol: '<polygon points="2.6,7.4 6.2,7.4 10.6,3.4 10.6,16.6 6.2,12.6 2.6,12.6"/><rect class="lv lv1" x="12.6" y="7.6" width="1.9" height="4.8"/><rect class="lv lv2" x="16" y="5.4" width="1.9" height="9.2"/>',
    mute: '<polygon points="2.6,7.4 6.2,7.4 10.6,3.4 10.6,16.6 6.2,12.6 2.6,12.6"/><rect x="12.4" y="8.6" width="5.6" height="2.8"/>',
    cc: '<path fill-rule="evenodd" d="M1.4 3.6H18.6V16.4H1.4Z M4.4 11.4H11.8V13.2H4.4Z M13.2 11.4H15.6V13.2H13.2Z"/>',
    gear: '<rect x="3.6" y="2.6" width="1.8" height="14.8"/><rect x="9.1" y="2.6" width="1.8" height="14.8"/><rect x="14.6" y="2.6" width="1.8" height="14.8"/><rect x="1.8" y="6" width="5.4" height="2.6"/><rect x="7.3" y="11" width="5.4" height="2.6"/><rect x="12.8" y="4.6" width="5.4" height="2.6"/>',
    theater: '<path fill-rule="evenodd" d="M1.4 3.4H18.6V16.6H1.4Z M1.4 6.6H18.6V13.4H1.4Z"/>',
    expand:
      '<polygon points="1.8,1.8 8,1.8 8,4 4,4 4,8 1.8,8"/><polygon points="18.2,1.8 12,1.8 12,4 16,4 16,8 18.2,8"/><polygon points="1.8,18.2 8,18.2 8,16 4,16 4,12 1.8,12"/><polygon points="18.2,18.2 12,18.2 12,16 16,16 16,12 18.2,12"/>',
    check: '<polygon points="7.6,13.1 3.4,8.9 1.6,10.7 7.6,16.7 18.4,5.9 16.6,4.1"/>',
    heart:
      '<path d="M10 17.4S2.4 12.6 2.4 7.9A3.9 3.9 0 0 1 10 6.2a3.9 3.9 0 0 1 7.6 1.7c0 4.7-7.6 9.5-7.6 9.5z"/>',
    // A keyboard: the shortcuts panel.
    help: '<path fill-rule="evenodd" d="M1.4 4.6H18.6V15.4H1.4Z M3.7 6.8H5.7V8.8H3.7Z M7.2 6.8H9.2V8.8H7.2Z M10.7 6.8H12.7V8.8H10.7Z M14.2 6.8H16.2V8.8H14.2Z M3.7 10.6H5.7V12.6H3.7Z M7.2 10.6H12.8V12.6H7.2Z M14.3 10.6H16.3V12.6H14.3Z"/>',
    close: '<polygon points="3.4,5 5,3.4 10,8.4 15,3.4 16.6,5 11.6,10 16.6,15 15,16.6 10,11.6 5,16.6 3.4,15 8.4,10"/>',
    more: '<rect x="8.6" y="2.8" width="2.8" height="2.8"/><rect x="8.6" y="8.6" width="2.8" height="2.8"/><rect x="8.6" y="14.4" width="2.8" height="2.8"/>',
    sort: '<rect x="2.4" y="4" width="15.2" height="2"/><rect x="2.4" y="9" width="10.4" height="2"/><rect x="2.4" y="14" width="5.6" height="2"/>',
    download: '<rect x="8.8" y="2.2" width="2.4" height="7"/><polygon points="5.2,8.4 14.8,8.4 10,14.4"/><rect x="2.8" y="15.8" width="14.4" height="2.2"/>',
    trash: '<rect x="7.4" y="1.8" width="5.2" height="2"/><rect x="2.6" y="4.4" width="14.8" height="2.2"/><polygon points="4.4,7.6 15.6,7.6 14.6,18.2 5.4,18.2"/>',
    chevl:'<polygon points="13.6,2.6 16.4,5.4 11.8,10 16.4,14.6 13.6,17.4 6.2,10"/>',
    chevr: '<polygon points="6.4,2.6 3.6,5.4 8.2,10 3.6,14.6 6.4,17.4 13.8,10"/>',
  };
  export type IconName = keyof typeof shapes;
</script>

<script lang="ts">
  let { name, size = 20 }: { name: IconName; size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
  {@html shapes[name]}
</svg>

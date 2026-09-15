# Bloom's part of the AppImage's startup hook. build-appimage.sh deletes the GTK hook's
# `export GDK_BACKEND=x11` and adds this at the end, so it runs after the hook's own settings.
# AppRun sources the hook with `set -e`.
#
# No forced backend: GTK picks Wayland on a Wayland desktop and X11 on an X11 one, as it does for
# Bloom outside the AppImage. Forcing X11 was for bundled Wayland libraries, which are now left out.
#
# The desktop's look. The hook forces GTK_THEME to Adwaita (light unless the GTK theme's name says
# "dark"), and its GTK_PATH has Debian's and Fedora's GTK module folders but not Arch's, so KDE's
# window-decorations and colorreload modules never load: the window got GNOME's title bar instead
# of the desktop's. Unless APPIMAGE_GTK_THEME picked something, GTK uses the desktop's own theme.
# (The cursor also needs GSettings' dconf backend, which build-appimage.sh bundles.)
if [ "${GTK_THEME:-}" = "Adwaita:${GTK_THEME_VARIANT:-}" ]; then
  unset GTK_THEME
fi
export GTK_PATH="${GTK_PATH:+$GTK_PATH:}/usr/lib/gtk-3.0"
#
# The WebKit bundled here is Ubuntu's, which carries disable-nvidia-dmabuf.patch: with NVIDIA's
# driver it turns WebKit's GPU renderer off. The page then has no accelerated backing store, and
# WebKit crashes the first time it enters compositing mode (opening a title, whose view transition
# needs it; WEBKIT_DISABLE_COMPOSITING_MODE doesn't stop that). Upstream WebKit has no such check.
# The patch's own switch, WEBKIT_FORCE_DMABUF_RENDERER, turns it back off (tested 2026-09-15,
# driver 615.71):
#   - On Wayland the renderer works as upstream's does.
#   - Without Wayland, hardware buffers gave a blank window (tried under XWayland), so buffers go
#     through shared memory instead, which renders properly. Only with NVIDIA's driver loaded:
#     anywhere else the patch isn't in play and the hardware path works.
# Anything already set in the environment wins; WEBKIT_FORCE_DMABUF_RENDERER=0 keeps Ubuntu's
# behaviour.
#
# VA-API, Intel's and AMD's hardware decoding. libva has to be the system's: it loads the
# system's drivers, from the folder its distro built it for, and only drivers made for its own
# version or older. Ubuntu's copy in the AppImage failed to initialise on Arch (tested 2026-09-15,
# Iris Xe), so mpv decoded everything in software. build-appimage.sh moves it to libva-fallback,
# which is used only when the system lacks any of the four (Bloom still starts, without VA-API).
bloom_system_libva=1
for bloom_lib in libva.so.2 libva-drm.so.2 libva-wayland.so.2 libva-x11.so.2; do
  bloom_found=
  for bloom_dir in /usr/lib/x86_64-linux-gnu /usr/lib64 /usr/lib /lib/x86_64-linux-gnu /lib64; do
    if [ -e "$bloom_dir/$bloom_lib" ]; then bloom_found=1; break; fi
  done
  [ -n "$bloom_found" ] || bloom_system_libva=
done
if [ -z "$bloom_system_libva" ]; then
  export LD_LIBRARY_PATH="${LD_LIBRARY_PATH:+$LD_LIBRARY_PATH:}$APPDIR/usr/lib/libva-fallback"
fi
unset bloom_system_libva bloom_lib bloom_found bloom_dir
#
if [ -n "${WAYLAND_DISPLAY:-}" ]; then
  export WEBKIT_FORCE_DMABUF_RENDERER="${WEBKIT_FORCE_DMABUF_RENDERER:-1}"
elif [ -e /proc/driver/nvidia/version ]; then
  export WEBKIT_FORCE_DMABUF_RENDERER="${WEBKIT_FORCE_DMABUF_RENDERER:-1}"
  export WEBKIT_DMABUF_RENDERER_FORCE_SHM="${WEBKIT_DMABUF_RENDERER_FORCE_SHM:-1}"
fi

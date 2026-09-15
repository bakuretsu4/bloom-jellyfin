# Getting started

## Install

Bloom comes as an AppImage: a single file that runs on most Linux distributions, with nothing to
install.

1. Download `Bloom_<version>_amd64.AppImage` from the [releases page](../../../releases).
2. Make it executable and run it:

   ```sh
   chmod +x Bloom_0.1.0_amd64.AppImage
   ./Bloom_0.1.0_amd64.AppImage
   ```

   Or right-click it in your file manager, allow it to run as a program in its properties, and
   double-click it.

**You'll need**

- 64-bit x86 Linux with glibc 2.39 or newer: Ubuntu 24.04, Debian 13, Fedora 40 or later, current
  Arch, or anything as recent.
- FUSE 2, which AppImages use to open themselves: `fuse2` on Arch, `libfuse2t64` on Ubuntu,
  `fuse-libs` on Fedora. Without it, start Bloom with `--appimage-extract-and-run`.
- A Jellyfin server to connect to (Bloom is tested with Jellyfin 10.11).

**Adding Bloom to your app menu (optional).** An AppImage doesn't add itself to your menu. Tools
such as [Gear Lever](https://flathub.org/apps/it.mijorus.gearlever) or AppImageLauncher can do it
for you, or keep the file somewhere like `~/Applications` and start it from there.

## Connect to your server

On first launch Bloom asks for your server.

- **On your network:** servers on your local network that answer Jellyfin's discovery show up in a
  list. Choose one and **Connect**. Use **Search again** if yours didn't appear yet.
- **By address:** type the address you use for Jellyfin's web app, such as
  `http://192.168.1.20:8096` or `https://jellyfin.example.com`. A server behind a reverse proxy at
  a path (`https://example.com/jellyfin`) works too.

If your server doesn't appear in the list, see
[Servers that don't show up](troubleshooting.md#my-server-isnt-listed-on-your-network).

## Sign in

Sign in with your Jellyfin username and password, or with **Quick Connect** if your server has it
turned on: Bloom shows a code, and on a device where you're already signed in to Jellyfin you open
your profile, choose Quick Connect, and enter that code.

**Stay signed in** (on by default) keeps you signed in between launches. Bloom never stores your
password: it keeps the sign-in token your server gives it, in a file only your user can read. With
Stay signed in off, the token lives in memory and you sign in again next time.

## Several servers and accounts

Bloom remembers every server and account you sign in to.

- **Another server:** click the server's name in the top bar. The menu lists your other servers
  (with the account you use on each) and **Manage servers**.
- **Another account:** the account menu (the person icon) lists your other accounts on this server
  under **Switch to**, and has **Add another account** and **Sign out**.
- **Signed out:** the sign-in screen lists saved accounts under **Welcome back**; the ✕ next to
  one forgets it.
- **The Servers screen** (**Manage servers**, or <kbd>Ctrl</kbd>+<kbd>K</kbd> → Servers) lists
  your servers and whether each is reachable, and lets you add one by address or remove one.
  Removing a server you have downloads from asks whether to **delete them** or **keep them** on
  disk.

Each account has its own downloads, remembered audio and subtitle choices, and watch progress.

## Finding your way around

- **Home:** the spotlight, Continue Watching, Next Up, recently added and your libraries.
- **Libraries:** browse each library as a grid, sorted the way you choose.
- **Search:** press <kbd>/</kbd>, or use the search box at the top.
- **Command palette:** <kbd>Ctrl</kbd>+<kbd>K</kbd> jumps to any title, library, page or
  settings section by typing part of its name.
- **Keyboard shortcuts:** press <kbd>?</kbd> for the full list.
- **Zoom:** <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>Ctrl</kbd>+<kbd>−</kbd>, or <kbd>Ctrl</kbd> with
  the scroll wheel; <kbd>Ctrl</kbd>+<kbd>0</kbd> resets it.

Bloom updates live as your server changes: a film marked watched on your TV, a new favourite or a
new episode shows up without reloading.

## Updating

Download the new AppImage and replace the old file. Your sign-ins, settings and downloads are kept:
they live in your home folder, not in the AppImage.

## Uninstalling

1. Delete the AppImage.
2. To remove everything Bloom saved as well:
   - `~/.local/share/dev.bloom.app`: sign-ins, settings, remembered track choices and the list of
     downloads.
   - `~/.cache/dev.bloom.app`: cached artwork, safe to delete at any time.
   - Your download folder (by default `~/Videos/Bloom`), if you want the downloaded videos gone too.

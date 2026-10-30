#!/usr/bin/env bash
# Builds Bloom's Flatpak from the last commit and puts a bundle in packaging/out/. Uncommitted changes
# aren't included, so what's built is what's in git. Needs flatpak and the Flatpak Builder
# (`flatpak install --user flathub org.flatpak.Builder org.freedesktop.Sdk//25.08
# org.freedesktop.Sdk.Extension.rust-stable//25.08`) and network for the first build's sources.
#
#   packaging/build-flatpak.sh            build, and install it for this user to try
#   packaging/build-flatpak.sh --bundle   also write packaging/out/Bloom-<version>-x86_64.flatpak
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/packaging/out"
src="$out/flatpak-src"
mkdir -p "$out"

rm -rf "$src" && mkdir -p "$src"
git -C "$root" archive --format=tar HEAD | tar -x -C "$src"

# The crates, as offline sources, from the lock file of the commit being built.
venv="$out/flatpak-venv"
if [ ! -x "$venv/bin/python" ]; then
  python3 -m venv "$venv"
  "$venv/bin/pip" install -q aiohttp tomlkit PyYAML
fi
generator="$out/flatpak-cargo-generator.py"
[ -f "$generator" ] || curl -fsSL -o "$generator" https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
"$venv/bin/python" "$generator" "$src/app/Cargo.lock" -o "$root/packaging/flatpak/cargo-sources.json"

flatpak run --user org.flatpak.Builder --user --force-clean --install --disable-rofiles-fuse \
  --state-dir="$out/flatpak-state" --repo="$out/flatpak-repo" \
  "$out/flatpak-build" "$root/packaging/flatpak/dev.bloom.app.yml"

if [ "${1:-}" = "--bundle" ]; then
  version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/app/Cargo.toml" | head -1)
  flatpak build-bundle "$out/flatpak-repo" "$out/Bloom-$version-x86_64.flatpak" dev.bloom.app
  ls -lh "$out"/*.flatpak
fi

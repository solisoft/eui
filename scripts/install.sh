#!/bin/sh
# Fetch the EUI client for whatever machine this is, and put it on the PATH.
#
#   ./scripts/install.sh [tag]        # default: rolling
#   curl -fsSL https://raw.githubusercontent.com/solisoft/eui/main/scripts/install.sh | sh
#
#   EUI_DEST=/usr/local/bin sh install.sh     # somewhere else
#   EUI_DEST=. sh install.sh                  # just leave it here
#   EUI_REPO=me/eui sh install.sh             # a fork's releases
#
# `sh` and not `bash`: a script people are invited to pipe into a shell
# should run in the one the machine already has. There are no arrays and no
# `pipefail` here for that reason, and it has to keep working on Alpine's ash
# and on Debian's dash, where `bash` is a package rather than a given.
#
# Nothing is read from stdin — no prompts, no confirmations — because when
# this arrives through a pipe stdin is the script itself.

set -eu

TAG="${1:-rolling}"
REPO="${EUI_REPO:-solisoft/eui}"
DEST="${EUI_DEST:-$HOME/.local/bin}"

say() { echo "install: $*"; }
die() { echo "install: $*" >&2; exit 1; }

# ---- which machine is this ------------------------------------------------
#
# Three desktop builds exist and no more (`.github/workflows/build.yml`), so
# the interesting cases here are the ones that do not: a Linux on ARM and an
# Intel Mac both `uname` perfectly well and have nothing to download. Each is
# refused by name rather than left to fail at the first launch, when the
# reason would be a dynamic linker error about a file that was never the
# right one.

os="$(uname -s)"
arch="$(uname -m)"

case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
esac

case "$os" in
  Linux)
    [ "$arch" = "x86_64" ] ||
      die "only x86_64 Linux is built; this is $arch. Build it with \`cargo build --release -p eui-client\`."
    asset="eui-x86_64-linux.tar.gz"
    member="eui-x86_64-linux"
    binary="eui"
    ;;
  Darwin)
    # Apple Silicon only, and Rosetta is not the answer: it runs an x86_64
    # binary and there is no x86_64 binary to run.
    [ "$arch" = "aarch64" ] ||
      die "only Apple Silicon macOS is built; this is $arch."
    asset="eui-aarch64-macos.tar.gz"
    member="eui-aarch64-macos"
    binary="eui"
    ;;
  MINGW* | MSYS* | CYGWIN*)
    [ "$arch" = "x86_64" ] ||
      die "only x86_64 Windows is built; this is $arch."
    asset="eui-x86_64-windows.zip"
    member="eui-x86_64-windows.exe"
    binary="eui.exe"
    ;;
  *)
    die "no build for $os. Linux, macOS and Windows are built; Android and iOS are packaged differently — see the README."
    ;;
esac

# ---- where it is going ----------------------------------------------------
#
# Settled before anything is fetched: a destination that cannot be written to
# is worth finding out about now rather than after sixteen megabytes. Made
# absolute at the same time, so that `EUI_DEST=.` still produces a PATH line
# somebody can paste — and so the advice is never the literal `.`, which is a
# thing to keep off a PATH rather than add to one.

mkdir -p "$DEST" || die "could not create $DEST."
DEST="$(cd "$DEST" && pwd)"
[ -w "$DEST" ] || die "$DEST is not writable. Set EUI_DEST, or run this where it is."

# ---- fetch ----------------------------------------------------------------
#
# curl if it is here, wget if it is not; a machine with neither cannot be
# helped by a downloader. `-f` on curl and `--server-response` semantics on
# wget matter more than they look: without them a 404 from a tag that does
# not exist is written to disk as an HTML page and unpacked as a corrupt
# archive, and the error a person then reads is about gzip.

url="https://github.com/$REPO/releases/download/$TAG/$asset"

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fL --progress-bar -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  # No `--show-progress`: busybox's wget, which is the wget on Alpine and on
  # most containers, does not have it and exits rather than ignoring it.
  # Plain `-O` prints progress on both.
  fetch() { wget -O "$2" "$1"; }
else
  die "neither curl nor wget is installed."
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "fetching $url"
fetch "$url" "$tmp/$asset" || die "could not fetch $asset from the '$TAG' release."

# ---- unpack ---------------------------------------------------------------

case "$asset" in
  *.tar.gz)
    tar -xzf "$tmp/$asset" -C "$tmp" ||
      die "$asset did not unpack; a 404 saved to disk looks exactly like this."
    ;;
  *.zip)
    command -v unzip >/dev/null 2>&1 || die "unzip is needed for $asset and is not installed."
    unzip -q "$tmp/$asset" -d "$tmp" || die "$asset did not unpack."
    ;;
esac

[ -f "$tmp/$member" ] || die "no $member inside $asset."

# There is no published checksum to compare this against — the release
# carries binaries and nothing else — so the guarantee is the HTTPS
# connection and GitHub's own certificate, not this number. It is printed so
# that two machines installing the same tag can be shown to have the same
# file, which is the question people actually ask.
if command -v sha256sum >/dev/null 2>&1; then
  say "sha256 $(sha256sum "$tmp/$member" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then
  say "sha256 $(shasum -a 256 "$tmp/$member" | cut -d' ' -f1)"
fi

# ---- install --------------------------------------------------------------

chmod +x "$tmp/$member"

# `mv` and never `cp`: on Linux and macOS this replaces the directory entry
# and leaves the old inode alone, so a window already running from the last
# version keeps running from it rather than having its executable rewritten
# underneath it. Copying over a live binary is how a running client dies
# mid-frame with a bus error.
mv -f "$tmp/$member" "$DEST/$binary" ||
  die "could not write $DEST/$binary."

say "installed $DEST/$binary"

# ---- what it will want at run time ----------------------------------------
#
# The Linux build links ALSA for the audio device; Vulkan, Wayland and
# xkbcommon are loaded when they are needed. ALSA is the one that stops the
# process before it has drawn anything, so it is the one worth naming here
# rather than leaving to a first launch that says nothing but the name of a
# shared object.
if [ "$os" = "Linux" ] && command -v ldd >/dev/null 2>&1; then
  if ldd "$DEST/$binary" 2>/dev/null | grep -q "libasound.so.2 => not found"; then
    say "warning: libasound.so.2 is missing — install alsa-lib (Arch), libasound2 (Debian/Ubuntu) or alsa-lib (Fedora)."
  fi
fi

# A binary somewhere the shell does not look is a binary nobody can run, and
# `$HOME/.local/bin` is on the PATH of some distributions and not others.
case ":${PATH}:" in
  *":$DEST:"*) ;;
  *)
    say "note: $DEST is not on your PATH. Add it with:"
    echo
    echo "    export PATH=\"$DEST:\$PATH\""
    echo
    ;;
esac

cat <<EOF

    $binary                                          the shell: a tab strip, and an address to type into
    $binary wss://host/_eui/session/app              one window on one application
    $binary --allow clipboard.read,notifications URL  grant capabilities up front (or --allow all)

EOF

if [ "$os" = "Darwin" ]; then
  say "this is the bare binary. For EUI.app in /Applications — and the quarantine it avoids — use scripts/install-macos.sh instead."
fi

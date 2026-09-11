#!/bin/bash
# Wrap a macOS executable in a .app bundle, and a .dmg beside it.
#
#   wrap-macos-app.sh <binary> <AppName> <bundle-id> <out-dir>
#
# The .app is the unit macOS actually understands: a bare executable cannot
# carry an icon, a display name, or a URL scheme. The DMG is only made when
# hdiutil is present, so this is still runnable on Linux for a smoke test.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BINARY="${1:?binary path}"
APP_NAME="${2:?app name}"
BUNDLE_ID="${3:?bundle id}"
OUT_DIR="${4:?output dir}"
VERSION="${VERSION:-0.1.0}"
# The icon, committed as .icns because macOS reads nothing else, and made
# from assets/icon/eui.svg by scripts/make-icons.py. `ICON=` points this at
# another one, the way the same script dresses another app's bundle.
ICON="${ICON:-$SCRIPT_DIR/../assets/icon/eui.icns}"

[ -f "$BINARY" ] || { echo "wrap-macos-app: no binary at $BINARY" >&2; exit 1; }

APP="$OUT_DIR/$APP_NAME.app"
CONTENTS="$APP/Contents"
rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

cp "$BINARY" "$CONTENTS/MacOS/$APP_NAME"
chmod +x "$CONTENTS/MacOS/$APP_NAME"

# The icon has to be inside the bundle and named by the plist; a file left
# beside it is not looked at. Without one the Finder draws the blank
# document sheet, which is what a download nobody trusts looks like.
if [ -f "$ICON" ]; then
  cp "$ICON" "$CONTENTS/Resources/$APP_NAME.icns"
  ICON_KEY="    <key>CFBundleIconFile</key><string>$APP_NAME</string>"
else
  echo "wrap-macos-app: no icon at $ICON — the bundle will show the blank one" >&2
  ICON_KEY=""
fi

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>$APP_NAME</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
$ICON_KEY
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>$APP_NAME</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

printf 'APPL????' > "$CONTENTS/PkgInfo"

# A desktop artifact carries its application encrypted — there is no
# unencrypted desktop build — so it needs its key at every launch, not only
# at build time. Without one it starts, fails to unlock, and exits: from the
# Finder that reads as an icon that flashes and vanishes, with the reason on
# a stderr nobody sees. A `.env` beside the executable is the documented
# place for it (standalone.rs resolves env_dir from current_exe).
#
# This does put the key inside the artifact. That is only acceptable because
# the caller generates a throwaway key per build for a demo, so it guards
# nothing. Never pass a key here that also unlocks a real installation.
if [ -n "${BUNDLE_KEY:-}" ]; then
  printf 'SOLI_BUNDLE_KEY=%s\n' "$BUNDLE_KEY" > "$CONTENTS/MacOS/.env"
  chmod 600 "$CONTENTS/MacOS/.env"
  echo "wrap-macos-app: wrote the launch key beside the executable"
fi

# Sign the bundle, ad-hoc, with no certificate. Assembling a .app by copying
# a binary and writing a plist leaves the bundle itself unsigned, and Apple
# Silicon refuses to run an unsigned bundle outright — reporting it to the
# user as "damaged", which sends them looking for a corrupt download. This
# is not notarisation and does not clear Gatekeeper's quarantine on a
# download; it turns a hard refusal into the ordinary unidentified-developer
# prompt, which right-click - Open can answer.
if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$APP" 2>&1 | sed 's/^/wrap-macos-app: /' || {
    echo "wrap-macos-app: codesign failed" >&2; exit 1; }
  codesign --verify --deep --strict "$APP" || {
    echo "wrap-macos-app: signature did not verify" >&2; exit 1; }
  echo "wrap-macos-app: ad-hoc signed"
fi

echo "wrap-macos-app: built $APP"

( cd "$OUT_DIR" && zip -qr "$APP_NAME-aarch64-macos.zip" "$APP_NAME.app" )
echo "wrap-macos-app: built $OUT_DIR/$APP_NAME-aarch64-macos.zip"

# The staged volume: the app, and a symlink to /Applications beside it, so
# installing is the drag between the two icons the window is laid out to
# show. Without the symlink the DMG is a folder holding one app and the user
# is left to guess where it goes — a guess many answer by running it from
# the mounted image, which then breaks the moment they eject.
stage_volume() {
  local stage="$1"
  rm -rf "$stage"
  mkdir -p "$stage/.background"
  cp -R "$APP" "$stage/" || return 1
  ln -s /Applications "$stage/Applications" || return 1
  # The disk itself, in the sidebar and on the desktop while it is mounted.
  # It is the same icon the app carries; a volume with none is the generic
  # white disk, which says nothing about what was downloaded.
  if [ -f "$ICON" ]; then cp "$ICON" "$stage/.VolumeIcon.icns"; fi
  make_background "$stage/.background" || true
  # Nothing drawn, nothing to carry: an empty directory would still ride
  # along in the image.
  rmdir "$stage/.background" 2>/dev/null || true
}

# Draws the backdrop at both resolutions and folds them into one TIFF, which
# is how a Finder background carries a HiDPI representation. Prints the file
# name it left in the directory, or fails, and the caller then dresses the
# window without a picture.
make_background() {
  local dir="$1"
  local js="$SCRIPT_DIR/dmg-background.js"
  [ -f "$js" ] || return 1
  command -v osascript >/dev/null 2>&1 || return 1
  osascript -l JavaScript "$js" "$APP_NAME" "$dir/background.png" 1 >/dev/null 2>&1 || return 1
  if osascript -l JavaScript "$js" "$APP_NAME" "$dir/background@2x.png" 2 >/dev/null 2>&1 &&
     command -v tiffutil >/dev/null 2>&1 &&
     tiffutil -cathidpicheck "$dir/background.png" "$dir/background@2x.png" \
       -out "$dir/background.tiff" >/dev/null 2>&1; then
    rm -f "$dir/background.png" "$dir/background@2x.png"
    echo "background.tiff"
    return 0
  fi
  rm -f "$dir/background@2x.png"
  echo "background.png"
}

# Finder is the only thing that writes a .DS_Store, so the window's size,
# its icon positions and its backdrop can only be set by scripting it on the
# mounted volume. That is also the step most likely to be refused: a machine
# that withholds automation permission answers with error -1743, and one
# with no Finder to talk to answers by never returning at all. So it runs on
# a leash, and a failure costs the layout, not the DMG.
dress_window() {
  local vol="$1" background="$2" picture=""
  if [ -n "$background" ]; then
    picture="set background picture of opts to file \".background:$background\""
  fi

  osascript >/dev/null 2>&1 <<APPLESCRIPT &
tell application "Finder"
  tell disk "$vol"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {240, 160, 880, 560}
    set opts to the icon view options of container window
    set arrangement of opts to not arranged
    set icon size of opts to 128
    set text size of opts to 13
    set label position of opts to bottom
    $picture
    set position of item "$APP_NAME.app" of container window to {170, 205}
    set position of item "Applications" of container window to {470, 205}
    close
    open
    update without registering applications
    delay 2
  end tell
end tell
APPLESCRIPT

  local pid=$! waited=0
  while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 90 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -9 "$pid" 2>/dev/null || true
    echo "wrap-macos-app: Finder did not answer, leaving the window undressed" >&2
    return 1
  fi
  wait "$pid"
}

# .VolumeIcon.icns is only looked at when the volume's Finder flags say it
# has a custom icon. SetFile writes that bit and comes with Xcode; where it
# is missing the same bit goes straight into the FinderInfo attribute — a
# folder's flags are the two bytes at offset 8, and kHasCustomIcon is 0x0400.
mark_volume_icon() {
  local mount="$1"
  [ -f "$mount/.VolumeIcon.icns" ] || return 0
  if command -v SetFile >/dev/null 2>&1 && SetFile -a C "$mount" 2>/dev/null; then
    return 0
  fi
  xattr -wx com.apple.FinderInfo \
    "0000000000000000040000000000000000000000000000000000000000000000" "$mount" 2>/dev/null ||
    echo "wrap-macos-app: could not mark the volume icon" >&2
}

# A mounted volume is not always free the instant the script stops touching
# it; Spotlight or Finder can still hold it for a beat.
#
# And an unmounted volume is not a detached image. The two come apart: the
# /Volumes entry goes while the device stays, and `hdiutil convert` on an
# image that is still attached answers "Resource temporarily unavailable".
# Measured on CI — five refusals over twenty seconds, with the mount point
# long gone — which is why waiting on the mount point was not enough and the
# image itself is what is waited on here, through `hdiutil info`.
#
# The device is also what the forced detach is aimed at. Forcing the mount
# point unmounts a volume that is already unmounted and leaves the device
# exactly where it was.
detach_volume() {
  local mount="$1" dev="$2" image="$3" tries=0
  while [ "$tries" -lt 10 ]; do
    hdiutil detach "$mount" >/dev/null 2>&1 && break
    sleep 2
    tries=$((tries + 1))
  done
  while hdiutil info | grep -qF "$image"; do
    if [ "$tries" -ge 20 ]; then
      echo "wrap-macos-app: $image is still attached" >&2
      return 0
    fi
    # Once the polite detach has had its ten seconds, take the device.
    if [ "$tries" -ge 10 ] && [ -n "$dev" ]; then
      hdiutil detach "$dev" -force >/dev/null 2>&1 || true
    fi
    sleep 1
    tries=$((tries + 1))
  done
  return 0
}

make_dmg() {
  local stage="$OUT_DIR/.dmg-stage"
  local rw="$OUT_DIR/.$APP_NAME-rw.dmg"
  local background mount vol attached dev image tries=0
  # `hdiutil info` names images by absolute path, and `$OUT_DIR` need not be
  # one — it is `dist` on CI.
  image="$(cd "$OUT_DIR" && pwd)/.$APP_NAME-rw.dmg"

  background="$(stage_volume "$stage")" || return 1

  # Sized to the payload with room to spare: the volume has to stay writable
  # for the .DS_Store the layout leaves behind, and an image sized exactly to
  # its contents has nowhere to put it.
  local megabytes=$(( $(du -sm "$stage" | cut -f1) + 64 ))
  rm -f "$rw"
  hdiutil create -srcfolder "$stage" -volname "$APP_NAME" -fs HFS+ \
    -fsargs "-c c=64,a=16,e=16" -format UDRW -size "${megabytes}m" -ov "$rw" >/dev/null || return 1

  # Both halves of what `attach` says are needed later: the volume to dress
  # and the device to detach. They are read from one run of it — attaching
  # twice to learn the second would be a second image.
  attached="$(hdiutil attach -readwrite -noverify -noautoopen "$rw")" || return 1
  mount="$(printf '%s\n' "$attached" | grep -Eo '/Volumes/.*$' | tail -1)"
  dev="$(printf '%s\n' "$attached" | awk '/^\/dev\// { print $1; exit }')"
  [ -n "$mount" ] || return 1
  # The volume name is not always the one asked for: mounting alongside a
  # volume of the same name gets a numbered one, and the layout has to name
  # the disk it is actually talking to.
  vol="$(basename "$mount")"

  dress_window "$vol" "$background" || true
  mark_volume_icon "$mount"
  chmod -Rf go-w "$mount" 2>/dev/null || true
  sync
  detach_volume "$mount" "$dev" "$image"

  # Even waited on, the image can still answer "Resource temporarily
  # unavailable" on a loaded runner. It is a transient, so it is retried
  # rather than allowed to lose a build that is otherwise complete.
  until hdiutil convert "$rw" -format UDZO -imagekey zlib-level=9 -ov -o "$DMG" >/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -lt 5 ] || return 1
    echo "wrap-macos-app: hdiutil convert did not take ($tries), trying again" >&2
    sleep 5
  done
  rm -f "$rw"
  rm -rf "$stage"
}

if command -v hdiutil >/dev/null 2>&1; then
  DMG="$OUT_DIR/$APP_NAME-aarch64-macos.dmg"
  if make_dmg; then
    echo "wrap-macos-app: built $DMG"
  else
    echo "wrap-macos-app: could not build the DMG" >&2
    rm -rf "$OUT_DIR/.dmg-stage" "$OUT_DIR/.$APP_NAME-rw.dmg"
    exit 1
  fi
fi


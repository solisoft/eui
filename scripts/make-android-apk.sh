#!/bin/bash
# Build the Android client as an APK you can install on a device.
#
#   make-android-apk.sh [out-dir]
#
# `cargo apk` reads the packaging out of `crates/eui-android/Cargo.toml` and
# signs the result with a debug key it generates itself, which is what makes
# the APK installable without an account, a store, or a key of your own:
#
#   adb install -r dist/eui.apk
#
# It wants the SDK and the NDK. `ANDROID_HOME` and `ANDROID_NDK_ROOT` are set
# on GitHub's ubuntu runners; on a workstation they are wherever Android
# Studio put them, usually ~/Android/Sdk and ~/Android/Sdk/ndk/<version>.
#
# `EUI_ANDROID_URL` bakes the session address into the binary. Without one
# the client opens the shell, which on a phone is a development convenience
# and not a design.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="${1:-$ROOT/dist}"

: "${ANDROID_HOME:?set ANDROID_HOME to the Android SDK (usually ~/Android/Sdk)}"
: "${ANDROID_NDK_ROOT:?set ANDROID_NDK_ROOT to the NDK (usually \$ANDROID_HOME/ndk/<version>)}"

# Which ABIs to build is the manifest's to say — `build_targets` in
# `crates/eui-android/Cargo.toml`, 64-bit ARM alone — so that a plain
# `cargo apk build` and this script cannot disagree. `EUI_ANDROID_ABIS`
# overrides it with a space-separated list for the cases where one is not
# enough; each one is another whole compile of the client.
ABIS="${EUI_ANDROID_ABIS:-}"
if [ -n "$ABIS" ]; then
  BUILD_TARGETS=""
  for abi in $ABIS; do
    rustup target add "$abi" >/dev/null 2>&1 || true
    BUILD_TARGETS="$BUILD_TARGETS --target $abi"
  done
else
  # The manifest's list, installed so the build does not stop on it.
  for abi in $(sed -n 's/^build_targets = \[\(.*\)\]/\1/p' "$ROOT/crates/eui-android/Cargo.toml" | tr -d '" ' | tr ',' ' '); do
    rustup target add "$abi" >/dev/null 2>&1 || true
  done
  BUILD_TARGETS=""
fi

command -v cargo-apk >/dev/null || { echo "make-android-apk: cargo install cargo-apk --locked" >&2; exit 1; }

# Signing. `cargo apk` mints a debug key by itself for a debug build and
# refuses a release one without a keystore, and a debug-profile Rust build
# is not a thing to put on a phone and draw conclusions from. So the release
# build is signed with Android's own debug keystore — the conventional one at
# `~/.android/debug.keystore`, with the conventional password, which is the
# key every `adb install` on every developer's machine already trusts.
#
# It is reused where it exists and made where it does not, so on a
# workstation the signature is stable and `adb install -r` upgrades in
# place. On a fresh CI runner there is nothing to reuse, so each run signs
# with a new key and a device that already has an older build wants
# `adb uninstall org.eui.client` first. That is the cost of not committing
# a key to the repository, and it is the right way round.
#
# `CARGO_APK_RELEASE_KEYSTORE` set beforehand wins over all of this, which
# is how a real signing key is used without any of it being written down
# here.
if [ -z "${CARGO_APK_RELEASE_KEYSTORE:-}" ]; then
  KEYSTORE="$HOME/.android/debug.keystore"
  if [ ! -f "$KEYSTORE" ]; then
    command -v keytool >/dev/null || { echo "make-android-apk: no keytool; install a JDK, or set CARGO_APK_RELEASE_KEYSTORE" >&2; exit 1; }
    mkdir -p "$(dirname "$KEYSTORE")"
    echo "make-android-apk: minting $KEYSTORE"
    keytool -genkeypair -v -keystore "$KEYSTORE" -storepass android -alias androiddebugkey \
      -keypass android -keyalg RSA -keysize 2048 -validity 10000 \
      -dname "CN=Android Debug,O=Android,C=US" >/dev/null
  fi
  export CARGO_APK_RELEASE_KEYSTORE="$KEYSTORE"
  export CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android
fi
echo "make-android-apk: signing with $CARGO_APK_RELEASE_KEYSTORE"

echo "make-android-apk: building${ABIS:+ for }$ABIS"
# shellcheck disable=SC2086
cargo apk build --release -p eui-android $BUILD_TARGETS

mkdir -p "$OUT_DIR"
# Where cargo-apk drops the file has moved between its versions, so it is
# looked for rather than assumed: the newest .apk under target/ is the one
# just built.
APK="$(find "$ROOT/target" -name '*.apk' -newer "$ROOT/crates/eui-android/Cargo.toml" -print 2>/dev/null | head -1)"
[ -n "$APK" ] || APK="$(find "$ROOT/target" -name '*.apk' -print 2>/dev/null | head -1)"
[ -n "$APK" ] || { echo "make-android-apk: cargo apk produced no .apk under target/" >&2; exit 1; }
cp "$APK" "$OUT_DIR/eui.apk"

echo "make-android-apk: $OUT_DIR/eui.apk"
echo "  adb install -r '$OUT_DIR/eui.apk'   # 'adb uninstall org.eui.client' first if the key changed"
echo "  adb shell am start -n org.eui.client/android.app.NativeActivity"
echo "  adb logcat -s EUI:V RustStdoutStderr:V   # the client's own words"

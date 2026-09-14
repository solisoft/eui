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
# Link against the C++ runtime. `oboe` — the audio backend — is C++, and its
# objects come into the shared object with undefined libc++ symbols; nothing
# adds a dependency on a library to resolve them from, so the result declares
# `libandroid`, `libdl`, `liblog`, `libOpenSLES`, `libm`, `libc` and no libc++
# at all. The application then installs and dies on its first frame:
#
#   UnsatisfiedLinkError … dlopen failed: cannot locate symbol
#   "__cxa_pure_virtual" referenced by "…/libeui.so"
#
# which reads like a packaging fault and is a linking one. Neither `CXXSTDLIB`
# nor `-static-libstdc++` answers it — the symbol stays undefined under both —
# and packing `libc++_shared.so` beside ours is not enough on its own either,
# because the dynamic linker only looks in libraries the object says it needs.
# It has to be named here, and carried below.
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=-lc++_shared"

cargo apk build --release -p eui-android $BUILD_TARGETS

mkdir -p "$OUT_DIR"
# Where cargo-apk drops the file has moved between its versions, so it is
# looked for rather than assumed: the newest .apk under target/ is the one
# just built.
# Newest, and never the `-unaligned` intermediate `cargo apk` leaves beside
# the real one: `find | head -1` returns whatever the directory happens to
# yield first, which was the unaligned, unsigned copy about half the time.
newest_apk() {
  find "$ROOT/target" -name '*.apk' ! -name '*-unaligned.apk' -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-
}
APK="$(newest_apk)"
[ -n "$APK" ] || { echo "make-android-apk: cargo apk produced no .apk under target/" >&2; exit 1; }
cp "$APK" "$OUT_DIR/eui.apk"

# The C++ runtime, carried in the package. `oboe` — the audio backend — is
# C++, so the shared object references libc++ symbols; `cargo apk` bundles no
# `libc++_shared.so`, and Android's own libc++ is a platform-private library
# that does not export them. The application then installs and dies on its
# first frame:
#
#   UnsatisfiedLinkError … dlopen failed: cannot locate symbol
#   "__cxa_pure_virtual" referenced by "…/libeui.so"
#
# which reads like a packaging fault and is a linking one. Static linking does
# not answer it — the symbol stays undefined however `CXXSTDLIB` and
# `-static-libstdc++` are set — so the library goes in beside ours, which is
# what the NDK intends and what every other Android build does.
SYSROOT="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
for abi_dir in $(unzip -Z1 "$OUT_DIR/eui.apk" 'lib/*/*' 2>/dev/null | cut -d/ -f2 | sort -u); do
  case "$abi_dir" in
    arm64-v8a)   triple=aarch64-linux-android ;;
    armeabi-v7a) triple=arm-linux-androideabi ;;
    x86_64)      triple=x86_64-linux-android ;;
    x86)         triple=i686-linux-android ;;
    *)           continue ;;
  esac
  [ -f "$SYSROOT/$triple/libc++_shared.so" ] || continue
  mkdir -p "$STAGE/lib/$abi_dir"
  cp "$SYSROOT/$triple/libc++_shared.so" "$STAGE/lib/$abi_dir/"
done
if [ -d "$STAGE/lib" ]; then
  (cd "$STAGE" && zip -q -r "$OUT_DIR/eui.apk" lib)
  echo "make-android-apk: libc++_shared.so packed for $(cd "$STAGE" && ls lib | tr '\n' ' ')"
fi

# Aligned after that, because adding to the archive undoes it, and before the
# signature, because aligning after would break it.
ZIPALIGN="$(find "$ANDROID_HOME/build-tools" -maxdepth 2 -name zipalign -print 2>/dev/null | sort | tail -1)"
if [ -n "$ZIPALIGN" ]; then
  "$ZIPALIGN" -f 4 "$OUT_DIR/eui.apk" "$OUT_DIR/eui-aligned.apk" && mv "$OUT_DIR/eui-aligned.apk" "$OUT_DIR/eui.apk"
fi

# Sign it ourselves. `cargo apk` prints that it is signing and, with
# build-tools 34, leaves the package without so much as a `META-INF` — which
# Android refuses at install time with `INSTALL_PARSE_FAILED_NO_CERTIFICATES`.
# Rather than depend on which of its versions signs and which only says so,
# the signature is applied here and verified, so a package that leaves this
# script is installable or the script fails.
APKSIGNER="$(find "$ANDROID_HOME/build-tools" -maxdepth 2 -name apksigner -print 2>/dev/null | sort | tail -1)"
if [ -n "$APKSIGNER" ]; then
  "$APKSIGNER" sign \
    --ks "$CARGO_APK_RELEASE_KEYSTORE" \
    --ks-pass "pass:${CARGO_APK_RELEASE_KEYSTORE_PASSWORD:-android}" \
    --ks-key-alias androiddebugkey \
    --key-pass "pass:${CARGO_APK_RELEASE_KEYSTORE_PASSWORD:-android}" \
    "$OUT_DIR/eui.apk"
  "$APKSIGNER" verify "$OUT_DIR/eui.apk" >/dev/null
  echo "make-android-apk: signed and verified"
else
  echo "make-android-apk: no apksigner under $ANDROID_HOME/build-tools; the package is unsigned" >&2
fi

echo "make-android-apk: $OUT_DIR/eui.apk"
echo "  adb install -r '$OUT_DIR/eui.apk'   # 'adb uninstall org.eui.client' first if the key changed"
echo "  adb shell am start -n org.eui.client/android.app.NativeActivity"
echo "  adb logcat -s EUI:V RustStdoutStderr:V   # the client's own words"

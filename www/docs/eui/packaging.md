# Phone packages

An application gets its own APK, or its own iOS bundle, from one command
run in its directory:

```sh
cd mail-app              # a Soli application, with an eui.toml beside soli.toml
eui package android      # → dist/com.example.mail.apk
eui package ios --sim    # → dist/Mail.app, for the simulator
eui package ios          # → dist/Mail.app, and dist/Mail.ipa when signed (a Mac)
```

The command compiles nothing. On a phone the client is the same client for
every application: one shared object on Android, one executable on iOS, and
neither knows which application it is for until something tells it. So a
package is *assembled*. The command takes the client package the release
publishes, gives it a name, an identifier, an icon and an address, and signs
it. You need no Android SDK, NDK, Xcode project or checkout of this
repository. The one exception is the iOS signature, which only a Mac can
apply.

## `eui.toml`

```toml
[app]
url       = "https://mail.example.com"   # the origin, or a whole session address
component = "inbox"                      # the name given to router_eui
label     = "Mail"                       # the name under the icon
icon      = "public/icon.png"            # a square PNG; 1024 px covers every size
version   = "1.2.0"                      # versionName / CFBundleShortVersionString

[android]
package      = "com.example.mail"        # required for `eui package android`
version_code = 3                         # raise it for every package you ship

[ios]
bundle_id = "com.example.mail"           # required for `eui package ios`
identity  = "Apple Development: you@example.com"   # codesign, for a device
profile   = "ios/Mail.mobileprovision"   # embedded before signing
```

| key | | |
|---|---|---|
| `[app] url` | required | `https://`, `wss://`, or `http://`/`ws://` for a development server. With `component`, the package opens `<url>/_eui/session/<component>`; without it, `url` must already be a session address. |
| `[app] component` | | The component's name, as `router_eui` serves it. |
| `[app] label` | required | 1 to 50 characters. |
| `[app] icon` | | A square PNG of at least 48 px. Without one, the package keeps the EUI icon. |
| `[app] version` | | The version people see. Android keeps the template's when it is left out. |
| `[android] package` | required for Android | The application id: two or more dot-separated parts, each starting with a letter. |
| `[android] version_code` | | 1 to 2 100 000 000. Android installs a package over another one only if its `version_code` is not lower. |
| `[android] signing` | | A directory holding `key.pk8` and `cert.der`, to use instead of your own key (below). |
| `[android] template`, `[ios] template` | | A client package to start from, instead of the release's. |
| `[ios] bundle_id` | required for iOS | A reverse-DNS name. |
| `[ios] identity`, `[ios] profile` | | Used for a device build on a Mac (below). |

Paths are relative to `eui.toml`. The file stands apart from `soli.toml`
because `soli.toml`'s parser refuses any section it does not know: a `[eui]`
section there would stop every soli older than the one that learned it from
starting the application. The reader is strict for the same reason. An
unknown key is an error with a line number, because a misspelt `bundle_ld`
that was silently ignored would produce a package with the wrong identity,
and nothing on the phone would say why.

## Android

`eui package android` makes these changes to the client APK:

1. **The manifest.** It sets the package name, the label, `versionName` and
   `versionCode` in the compiled `AndroidManifest.xml`. Each new value is
   appended to the manifest's string pool and the attribute is pointed at
   it, so no other string in the file moves. The resource table is left
   alone, which is how `aapt --rename-manifest-package` has always worked.
2. **The icon.** It redraws the five launcher icons the resource table
   already points at (`res/mipmap-{m,h,x,xx,xxx}hdpi-v4/ic_launcher.png`,
   48 to 192 px) from yours. Shrinking averages each output pixel over the
   area it covers, in premultiplied alpha.
3. **The address.** It adds `assets/eui.url`.
4. **The alignment.** Uncompressed entries are aligned the way `zipalign`
   aligns them: 4 bytes, and 4096 for a shared object.
5. **The signature.** It signs with APK Signature Scheme v2 and an ECDSA
   P-256 key. The package needs API 26 and v2 is read from API 24, so no
   JAR signature is written. The one the template carried is dropped,
   because it covered the template's bytes, not these.

```sh
adb install -r dist/com.example.mail.apk
```

**The signing key is yours, and is kept.** The first run creates
`~/.config/eui/android-signing/key.pk8` and `cert.der` and says that it did.
The directory is `$XDG_CONFIG_HOME/eui` when that variable is set, and
`%APPDATA%\eui` on Windows: the same place the client keeps its pins.
Android lets a package upgrade another only when both carry the same
certificate. Lose the key, and anyone who installed the application has to
uninstall it before the next version will install. The two files are what
`apksigner sign --key … --cert …` takes too: a package made here can be
re-signed with a key from elsewhere, and `[android] signing` points at a key
made elsewhere.

**What this is not:** a store upload. Google Play takes an Android App
Bundle (`.aab`) and signs it with Play App Signing. This command produces an
APK for `adb`, for a device's "install unknown apps", or for any store that
takes an APK.

## iOS

`eui package ios` writes `dist/<label>.app`: the client's executable, an
`Info.plist` with the bundle identifier, name and version set, the five
icons (120 to 1024 px), and `eui.url` beside the executable. iOS draws an
icon's transparent pixels black, so transparency is flattened onto white
and the icons have no alpha channel. The App Store refuses a 1024 px icon
that has one.

- **`--sim`** starts from the simulator build. It needs no signature:
  `xcrun simctl install booted dist/Mail.app`.
- **A device build** starts from the release's unsigned device bundle. On a
  Mac with `[ios] identity` set, the command copies `[ios] profile` in as
  `embedded.mobileprovision`, runs `codesign --force --sign <identity>` over
  the bundle, and wraps the result in `dist/<label>.ipa`. Without an
  identity it leaves the bundle unsigned, for Xcode to sign on the way to
  the device. With an identity on anything other than a Mac, it writes the
  bundle, says it is unsigned, and exits 1.

## Where the client package comes from

In order:

1. `--from <file>`;
2. `[android] template` or `[ios] template`;
3. the release asset for this version of `eui`, downloaded once into
   `~/.config/eui/templates/v<version>/` and reused after: `eui.apk`,
   `EUI-ios-device-unsigned.zip`, or `EUI-ios-simulator.zip`.

`--release <tag>` chooses a different release. `--release rolling` takes the
build of `main` and downloads it every time, because that tag moves. The
download is curl's, because GitHub answers a release download with a
redirect and the client's own fetch follows none.

A client from before 0.8.0 does not look for `eui.url`, and would open the
shell instead of the application. The command refuses such a template
rather than producing a package that fails on someone's phone. It
recognises one by the name `eui.url` missing from the binary, a literal
both clients carry for exactly that check.

## What the client does with it

On Android the client reads `assets/eui.url` through the asset manager
before anything else. On iOS it reads `eui.url` from the directory its
executable is in. Either way it opens that address as its only window: no
tab strip, no address bar, no shell. The file takes precedence over an
address compiled in at build time (`EUI_ANDROID_URL`, `EUI_IOS_URL`, which
`scripts/make-android-apk.sh` and `scripts/make-ios-app.sh` still honour
for a package built from source). With neither, the client opens the shell.

## What has been checked, and where

- `crates/eui-package` has unit tests for each part: the `eui.toml` reader
  and its refusals, the manifest patch against the real 0.7.1 manifest
  (`tests/AndroidManifest-0.7.1.bin`), the zip writer's alignment, the
  icon resampler, a signed archive verified the way the v2 scheme
  describes, and a template assembled end to end for each platform.
- CI's `build-android` job packages an application from the APK it has
  just built (`tests/template.rs`), then has Google's own tools read the
  result: `apksigner verify --min-sdk-version 26`, `zipalign -c -p 4`, and
  `aapt2 dump badging` for the package name, version and label.
- By hand, on 2026-10-01, with build-tools 34: a package made from the
  0.7.1 release APK (its shared object marked to pass the template check)
  verified under v2 with `apksigner`, passed `zipalign -c`, and
  `aapt2 dump badging` read back `com.example.mail`, `versionCode='3'`,
  `versionName='1.2.0'`, `Mail` and five icons.
- **Not yet:** an `eui package` APK installed and run on a device or an
  emulator, or an iOS bundle made by it signed on a Mac and run. The first
  template that reads `eui.url` is the build this change produces.

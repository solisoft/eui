//! Installing an application: a launcher entry, with the application's own
//! icon, that starts it in an EUI window.
//!
//! What a browser calls "install this app". An EUI application is an
//! address, and an address is a thing you have to have somewhere to type —
//! which is the difference between something you use and something you
//! visit. This puts it where the desktop keeps applications: the Linux
//! launcher, the macOS Applications folder, the Windows Start menu.
//!
//! Nothing is packaged and nothing is downloaded. The entry runs *this*
//! binary with the application's address, so an installed application is
//! exactly the session it always was, started by an icon instead of by
//! typing. Updating the client updates every installed application at
//! once, because there is only ever one client.
//!
//! The icon is the manifest's (01 §2.1), fetched as a content-addressed
//! asset and checked against the hash the publisher signed before it
//! reaches this module. That matters more here than anywhere else in the
//! client: everything else a session draws is inside a window that says
//! whose it is, and this is a tile in a dock with nothing around it.
//!
//! Three desktops, three formats, one shape — write the icon in whatever
//! form the platform reads, write a launcher that names this binary and
//! the address, and record what was written so that removing it is exact
//! rather than a guess at what the names would have been.

use std::path::{Path, PathBuf};

/// An application to install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// The manifest's `app_id`: what the entry is filed under, and what
    /// finds it again.
    pub app_id: String,
    /// What the desktop shows under the icon.
    pub name: String,
    /// The address the entry opens — the whole session URL, so that the
    /// entry needs nothing else to work.
    pub url: String,
    /// The PNG the manifest's `icon` named, already verified against it.
    pub icon: Vec<u8>,
}

/// An application that is installed, and what it put on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The manifest's `app_id`.
    pub app_id: String,
    /// The name the entry shows.
    pub name: String,
    /// The address it opens.
    pub url: String,
    /// Every file written for it, **the launcher entry first**.
    ///
    /// The order is load-bearing: [`launch`] starts `files[0]`, and on two
    /// of the three platforms the icon is written before the entry that
    /// names it. Launching the icon is what that cost the first time.
    pub files: Vec<PathBuf>,
}

/// Where the record of what is installed lives.
///
/// A record rather than a rule. The paths could nearly be recomputed from
/// the `app_id` — but not on Windows, where the file name *is* what the
/// Start menu shows and so has to be the application's name, and not
/// after the person has renamed something. What was written is the only
/// thing that can be removed with confidence.
fn record_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("EUI_INSTALLED_FILE") {
        return Some(PathBuf::from(p));
    }
    crate::manifest::config_dir().map(|d| d.join("installed"))
}

/// A file name for `app_id`: lowercase, and only what every one of the
/// three filesystems and the freedesktop icon lookup agree is a name.
///
/// An `app_id` is a publisher's string and may be anything at all. This is
/// the only place it becomes a path, so it is the only place that has to
/// be careful: no separators, no dots leading, nothing empty, and a length
/// a filesystem will take.
fn slug(app_id: &str) -> Option<String> {
    let mut out = String::new();
    for c in app_id.chars().take(96) {
        match c {
            'a'..='z' | '0'..='9' | '-' | '_' => out.push(c),
            'A'..='Z' => out.extend(c.to_lowercase()),
            '.' if !out.is_empty() => out.push('.'),
            _ if out.ends_with('-') => {}
            _ => out.push('-'),
        }
    }
    let trimmed = out.trim_matches(|c| c == '-' || c == '.');
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// This binary, which is what a launcher entry names.
///
/// On macOS inside a bundle this is `EUI.app/Contents/MacOS/EUI`, which is
/// the right thing to run: it is the same executable, and running it
/// directly rather than through `open` keeps the new window a child of the
/// entry that started it.
fn me() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))
}

/// Read the record. A line that does not parse is skipped rather than
/// discarding the rest, for [`crate::recent`]'s reason: a truncated write
/// should cost one entry and not all of them.
pub fn list() -> Vec<Entry> {
    let Some(p) = record_path() else { return Vec::new() };
    load_from(&p)
}

/// [`list`] against a named file, which is what the tests can hold still.
fn load_from(path: &Path) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let app_id = parts.next()?.trim();
            let name = parts.next()?.trim();
            let url = parts.next()?.trim();
            if app_id.is_empty() || url.is_empty() {
                return None;
            }
            Some(Entry { app_id: app_id.to_owned(), name: name.to_owned(), url: url.to_owned(), files: parts.filter(|f| !f.trim().is_empty()).map(|f| PathBuf::from(f.trim())).collect() })
        })
        .collect()
}

/// Whether this application has an entry on this machine.
pub fn installed(app_id: &str) -> bool {
    list().iter().any(|e| e.app_id == app_id)
}

/// Write the record back, whole.
fn save(entries: &[Entry]) -> Result<(), String> {
    let p = record_path().ok_or_else(|| "no configuration directory to record this in".to_owned())?;
    save_to(&p, entries)
}

/// [`save`] to a named file.
fn save_to(p: &Path, entries: &[Entry]) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = String::new();
    for e in entries {
        text.push_str(&e.app_id);
        text.push('\t');
        text.push_str(&e.name);
        text.push('\t');
        text.push_str(&e.url);
        for f in &e.files {
            text.push('\t');
            text.push_str(&f.to_string_lossy());
        }
        text.push('\n');
    }
    std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))
}

/// Write `bytes` to `path`, making its directory first.
fn put(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Everything an entry needs, fetched from the address itself.
///
/// The manifest first and by the same route a session takes (01 §2.1, 08
/// §2): the signature is checked and the publisher's key pinned or
/// compared before a byte of this is believed. Then the icon, as a
/// content-addressed asset, which makes its hash the publisher's word for
/// what the tile in the dock looks like.
///
/// An application with no `icon` in its manifest is not installable, and
/// says so. It could be given the client's own icon instead — and then
/// every installed application would wear the same picture, which is a
/// launcher full of things you cannot tell apart. A publisher who wants to
/// be installed can say what it looks like.
///
/// Blocking: two HTTPS round trips. Not to be called on the thread that
/// draws.
#[cfg(all(has_pins, has_native_net))]
pub fn from_url(url: &str) -> Result<App, String> {
    let origin = crate::assets::origin_for(url).map_err(|e| e.to_string())?;
    let pins = crate::manifest::pins_dir().ok_or_else(|| "no pin store; refusing to install an unverified application".to_owned())?;
    let m = crate::manifest::check(&origin, &pins, None).map_err(|e| e.to_string())?;
    let icon = m.icon.ok_or_else(|| format!("{} publishes no icon, so there is nothing to install it as", m.name))?;
    let bytes = crate::assets::fetch(&origin, &icon, None).map_err(|e| format!("the icon could not be fetched: {e}"))?;
    // The address as the entry will hold it: an origin with no path is
    // completed by the manifest's `entry`, exactly as opening it would be,
    // so that the launcher records a whole address and not half of one.
    let whole = crate::app::completed(url, &m.entry).unwrap_or_else(|| url.to_owned());
    let name = if m.name.trim().is_empty() { m.app_id.clone() } else { m.name };
    Ok(App { app_id: m.app_id, name, url: whole, icon: bytes })
}

/// Install `app`: write its icon and its launcher, and record both.
///
/// Installing something already installed replaces it, which is what a
/// person who clicks it twice means and also how an application that
/// changed its name or its icon is updated.
pub fn install(app: &App) -> Result<Entry, String> {
    let Some(slug) = slug(&app.app_id) else {
        return Err(format!("{:?} is not a name anything can be filed under", app.app_id));
    };
    // The old entry goes first, so that a rename does not leave the
    // previous name behind in the launcher beside the new one.
    let _ = uninstall(&app.app_id);
    let files = write_entry(app, &slug)?;
    let mut entries: Vec<Entry> = list().into_iter().filter(|e| e.app_id != app.app_id).collect();
    let mine = Entry { app_id: app.app_id.clone(), name: app.name.clone(), url: app.url.clone(), files };
    entries.push(mine.clone());
    save(&entries)?;
    Ok(mine)
}

/// Start an application through the entry that was just written for it.
///
/// The point of installing something is having it; a person who has just
/// said yes to an application has said they want it, and making them go
/// and find the icon they asked for is a step with nothing in it.
///
/// Through the *entry*, not by running the client again. That is what the
/// desktop will do every time afterwards, so it is the thing worth being
/// sure of on the one occasion somebody is watching: a bundle that will
/// not open, or a `.desktop` whose `Exec` is wrong, says so now rather
/// than the next time they reach for it.
///
/// Best effort. An entry that was written and did not start is still an
/// entry, so this never turns an install into a failure.
pub fn launch(entry: &Entry) {
    // `files[0]` and not "the one that looks right": every arm of
    // `write_entry` puts the launcher entry first, and that is the
    // contract [`Entry::files`] states.
    let Some(what) = entry.files.first() else { return };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(what);
        c
    };
    #[cfg(target_os = "linux")]
    let mut cmd = {
        // `gio launch` reads the entry the way the launcher does. Failing
        // that — a desktop without GLib's tools — the address goes to this
        // binary directly, which is what the entry says anyway.
        let mut c = std::process::Command::new("gio");
        c.arg("launch").arg(what);
        if std::process::Command::new("gio").arg("--version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_err() {
            c = std::process::Command::new(me().unwrap_or_else(|_| PathBuf::from("eui")));
            c.arg(&entry.url);
        }
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]).arg(what);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let mut cmd = std::process::Command::new(me().unwrap_or_else(|_| PathBuf::from("eui")));
    // Its own process group, or it does not outlive us.
    //
    // This process is about to exit — installing is the whole of what it
    // was for — and a child left in our group goes down with the group the
    // moment whatever started us reaps it. The application appeared to
    // start and was gone before anything drew: the same launch by hand
    // worked every time, which is what made it look like the launcher and
    // not the launching.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let _ = cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

/// Remove this application's entry. Removing one that is not installed is
/// not an error: the end state is the one that was asked for.
pub fn uninstall(app_id: &str) -> Result<Vec<PathBuf>, String> {
    let entries = list();
    let Some(mine) = entries.iter().find(|e| e.app_id == app_id) else {
        return Ok(Vec::new());
    };
    let mut gone = Vec::new();
    for f in &mine.files {
        // Only inside what this module writes to. The record is a file in
        // the person's own configuration and could have been edited, by
        // hand or by something else; a path in it is a path this process
        // is about to delete, and "it was in the file" is not a good
        // enough reason.
        if !ours(f) {
            continue;
        }
        if std::fs::remove_file(f).is_ok() || std::fs::remove_dir_all(f).is_ok() {
            gone.push(f.clone());
        }
    }
    let rest: Vec<Entry> = entries.into_iter().filter(|e| e.app_id != app_id).collect();
    save(&rest)?;
    Ok(gone)
}

/// Whether `path` is somewhere this module installs into.
fn ours(path: &Path) -> bool {
    roots().iter().any(|root| path.starts_with(root))
}

/// The directories an entry may be written to on this platform.
fn roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(d) = crate::manifest::config_dir() {
        out.push(d);
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(d) = data_home() {
            out.push(d.join("applications"));
            out.push(d.join("icons"));
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(h) = std::env::var_os("HOME") {
        out.push(PathBuf::from(h).join("Applications"));
    }
    #[cfg(target_os = "windows")]
    if let Some(d) = std::env::var_os("APPDATA") {
        out.push(PathBuf::from(d).join("Microsoft").join("Windows").join("Start Menu").join("Programs"));
    }
    out
}

// ----------------------------------------------------------------- pictures

/// `png` re-encoded at `edge × edge`.
///
/// Every one of the three formats declares the size of the picture it
/// holds, and a declared size that is not the real one is an icon the
/// platform draws wrong or refuses. So the publisher's PNG — which is
/// whatever they drew — is decoded once and written out at the sizes the
/// format asks for.
fn square(png: &[u8], edge: u32) -> Result<Vec<u8>, String> {
    let img = crate::assets::decode_png(png).map_err(|e| format!("the icon is not a PNG this client reads: {e}"))?;
    let img = if img.width == edge && img.height == edge {
        img
    } else {
        // Not `resized` straight to a square: an icon that is not square
        // would be stretched, and the publisher drew a picture and not a
        // rectangle to fill. It is fitted inside the square instead and
        // the rest left transparent.
        let long = img.width.max(img.height);
        if long == 0 {
            return Err("the icon has no pixels".into());
        }
        let nw = (u64::from(img.width) * u64::from(edge) / u64::from(long)).max(1) as u32;
        let nh = (u64::from(img.height) * u64::from(edge) / u64::from(long)).max(1) as u32;
        let small = crate::assets::resized(&img, nw, nh).ok_or_else(|| "the icon could not be resized".to_owned())?;
        centred(&small, edge)
    };
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| format!("cannot write the icon: {e}"))?;
    w.write_image_data(&img.rgba).map_err(|e| format!("cannot write the icon: {e}"))?;
    w.finish().map_err(|e| format!("cannot write the icon: {e}"))?;
    Ok(out)
}

/// `img` in the middle of a transparent `edge × edge` square.
fn centred(img: &crate::assets::Image, edge: u32) -> crate::assets::Image {
    let side = edge as usize;
    let mut rgba = vec![0u8; side.saturating_mul(side).saturating_mul(4)];
    let (w, h) = (img.width as usize, img.height as usize);
    let x0 = side.saturating_sub(w) / 2;
    let y0 = side.saturating_sub(h) / 2;
    for y in 0..h {
        let Some(src) = img.rgba.get(y.saturating_mul(w).saturating_mul(4)..y.saturating_add(1).saturating_mul(w).saturating_mul(4)) else { continue };
        let at = y0.saturating_add(y).saturating_mul(side).saturating_add(x0).saturating_mul(4);
        let Some(dst) = rgba.get_mut(at..at.saturating_add(src.len())) else { continue };
        dst.copy_from_slice(src);
    }
    crate::assets::Image { width: edge, height: edge, rgba }
}

/// An `.icns`: the container macOS reads, which is a header and then one
/// chunk per size. PNG is a legal payload for every type named here, so
/// the pictures go in as they are and nothing has to speak Apple's older
/// packed formats.
///
/// Compiled everywhere, called on macOS. It is byte-shuffling with no
/// platform behind it, and a build machine that cannot run the result can
/// still check that what it produced says what it contains — which is more
/// than an encoder behind a `cfg` nobody here compiles ever gets. Dead on
/// every other platform, and deliberately so.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn icns(png: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    for (kind, edge) in [(*b"ic07", 128u32), (*b"ic08", 256), (*b"ic09", 512)] {
        let one = square(png, edge)?;
        let len = u32::try_from(one.len().saturating_add(8)).map_err(|_| "the icon is too large".to_owned())?;
        body.extend_from_slice(&kind);
        body.extend_from_slice(&len.to_be_bytes());
        body.extend_from_slice(&one);
    }
    let total = u32::try_from(body.len().saturating_add(8)).map_err(|_| "the icon is too large".to_owned())?;
    let mut out = Vec::with_capacity(body.len().saturating_add(8));
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// An `.ico`: a directory of pictures, each a PNG. Windows has taken PNG
/// inside an icon since Vista, so this is a table of contents and the same
/// pictures again rather than five bitmaps and their masks.
///
/// Compiled everywhere, for [`icns`]'s reason.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn ico(png: &[u8]) -> Result<Vec<u8>, String> {
    let edges = [16u32, 32, 48, 256];
    let pictures: Vec<Vec<u8>> = edges.iter().map(|e| square(png, *e)).collect::<Result<_, _>>()?;
    let count = u16::try_from(pictures.len()).map_err(|_| "too many icon sizes".to_owned())?;
    // Six bytes of header, then sixteen per entry; the pictures follow.
    let mut offset = u32::from(count).saturating_mul(16).saturating_add(6);
    let mut dir = Vec::new();
    dir.extend_from_slice(&0u16.to_le_bytes());
    dir.extend_from_slice(&1u16.to_le_bytes());
    dir.extend_from_slice(&count.to_le_bytes());
    for (edge, one) in edges.iter().zip(&pictures) {
        let len = u32::try_from(one.len()).map_err(|_| "the icon is too large".to_owned())?;
        // 256 is written as zero: the field is one byte and 256 does not
        // fit in it, which is the format's own convention and not a trick.
        let side = u8::try_from(*edge).unwrap_or(0);
        dir.push(side);
        dir.push(side);
        dir.push(0); // no colour table
        dir.push(0); // reserved
        dir.extend_from_slice(&1u16.to_le_bytes()); // planes
        dir.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        dir.extend_from_slice(&len.to_le_bytes());
        dir.extend_from_slice(&offset.to_le_bytes());
        offset = offset.saturating_add(len);
    }
    for one in &pictures {
        dir.extend_from_slice(one);
    }
    Ok(dir)
}

// ------------------------------------------------------------------- Linux

/// `$XDG_DATA_HOME`, else `~/.local/share`: where a desktop looks for the
/// applications and icons one person installed.
#[cfg(target_os = "linux")]
fn data_home() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(d));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
}

/// A value in a desktop entry's `Exec`: the quoting that specification
/// asks for, which is not the shell's.
#[cfg(target_os = "linux")]
fn exec_arg(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        // A literal per cent is doubled; a single one introduces a field
        // code and would silently eat the character after it.
        if c == '%' {
            out.push('%');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// A `.desktop` file and a PNG beside it.
///
/// `Icon` is an absolute path rather than a name in the icon theme. The
/// theme would be the tidier answer and it is the wrong one here: hicolor
/// only looks in the directories its own `index.theme` names, so a
/// publisher's picture at whatever size they drew it would have to be
/// resized into a fixed set of buckets to be found at all. An absolute
/// path is read by every launcher, taskbar and switcher, at the size each
/// one wants.
#[cfg(target_os = "linux")]
fn write_entry(app: &App, slug: &str) -> Result<Vec<PathBuf>, String> {
    let home = data_home().ok_or_else(|| "no XDG data directory to install into".to_owned())?;
    let exe = me()?;
    let icon_at = home.join("icons").join("eui").join(format!("{slug}.png"));
    put(&icon_at, &square(&app.icon, 512)?)?;
    let name = app.name.trim();
    let name = if name.is_empty() { slug } else { name };
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Comment=An EUI application\n\
         Exec={exe} {url}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Network;\n\
         StartupWMClass=eui\n\
         X-EUI-AppId={app_id}\n",
        exe = exec_arg(&exe.to_string_lossy()),
        url = exec_arg(&app.url),
        icon = icon_at.to_string_lossy(),
        app_id = app.app_id,
    );
    let desktop_at = home.join("applications").join(format!("eui-{slug}.desktop"));
    put(&desktop_at, entry.as_bytes())?;
    // Best effort, and silent: every desktop in current use watches the
    // directory, and the ones that do not are the ones where a person
    // logs out anyway.
    let _ = std::process::Command::new("update-desktop-database").arg(home.join("applications")).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    // The entry first: it is what `launch` starts, and what the icon is for.
    Ok(vec![desktop_at, icon_at])
}

// ------------------------------------------------------------------- macOS

/// A `.app` in `~/Applications`: a property list, an icon, and a stub that
/// becomes the client.
///
/// `exec` and not a launch: the stub replaces itself with this binary, so
/// the process the Dock is watching *is* the window. A wrapper that
/// started a child and waited would put two things in the Dock and leave
/// the wrong one holding the icon.
///
/// Nothing is signed. A bundle assembled here by the person who will run
/// it never passes through anything that sets `com.apple.quarantine`, so
/// there is no Gatekeeper hold to get past — which is the whole reason
/// `scripts/install-macos.sh` exists for the client itself.
#[cfg(target_os = "macos")]
fn write_entry(app: &App, slug: &str) -> Result<Vec<PathBuf>, String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "no home directory to install into".to_owned())?;
    let exe = me()?;
    let name = app.name.trim();
    let name = if name.is_empty() { slug } else { name };
    // The bundle is named for the application, because that name is what
    // Finder, Spotlight and the Dock all show; everything inside it is
    // named for the slug, where a stray character would be a broken path
    // rather than an odd-looking label.
    let bundle = PathBuf::from(home).join("Applications").join(format!("{}.app", name.replace('/', "-")));
    let contents = bundle.join("Contents");
    put(&contents.join("Resources").join(format!("{slug}.icns")), &icns(&app.icon)?)?;
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20   <key>CFBundleName</key><string>{name}</string>\n\
         \x20   <key>CFBundleDisplayName</key><string>{name}</string>\n\
         \x20   <key>CFBundleIdentifier</key><string>net.eui.app.{slug}</string>\n\
         \x20   <key>CFBundleExecutable</key><string>{slug}</string>\n\
         \x20   <key>CFBundleIconFile</key><string>{slug}</string>\n\
         \x20   <key>CFBundlePackageType</key><string>APPL</string>\n\
         \x20   <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>\n\
         \x20   <key>CFBundleShortVersionString</key><string>1.0</string>\n\
         \x20   <key>CFBundleVersion</key><string>1</string>\n\
         \x20   <key>LSMinimumSystemVersion</key><string>11.0</string>\n\
         \x20   <key>NSHighResolutionCapable</key><true/>\n\
         \x20   <key>NSPrincipalClass</key><string>NSApplication</string>\n\
         </dict>\n\
         </plist>\n",
        name = xml(name),
        slug = slug,
    );
    put(&contents.join("Info.plist"), plist.as_bytes())?;
    let stub_at = contents.join("MacOS").join(slug);
    let stub = format!("#!/bin/sh\nexec {exe} {url}\n", exe = sh(&exe.to_string_lossy()), url = sh(&app.url));
    put(&stub_at, stub.as_bytes())?;
    executable(&stub_at)?;

    // Two calls the platform wants, both best effort and both silent —
    // the same shape as `update-desktop-database` on Linux, and for the
    // same reason: an entry the desktop has not noticed is an entry that
    // is not there as far as anybody looking for it is concerned.
    //
    // Signing first. `scripts/wrap-macos-app.sh` says why in more detail
    // than fits here: assembling a bundle by writing files into it leaves
    // it unsigned, and Apple Silicon refuses to run an unsigned bundle
    // outright — reporting it as "damaged", which sends somebody looking
    // for a corrupt download. Ad hoc is enough here and notarisation is
    // not wanted: nothing downloaded this, so nothing wrote
    // `com.apple.quarantine` on it and there is no Gatekeeper hold to
    // clear.
    let _ = std::process::Command::new("codesign").args(["--force", "--sign", "-"]).arg(&bundle).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    // And then Launch Services, which is what Spotlight and Launchpad ask.
    // `~/Applications` is a directory macOS honours and does not create,
    // so on most machines this one is making it for the first time — and a
    // directory that did not exist a moment ago is one nothing has
    // indexed. Finder shows the bundle either way; without this, searching
    // for it does not.
    let _ = std::process::Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
        .arg("-f")
        .arg(&bundle)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    // One path recorded, not four: removing an application means removing
    // its bundle, and a bundle is a directory.
    Ok(vec![bundle])
}

/// `s` as XML character data.
#[cfg(target_os = "macos")]
fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `s` as one single-quoted word for `/bin/sh`.
#[cfg(target_os = "macos")]
fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Mark `path` executable, which is what makes the stub a bundle's binary
/// rather than a text file inside one.
#[cfg(target_os = "macos")]
fn executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{}: {e}", path.display()))
}

// ----------------------------------------------------------------- Windows

/// A shortcut in the Start menu, and an `.ico` beside it.
///
/// The `.lnk` is made by the shell rather than written here. Its format is
/// documented and could be assembled by hand, and that is exactly the kind
/// of binary structure that is right in nine fields and wrong in the
/// tenth, on a platform this client cannot try it on. `WScript.Shell` is
/// what every installer on Windows uses, ships with the system, and is the
/// component whose job this is.
#[cfg(target_os = "windows")]
fn write_entry(app: &App, slug: &str) -> Result<Vec<PathBuf>, String> {
    let appdata = std::env::var_os("APPDATA").ok_or_else(|| "no %APPDATA% to install into".to_owned())?;
    let exe = me()?;
    let name = app.name.trim();
    let name = if name.is_empty() { slug } else { name };
    let icon_at = PathBuf::from(&appdata).join("eui").join("icons").join(format!("{slug}.ico"));
    put(&icon_at, &ico(&app.icon)?)?;
    let programs = PathBuf::from(&appdata).join("Microsoft").join("Windows").join("Start Menu").join("Programs");
    let link_at = programs.join(format!("{}.lnk", name.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "-")));
    std::fs::create_dir_all(&programs).map_err(|e| format!("{}: {e}", programs.display()))?;
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({link}); \
         $s.TargetPath = {exe}; \
         $s.Arguments = {url}; \
         $s.IconLocation = {icon}; \
         $s.Description = 'An EUI application'; \
         $s.Save()",
        link = ps(&link_at.to_string_lossy()),
        exe = ps(&exe.to_string_lossy()),
        url = ps(&app.url),
        icon = ps(&icon_at.to_string_lossy()),
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|e| format!("cannot run powershell to make the shortcut: {e}"))?;
    if !out.status.success() {
        return Err(format!("the shortcut was refused: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    // The shortcut first, for the reason the Linux arm gives.
    Ok(vec![link_at, icon_at])
}

/// `s` as one single-quoted PowerShell string.
#[cfg(target_os = "windows")]
fn ps(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

// --------------------------------------------------------- everywhere else

/// A platform with no launcher to write to. Both phones start applications
/// their own way and a page is already in one.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn write_entry(_app: &App, _slug: &str) -> Result<Vec<PathBuf>, String> {
    Err("this platform has nowhere to install an application to".into())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

    use super::*;

    /// A one-pixel PNG, for the paths that only care that it decodes.
    fn dot() -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 1, 1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[9, 9, 9, 255]).unwrap();
        w.finish().unwrap();
        out
    }

    #[test]
    fn an_app_id_becomes_a_file_name_or_nothing() {
        assert_eq!(slug("counter-app").as_deref(), Some("counter-app"));
        assert_eq!(slug("Counter.App").as_deref(), Some("counter.app"));
        // A path is not a name: every separator goes, and the run of them
        // collapses rather than leaving a file called `a---b`.
        assert_eq!(slug("../../etc/passwd").as_deref(), Some("etc-passwd"));
        assert_eq!(slug("a b").as_deref(), Some("a-b"));
        // Nothing usable in it at all is refused rather than guessed at.
        assert_eq!(slug(""), None);
        assert_eq!(slug("///"), None);
        assert_eq!(slug("..."), None);
        assert!(slug(&"x".repeat(500)).unwrap().len() <= 96);
    }

    #[test]
    fn an_icon_is_written_at_the_size_the_format_declares() {
        let png = square(&dot(), 256).unwrap();
        let img = crate::assets::decode_png(&png).unwrap();
        assert_eq!((img.width, img.height), (256, 256));
        // A picture that is not square is fitted inside one, not stretched.
        let mut wide = Vec::new();
        let mut enc = png::Encoder::new(&mut wide, 4, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[255; 4 * 2 * 4]).unwrap();
        w.finish().unwrap();
        let img = crate::assets::decode_png(&square(&wide, 64).unwrap()).unwrap();
        assert_eq!((img.width, img.height), (64, 64));
        // The top row is outside the picture and stayed transparent.
        assert_eq!(img.rgba[3], 0, "fitted, not stretched");
    }

    #[test]
    fn the_record_round_trips_and_survives_a_torn_line() {
        let dir = std::env::temp_dir().join(format!("eui-install-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let at = dir.join("installed");
        assert!(load_from(&at).is_empty(), "no file is an empty list, not a failure");
        let one = Entry { app_id: "counter-app".into(), name: "Counter".into(), url: "wss://demo.example/_eui/session".into(), files: vec![dir.join("a.desktop"), dir.join("a.png")] };
        save_to(&at, std::slice::from_ref(&one)).unwrap();
        assert_eq!(load_from(&at), vec![one.clone()]);
        // A half-written line costs its own entry and nothing else.
        let mut text = std::fs::read_to_string(&at).unwrap();
        text.push_str("truncated\n");
        std::fs::write(&at, &text).unwrap();
        assert_eq!(load_from(&at), vec![one]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_what_this_module_writes_is_ever_deleted() {
        // The record is a file in the person's own configuration and could
        // have been edited. A path in it is a path `uninstall` is about to
        // remove, so being in the file is not sufficient reason.
        assert!(!ours(Path::new("/etc/passwd")));
        assert!(!ours(Path::new("/")));
        let inside = roots().first().map(|r| r.join("eui-demo.desktop"));
        assert!(inside.is_none_or(|p| ours(&p)), "what we write, we may remove");
    }

    /// `.icns` and `.ico` are byte formats nobody on this build machine can
    /// open, so what is checked is that each says what it contains: the
    /// magic, the lengths that have to add up, and a picture at every
    /// declared size that really is a PNG of that size. A declared size
    /// that is not the real one is an icon the platform draws wrong or
    /// refuses, and it is the one mistake neither of them reports.
    #[test]
    fn the_two_icon_containers_declare_what_they_hold() {
        let png = dot();

        let icns = icns(&png).unwrap();
        assert_eq!(icns.get(..4), Some(b"icns".as_slice()));
        let total = u32::from_be_bytes([icns[4], icns[5], icns[6], icns[7]]) as usize;
        assert_eq!(total, icns.len(), "the header's length is the file's");
        let mut at = 8;
        let mut seen = Vec::new();
        while at < icns.len() {
            let kind = icns[at..at + 4].to_vec();
            let len = u32::from_be_bytes([icns[at + 4], icns[at + 5], icns[at + 6], icns[at + 7]]) as usize;
            assert!(len >= 8 && at + len <= icns.len(), "chunk {kind:?} runs past the end");
            let img = crate::assets::decode_png(&icns[at + 8..at + len]).unwrap();
            assert_eq!(img.width, img.height, "an icon entry is square");
            seen.push((kind, img.width));
            at += len;
        }
        assert_eq!(seen.iter().map(|(_, w)| *w).collect::<Vec<_>>(), vec![128, 256, 512]);
        assert_eq!(seen.first().map(|(k, _)| k.as_slice()), Some(b"ic07".as_slice()), "128 is ic07");

        let ico = ico(&png).unwrap();
        assert_eq!(u16::from_le_bytes([ico[0], ico[1]]), 0);
        assert_eq!(u16::from_le_bytes([ico[2], ico[3]]), 1, "an icon, not a cursor");
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        assert_eq!(count, 4);
        for i in 0..count {
            let e = 6 + i * 16;
            let declared = ico[e];
            let len = u32::from_le_bytes([ico[e + 8], ico[e + 9], ico[e + 10], ico[e + 11]]) as usize;
            let off = u32::from_le_bytes([ico[e + 12], ico[e + 13], ico[e + 14], ico[e + 15]]) as usize;
            assert_eq!(ico[e], ico[e + 1], "square");
            assert_eq!(u16::from_le_bytes([ico[e + 6], ico[e + 7]]), 32, "32 bits per pixel");
            assert!(off + len <= ico.len(), "entry {i} points past the end");
            let img = crate::assets::decode_png(&ico[off..off + len]).unwrap();
            // 256 is written as zero: the field is one byte and 256 does
            // not fit in it, which is the format's own convention.
            let expected = if declared == 0 { 256 } else { u32::from(declared) };
            assert_eq!(img.width, expected, "entry {i} says {declared} and holds {}", img.width);
        }
    }

    #[test]
    fn installing_needs_a_usable_app_id() {
        let bad = App { app_id: "///".into(), name: "Demo".into(), url: "wss://x/s".into(), icon: dot() };
        assert!(install(&bad).is_err(), "nothing is written under a name that is not one");
    }
}

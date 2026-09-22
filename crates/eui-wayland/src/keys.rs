//! The keymap, asked of the compositor on winit's own connection.
//!
//! winit binds a `wl_keyboard` and keeps everything it is sent — the keymap
//! above all — to itself. This asks the same seat for a second one, on a
//! second queue of the same connection (the shape of `dnd.rs`, for the same
//! reason: a keymap is delivered to a `wl_keyboard`, and a `wl_keyboard`
//! belongs to a seat this client can only reach as itself). From it come
//! the keymap, the locks and the layout group, and whether the keyboard is
//! on this window at all. **The keys themselves are not read here.** They
//! reach the window through winit, in order, and the window feeds each one
//! to the [`xkb::State`] kept per keyboard — that is what keeps a Shift
//! pressed a moment before a `1` in force for it, whatever the compositor
//! says about its modifiers afterwards (see `xkb.rs`).
//!
//! No thread. The compositor's events for this queue are read off the
//! socket by winit's own reads and wait here until [`Keys::typed`]
//! dispatches them, which it does before every answer. A key event winit
//! is delivering was read in the same batch as the keymap that explains
//! it, so by the time the window asks, the answer is already in the queue.

use std::collections::HashMap;
use std::os::unix::fs::FileExt;

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::protocol::{wl_keyboard, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};

use crate::xkb;

/// xkb keycodes are evdev scancodes plus eight, and always have been.
const XKB_OFFSET: u32 = 8;

/// A keymap the compositor might send that is not a keymap. Anything past
/// this is not the text of one.
const MAX_KEYMAP: u32 = 4 * 1024 * 1024;

/// One window's keyboards, and the state this window keeps for each.
pub struct Keys {
    conn: Connection,
    queue: EventQueue<Board>,
    board: Board,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keys").field("keyboards", &self.board.states.len()).finish_non_exhaustive()
    }
}

impl Keys {
    /// Ask the seats of the connection `display` is running for their
    /// keymaps.
    ///
    /// `None` when there is no libwayland, no seat with a keyboard, no
    /// keymap this process's `libxkbcommon` can compile, or no
    /// `libxkbcommon` at all — a window that names its keys the way it
    /// did before, from what winit says was typed.
    ///
    /// # Safety
    ///
    /// `display` must be a live `wl_display` belonging to this process's
    /// own window, and must outlive the returned `Keys`. Both hold at the
    /// one call site: the pointer comes out of winit's window handle, and
    /// the `Keys` lives in the shell that owns the window and goes when it
    /// closes.
    #[must_use]
    pub fn start(display: *mut core::ffi::c_void) -> Option<Self> {
        if display.is_null() {
            return None;
        }
        // SAFETY: the caller's contract, above.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let mut queue = conn.new_event_queue::<Board>();
        let qh = queue.handle();
        let _registry = conn.display().get_registry(&qh, ());
        let mut board = Board { seats: HashMap::new(), states: HashMap::new(), focused: None, locale: locale() };
        // Three roundtrips: the globals; a seat's capabilities, and so its
        // keyboard; the keyboard's keymap.
        for _ in 0..3 {
            if queue.roundtrip(&mut board).is_err() {
                board.release();
                let _ = conn.flush();
                return None;
            }
        }
        if !board.states.values().any(Option::is_some) {
            board.release();
            let _ = conn.flush();
            return None;
        }
        Some(Self { conn, queue, board })
    }

    /// What the key with evdev scancode `scancode` types, now, and the
    /// state moved past it.
    ///
    /// Called for **every** key event the window receives, modifier keys
    /// included — a Shift the state never saw pressed is a Shift it will
    /// never apply. `None` when no keyboard has a keymap yet, or the
    /// keycode is out of range; `Some("")` for a key that types nothing,
    /// which is every modifier and every release.
    pub fn typed(&mut self, scancode: u32, down: bool, repeat: bool) -> Option<String> {
        // Whatever the compositor said since last time — a keymap, a lock,
        // a layout switch, the keyboard leaving — before the key is read
        // against it.
        let _ = self.queue.dispatch_pending(&mut self.board);
        let _ = self.conn.flush();
        let keycode = scancode.checked_add(XKB_OFFSET)?;
        let state = self.board.current()?;
        Some(state.typed(keycode, down, repeat))
    }

    /// The modifiers the current keyboard's state has in force, as
    /// xkbcommon's mask: bit 0 Shift, bit 1 Lock, bit 2 Control, bit 3
    /// Mod1 (Alt). For the trace, so a disagreement with what the
    /// compositor reports is a line somebody can read.
    #[must_use]
    pub fn mods(&mut self) -> Option<u32> {
        self.board.current().map(|s| s.effective_mods())
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        self.board.release();
        let _ = self.conn.flush();
    }
}

/// The locale the compose table is read for, the way every toolkit reads
/// it: `LC_ALL`, then `LC_CTYPE`, then `LANG`, then `C`.
fn locale() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"].iter().filter_map(std::env::var_os).map(|v| v.to_string_lossy().into_owned()).find(|v| !v.is_empty()).unwrap_or_else(|| "C".into())
}

/// A seat, and the keyboard asked of it once it said it had one.
struct Seat {
    seat: wl_seat::WlSeat,
    keyboard: Option<wl_keyboard::WlKeyboard>,
}

/// Everything the queue keeps.
struct Board {
    /// The seats, by registry name so one going away can take its
    /// keyboard with it.
    seats: HashMap<u32, Seat>,
    /// One state per keyboard, by the keyboard's object id, made when its
    /// keymap arrives. `None` for a keymap that would not compile: a
    /// keyboard this window does not resolve keys for.
    states: HashMap<ObjectId, Option<xkb::State>>,
    /// The keyboard that last entered this client's surfaces.
    focused: Option<ObjectId>,
    locale: String,
}

impl Board {
    /// The state the keys are read against: the focused keyboard's, or —
    /// before any `enter`, which a compositor may not send to a keyboard
    /// bound after the surface was already focused — the only one there is.
    fn current(&mut self) -> Option<&mut xkb::State> {
        let id = match &self.focused {
            Some(id) => id.clone(),
            None => {
                let mut with = self.states.iter().filter(|(_, s)| s.is_some()).map(|(id, _)| id.clone());
                let one = with.next()?;
                if with.next().is_some() {
                    return None;
                }
                one
            }
        };
        self.states.get_mut(&id)?.as_mut()
    }

    /// Give every keyboard and seat back, where the versions have a way to.
    fn release(&mut self) {
        for s in self.seats.values_mut() {
            if let Some(k) = s.keyboard.take() {
                if k.version() >= 3 {
                    k.release();
                }
                self.states.remove(&k.id());
            }
            if s.seat.version() >= 5 {
                s.seat.release();
            }
        }
        self.seats.clear();
        self.focused = None;
    }

    /// The keymap `fd` carries, as text: `size` bytes, the trailing NUL
    /// dropped.
    fn keymap_text(fd: &std::fs::File, size: u32) -> Option<String> {
        if size == 0 || size > MAX_KEYMAP {
            return None;
        }
        let mut buf = vec![0_u8; usize::try_from(size).ok()?];
        // `read_exact_at`, not `read_to_string`: the descriptor's offset is
        // wherever the compositor left it, and a keymap is read from 0.
        fd.read_exact_at(&mut buf, 0).ok()?;
        while buf.last() == Some(&0) {
            buf.pop();
        }
        String::from_utf8(buf).ok()
    }
}

// ------------------------------------------------------------- dispatch

impl Dispatch<wl_registry::WlRegistry, ()> for Board {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, (): &(), _conn: &Connection, qh: &QueueHandle<Self>) {
        match event {
            // Every seat, not the first: the one a person types on is
            // whichever's keyboard enters the surface.
            wl_registry::Event::Global { name, interface, version } if interface == "wl_seat" => {
                // 5 has `release`; nothing later is used, and every version
                // bound is another request that can be got wrong.
                let want = version.min(5);
                let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, want, qh, ());
                state.seats.insert(name, Seat { seat, keyboard: None });
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(mut s) = state.seats.remove(&name) {
                    if let Some(k) = s.keyboard.take() {
                        if k.version() >= 3 {
                            k.release();
                        }
                        state.states.remove(&k.id());
                        if state.focused.as_ref() == Some(&k.id()) {
                            state.focused = None;
                        }
                    }
                    if s.seat.version() >= 5 {
                        s.seat.release();
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Board {
    fn event(state: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event, (): &(), _conn: &Connection, qh: &QueueHandle<Self>) {
        let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event else { return };
        let Some(s) = state.seats.values_mut().find(|s| s.seat.id() == seat.id()) else { return };
        let has = caps.contains(wl_seat::Capability::Keyboard);
        match (has, s.keyboard.is_some()) {
            (true, false) => s.keyboard = Some(seat.get_keyboard(qh, ())),
            (false, true) => {
                if let Some(k) = s.keyboard.take() {
                    if k.version() >= 3 {
                        k.release();
                    }
                    state.states.remove(&k.id());
                    if state.focused.as_ref() == Some(&k.id()) {
                        state.focused = None;
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Board {
    fn event(state: &mut Self, keyboard: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, (): &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                let compiled = match format {
                    WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) => {
                        let file = std::fs::File::from(fd);
                        Self::keymap_text(&file, size).and_then(|text| xkb::Keymap::from_text(&text)).and_then(|map| xkb::State::new(map, &state.locale))
                    }
                    // `no_keymap`, or a format from after this was written.
                    _ => None,
                };
                if compiled.is_none() {
                    eprintln!("eui: the compositor's keymap could not be read; keys are named from what winit says was typed");
                }
                state.states.insert(keyboard.id(), compiled);
            }
            // Focus came here. What was held while it was elsewhere was
            // released elsewhere: start from nothing pressed. The
            // `modifiers` that follows brings the locks and the group.
            wl_keyboard::Event::Enter { .. } => {
                state.focused = Some(keyboard.id());
                if let Some(Some(s)) = state.states.get_mut(&keyboard.id()) {
                    s.release_all();
                }
            }
            wl_keyboard::Event::Leave { .. } => {
                if state.focused.as_ref() == Some(&keyboard.id()) {
                    state.focused = None;
                }
                if let Some(Some(s)) = state.states.get_mut(&keyboard.id()) {
                    s.release_all();
                }
            }
            // The locks and the group, and not what is pressed: the latter
            // is what the keys already said, and what this event has been
            // seen to get wrong.
            wl_keyboard::Event::Modifiers { mods_locked, group, .. } => {
                if let Some(Some(s)) = state.states.get_mut(&keyboard.id()) {
                    s.locks(mods_locked, group);
                }
            }
            // The keys come through winit; a second reading of the same
            // press would press it twice. Repeat is winit's too.
            wl_keyboard::Event::Key { .. } | wl_keyboard::Event::RepeatInfo { .. } => {}
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn a_keymap_is_its_bytes_without_the_nul_the_compositor_appends() {
        let dir = std::env::temp_dir().join(format!("eui-keymap-{}", std::process::id()));
        std::fs::write(&dir, b"xkb_keymap { }\0").expect("a temp file");
        let file = std::fs::File::open(&dir).expect("it opens");
        assert_eq!(Board::keymap_text(&file, 15).as_deref(), Some("xkb_keymap { }"));
        // A size past the file is a keymap that is not there.
        assert_eq!(Board::keymap_text(&file, 16), None);
        assert_eq!(Board::keymap_text(&file, 0), None);
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn the_locale_is_c_when_nothing_says_otherwise() {
        // Only the fallback is pinned: the environment of the test runner
        // is its own.
        assert!(!locale().is_empty());
    }
}

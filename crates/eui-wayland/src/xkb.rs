//! xkbcommon, through the same `dlopen` winit already made.
//!
//! A keymap says what each key types at each level; a state says which
//! level is in force — Shift held, Caps locked, AltGr down, which layout.
//! winit keeps a state of its own and fills a key event's `text` from it,
//! but it only ever moves that state on `wl_keyboard.modifiers`, and under
//! Hyprland that event arrives *after* the key it applies to often enough
//! that a fast `Shift`+`1` goes out named `1`. The state here is moved by
//! the **keys** instead: xkbcommon treats a modifier key as a key like any
//! other (`xkb_state_update_key` on `Shift_L` sets Shift), and the keys
//! arrive in the order they were pressed, whatever the compositor says
//! about them afterwards. Only the locks and the layout group — Caps Lock,
//! Num Lock, a layout switch — are taken from `modifiers`, because those
//! a compositor may change without any key this window saw.
//!
//! `xkbcommon-dl` is winit's own binding: the library is dlopen'd, so a
//! machine without it starts the window and simply answers `None` here,
//! and nothing new is linked. The `unsafe` is the FFI and nothing else;
//! every pointer is null-checked at birth and freed in `Drop`.

use std::ffi::{c_char, CString};

use xkbcommon_dl as x;
use xkbcommon_dl::{xkb_compose_feed_result, xkb_compose_status, xkb_key_direction, xkb_state_component};

/// The most bytes one key can type. A keysym is one character, at most
/// four bytes of UTF-8; a composed sequence is one character too.
const ONE_KEY: usize = 32;

/// A compiled keymap: the compositor's, as the text it sent.
pub(crate) struct Keymap {
    ctx: *mut x::xkb_context,
    map: *mut x::xkb_keymap,
}

impl Keymap {
    /// Compile a keymap from its `xkb_keymap { … }` text.
    ///
    /// `None` when `libxkbcommon` cannot be loaded, or when the text does
    /// not compile — a compositor that sent something this library cannot
    /// read, which is a window that types the way it did before.
    pub(crate) fn from_text(text: &str) -> Option<Self> {
        let lib = x::xkbcommon_option()?;
        let text = CString::new(text).ok()?;
        // SAFETY: plain FFI on a library that loaded. The result is checked
        // for null before it is used or kept.
        let ctx = unsafe { (lib.xkb_context_new)(x::xkb_context_flags::XKB_CONTEXT_NO_FLAGS) };
        if ctx.is_null() {
            return None;
        }
        // SAFETY: `ctx` is live and `text` is a NUL-terminated string that
        // outlives the call.
        let map = unsafe { (lib.xkb_keymap_new_from_string)(ctx, text.as_ptr(), x::xkb_keymap_format::XKB_KEYMAP_FORMAT_TEXT_V1, x::xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS) };
        if map.is_null() {
            // SAFETY: `ctx` was made above and is given up here, once.
            unsafe { (lib.xkb_context_unref)(ctx) };
            return None;
        }
        Some(Self { ctx, map })
    }
}

impl Drop for Keymap {
    fn drop(&mut self) {
        if let Some(lib) = x::xkbcommon_option() {
            // SAFETY: both were made in `from_text` and are unreferenced
            // once, here, map before context.
            unsafe {
                (lib.xkb_keymap_unref)(self.map);
                (lib.xkb_context_unref)(self.ctx);
            }
        }
    }
}

/// Dead keys: `^` then `e` is `ê`. The table is the locale's, the same
/// one winit reads, so a window resolving its own keys composes the same
/// way the one before it did.
struct Compose {
    table: *mut x::xkb_compose_table,
    state: *mut x::xkb_compose_state,
}

impl Compose {
    fn new(ctx: *mut x::xkb_context, locale: &str) -> Option<Self> {
        let lib = x::xkbcommon_compose_option()?;
        let locale = CString::new(locale).ok()?;
        // SAFETY: `ctx` is a live context and `locale` a NUL-terminated
        // string; null is checked.
        let table = unsafe { (lib.xkb_compose_table_new_from_locale)(ctx, locale.as_ptr(), x::xkb_compose_compile_flags::XKB_COMPOSE_COMPILE_NO_FLAGS) };
        if table.is_null() {
            return None;
        }
        // SAFETY: `table` is live; null is checked.
        let state = unsafe { (lib.xkb_compose_state_new)(table, x::xkb_compose_state_flags::XKB_COMPOSE_STATE_NO_FLAGS) };
        if state.is_null() {
            // SAFETY: made above, given up once.
            unsafe { (lib.xkb_compose_table_unref)(table) };
            return None;
        }
        Some(Self { table, state })
    }

    /// Feed one keysym. `None` when the sequence is still going or was
    /// abandoned — nothing is typed by this key — and `Some` with the
    /// character when one just completed; `Some(plain)` when the key was
    /// not part of any sequence.
    fn feed(&mut self, keysym: u32, plain: String) -> Option<String> {
        let lib = x::xkbcommon_compose_option()?;
        // SAFETY: `state` is live for as long as `self` is.
        let fed = unsafe { (lib.xkb_compose_state_feed)(self.state, keysym) };
        if fed == xkb_compose_feed_result::XKB_COMPOSE_FEED_IGNORED {
            // A modifier, or a keysym compose does not look at: the
            // sequence, if any, is untouched, and so is what the key types.
            return Some(plain);
        }
        // SAFETY: as above.
        match unsafe { (lib.xkb_compose_state_get_status)(self.state) } {
            xkb_compose_status::XKB_COMPOSE_NOTHING => Some(plain),
            xkb_compose_status::XKB_COMPOSE_COMPOSING | xkb_compose_status::XKB_COMPOSE_CANCELLED => None,
            xkb_compose_status::XKB_COMPOSE_COMPOSED => {
                let mut buf = [0_u8; ONE_KEY];
                // SAFETY: the buffer is `ONE_KEY` bytes and the length says
                // so; xkbcommon writes at most that, NUL included.
                let n = unsafe { (lib.xkb_compose_state_get_utf8)(self.state, buf.as_mut_ptr().cast::<c_char>(), buf.len()) };
                Some(text_of(&buf, n))
            }
        }
    }
}

impl Drop for Compose {
    fn drop(&mut self) {
        if let Some(lib) = x::xkbcommon_compose_option() {
            // SAFETY: both made in `new`, unreferenced once, state first.
            unsafe {
                (lib.xkb_compose_state_unref)(self.state);
                (lib.xkb_compose_table_unref)(self.table);
            }
        }
    }
}

/// What xkbcommon wrote into `buf`: `n` bytes, NUL not counted, or nothing.
fn text_of(buf: &[u8], n: i32) -> String {
    let Ok(n) = usize::try_from(n) else { return String::new() };
    // One byte short: the last is the NUL when the text filled the buffer.
    let n = n.min(buf.len().saturating_sub(1));
    buf.get(..n).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default()
}

/// The keyboard as this window understands it: a keymap, and which level
/// of it the keys pressed so far have put it on.
pub(crate) struct State {
    st: *mut x::xkb_state,
    compose: Option<Compose>,
    // Dropped after `st`: the state holds a reference to the map.
    map: Keymap,
}

// SAFETY: xkbcommon objects are not thread-safe, and this type is not
// `Sync`; but nothing in one is tied to the thread that made it, so the
// whole may move to another thread and be used there alone.
unsafe impl Send for State {}

impl State {
    /// A fresh state on `map`: nothing pressed, nothing locked, group 0.
    /// `locale` is for the compose table; `"C"` when nothing better is known.
    pub(crate) fn new(map: Keymap, locale: &str) -> Option<Self> {
        let lib = x::xkbcommon_option()?;
        // SAFETY: `map.map` is live; null is checked.
        let st = unsafe { (lib.xkb_state_new)(map.map) };
        if st.is_null() {
            return None;
        }
        let compose = Compose::new(map.ctx, locale);
        Some(Self { st, compose, map })
    }

    /// What `keycode` types with the state as it is — that is, with every
    /// key pressed *before* it applied and itself not yet.
    pub(crate) fn utf8(&self, keycode: u32) -> String {
        let Some(lib) = x::xkbcommon_option() else { return String::new() };
        let mut buf = [0_u8; ONE_KEY];
        // SAFETY: `st` is live; the buffer is as long as the length says.
        let n = unsafe { (lib.xkb_state_key_get_utf8)(self.st, keycode, buf.as_mut_ptr().cast::<c_char>(), buf.len()) };
        text_of(&buf, n)
    }

    /// Apply a press or a release. A modifier key moves the level for the
    /// keys after it; that is the whole reason this state exists.
    pub(crate) fn key(&mut self, keycode: u32, down: bool) {
        let Some(lib) = x::xkbcommon_option() else { return };
        let dir = if down { xkb_key_direction::XKB_KEY_DOWN } else { xkb_key_direction::XKB_KEY_UP };
        // SAFETY: `st` is live.
        let _ = unsafe { (lib.xkb_state_update_key)(self.st, keycode, dir) };
    }

    /// The character `keycode` types, and the state moved past it.
    ///
    /// On a press the keysym goes through the compose table first, so a
    /// dead key types nothing and the key after it types the composed
    /// character. A repeat reads the state and moves nothing: the key is
    /// already down.
    pub(crate) fn typed(&mut self, keycode: u32, down: bool, repeat: bool) -> String {
        let plain = self.utf8(keycode);
        let text = if down && !repeat {
            let sym = self.keysym(keycode);
            match (self.compose.as_mut(), sym) {
                (Some(c), Some(sym)) => c.feed(sym, plain).unwrap_or_default(),
                _ => plain,
            }
        } else {
            plain
        };
        if !repeat {
            self.key(keycode, down);
        }
        text
    }

    /// The one keysym `keycode` stands for now, or `None` when it has
    /// several or none — neither is anything compose can be fed.
    fn keysym(&self, keycode: u32) -> Option<u32> {
        let lib = x::xkbcommon_option()?;
        // SAFETY: `st` is live.
        let sym = unsafe { (lib.xkb_state_key_get_one_sym)(self.st, keycode) };
        (sym != 0).then_some(sym)
    }

    /// Take the compositor's word for the locks and the layout group, and
    /// **only** those: what is pressed stays what this window saw pressed.
    pub(crate) fn locks(&mut self, locked: u32, group: u32) {
        let Some(lib) = x::xkbcommon_option() else { return };
        // SAFETY: `st` is live throughout.
        unsafe {
            let depressed = (lib.xkb_state_serialize_mods)(self.st, xkb_state_component::XKB_STATE_MODS_DEPRESSED);
            let latched = (lib.xkb_state_serialize_mods)(self.st, xkb_state_component::XKB_STATE_MODS_LATCHED);
            let _ = (lib.xkb_state_update_mask)(self.st, depressed, latched, locked, 0, 0, group);
        }
    }

    /// Nothing is pressed any more. The keyboard went to another window,
    /// and the releases with it; a Shift still down here would shift every
    /// key typed after the focus comes back.
    pub(crate) fn release_all(&mut self) {
        let Some(lib) = x::xkbcommon_option() else { return };
        // SAFETY: `st` is live throughout.
        unsafe {
            let locked = (lib.xkb_state_serialize_mods)(self.st, xkb_state_component::XKB_STATE_MODS_LOCKED);
            let group = (lib.xkb_state_serialize_layout)(self.st, xkb_state_component::XKB_STATE_LAYOUT_LOCKED);
            let _ = (lib.xkb_state_update_mask)(self.st, 0, 0, locked, 0, 0, group);
        }
        if let Some(c) = self.compose.as_mut() {
            if let Some(lib) = x::xkbcommon_compose_option() {
                // SAFETY: `c.state` is live.
                unsafe { (lib.xkb_compose_state_reset)(c.state) };
            }
        }
    }

    /// The modifiers in force, as xkbcommon's mask. For the trace.
    pub(crate) fn effective_mods(&self) -> u32 {
        let Some(lib) = x::xkbcommon_option() else { return 0 };
        // SAFETY: `st` is live.
        unsafe { (lib.xkb_state_serialize_mods)(self.st, xkb_state_component::XKB_STATE_MODS_EFFECTIVE) }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(lib) = x::xkbcommon_option() {
            // SAFETY: made in `new`, unreferenced once, before the map it
            // references is (field order).
            unsafe { (lib.xkb_state_unref)(self.st) };
        }
        let _ = &self.map;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five keys, two layouts, and nothing read from disk: `<AE01>` is the
    /// `1` key, `<AC01>` the `a` key, `<LFSH>` Shift, `<CAPS>` Caps Lock and
    /// `<RALT>` AltGr. Group 1 is a US row (`1` / `!`), group 2 a French
    /// one (`&` / `1` / `|` with AltGr). Checked with `xkbcli compile-keymap`.
    const MAP: &str = r#"xkb_keymap {
 xkb_keycodes { minimum = 8; maximum = 255; <LFSH> = 50; <AE01> = 10; <AC01> = 38; <CAPS> = 66; <RALT> = 108; };
 xkb_types {
  virtual_modifiers LevelThree;
  type "ONE_LEVEL" { modifiers = none; map[none] = Level1; level_name[Level1] = "Any"; };
  type "TWO_LEVEL" { modifiers = Shift; map[Shift] = Level2; level_name[Level1] = "Base"; level_name[Level2] = "Shift"; };
  type "ALPHABETIC" { modifiers = Shift+Lock; map[Shift] = Level2; map[Lock] = Level2; map[Shift+Lock] = Level1; level_name[Level1] = "Base"; level_name[Level2] = "Caps"; };
  type "FOUR_LEVEL" { modifiers = Shift+LevelThree; map[Shift] = Level2; map[LevelThree] = Level3; map[Shift+LevelThree] = Level4; level_name[Level1] = "Base"; level_name[Level2] = "Shift"; level_name[Level3] = "Alt Base"; level_name[Level4] = "Shift Alt"; };
 };
 xkb_compatibility {
  virtual_modifiers LevelThree;
  interpret Shift_L { action = SetMods(modifiers = Shift); };
  interpret Caps_Lock { action = LockMods(modifiers = Lock); };
  interpret ISO_Level3_Shift { virtualModifier = LevelThree; action = SetMods(modifiers = LevelThree); };
 };
 xkb_symbols {
  name[Group1] = "us"; name[Group2] = "fr";
  key <LFSH> { [ Shift_L ] };
  key <CAPS> { [ Caps_Lock ] };
  key <RALT> { type = "ONE_LEVEL", [ ISO_Level3_Shift ] };
  key <AE01> { type[Group1] = "TWO_LEVEL", type[Group2] = "FOUR_LEVEL", symbols[Group1] = [ 1, exclam ], symbols[Group2] = [ ampersand, 1, bar ] };
  key <AC01> { type = "ALPHABETIC", [ a, A ] };
  modifier_map Shift { <LFSH> };
  modifier_map Lock { <CAPS> };
  modifier_map Mod5 { <RALT> };
 };
};
"#;
    const SHIFT: u32 = 50;
    const ONE: u32 = 10;
    const A: u32 = 38;
    const CAPS: u32 = 66;
    const ALTGR: u32 = 108;

    /// A state on the map above, or `None` with the reason printed — a
    /// machine with no `libxkbcommon` has nothing to test, and should say
    /// so rather than pass.
    fn a_state() -> Option<State> {
        let Some(map) = Keymap::from_text(MAP) else {
            eprintln!("skipped: libxkbcommon could not be loaded, or the test keymap did not compile on it");
            return None;
        };
        State::new(map, "C")
    }

    #[test]
    fn shift_is_in_force_the_moment_its_key_went_down() {
        let Some(mut s) = a_state() else { return };
        assert_eq!(s.typed(ONE, true, false), "1");
        s.typed(ONE, false, false);
        // Nobody told the state about Shift but the Shift key itself.
        assert_eq!(s.typed(SHIFT, true, false), "");
        assert_eq!(s.typed(ONE, true, false), "!");
        s.typed(ONE, false, false);
        s.typed(SHIFT, false, false);
        assert_eq!(s.typed(ONE, true, false), "1");
    }

    #[test]
    fn a_repeat_reads_and_moves_nothing() {
        let Some(mut s) = a_state() else { return };
        s.typed(SHIFT, true, false);
        assert_eq!(s.typed(A, true, false), "A");
        assert_eq!(s.typed(A, true, true), "A");
        assert_eq!(s.typed(A, true, true), "A");
        s.typed(A, false, false);
        s.typed(SHIFT, false, false);
        assert_eq!(s.typed(A, true, false), "a");
    }

    #[test]
    fn caps_lock_is_a_key_too_and_shift_undoes_it() {
        let Some(mut s) = a_state() else { return };
        s.typed(CAPS, true, false);
        s.typed(CAPS, false, false);
        assert_eq!(s.typed(A, true, false), "A");
        s.typed(A, false, false);
        s.typed(SHIFT, true, false);
        assert_eq!(s.typed(A, true, false), "a");
        s.typed(A, false, false);
        s.typed(SHIFT, false, false);
        // The digit row has no Lock in its type: still a digit.
        assert_eq!(s.typed(ONE, true, false), "1");
    }

    #[test]
    fn the_french_row_is_the_other_way_round_and_altgr_is_a_third_level() {
        let Some(mut s) = a_state() else { return };
        // The compositor says group 2 — the one thing it is trusted for.
        s.locks(0, 1);
        assert_eq!(s.typed(ONE, true, false), "&");
        s.typed(ONE, false, false);
        s.typed(SHIFT, true, false);
        assert_eq!(s.typed(ONE, true, false), "1");
        s.typed(ONE, false, false);
        s.typed(SHIFT, false, false);
        s.typed(ALTGR, true, false);
        assert_eq!(s.typed(ONE, true, false), "|");
        s.typed(ONE, false, false);
        s.typed(ALTGR, false, false);
        assert_eq!(s.typed(ONE, true, false), "&");
    }

    #[test]
    fn what_the_compositor_says_is_pressed_is_not_believed() {
        let Some(mut s) = a_state() else { return };
        s.typed(SHIFT, true, false);
        // A `modifiers` event with nothing depressed, while Shift is down:
        // the Hyprland lie. Locks and group are taken; the press is kept.
        s.locks(0, 0);
        assert_eq!(s.typed(ONE, true, false), "!");
        s.typed(ONE, false, false);
        // Caps Lock set from outside — a lock is the compositor's to say.
        s.locks(2, 0);
        assert_eq!(s.typed(A, true, false), "a", "Shift and Lock together are lowercase");
        s.typed(A, false, false);
        s.typed(SHIFT, false, false);
        assert_eq!(s.typed(A, true, false), "A");
    }

    #[test]
    fn losing_the_keyboard_releases_what_was_held_and_keeps_the_locks() {
        let Some(mut s) = a_state() else { return };
        s.typed(CAPS, true, false);
        s.typed(CAPS, false, false);
        s.typed(SHIFT, true, false);
        s.release_all();
        assert_eq!(s.typed(A, true, false), "A", "Caps Lock survives, Shift does not");
        s.typed(A, false, false);
        assert_eq!(s.typed(ONE, true, false), "1");
    }

    #[test]
    fn what_xkbcommon_wrote_is_read_back_without_its_nul() {
        assert_eq!(text_of(b"!\0\0\0", 1), "!");
        assert_eq!(text_of(b"\xc3\xa9\0", 2), "é");
        assert_eq!(text_of(b"\0\0", 0), "");
        assert_eq!(text_of(b"\0\0", -1), "");
        // A count past the buffer stops one short of its end.
        assert_eq!(text_of(b"ab\0", 9), "ab");
    }
}

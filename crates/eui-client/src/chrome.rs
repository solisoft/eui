//! The shell's own interface: a tab strip, an address bar, and the page a
//! tab shows before it has an application in it.
//!
//! It is drawn the way an application is drawn — an ordinary tree on an
//! ordinary [`Driver`], through the same layout, text and paint path — and
//! it is deliberately not privileged over one. There is no wire protocol
//! here and no server: the tree is built in this file, and a click comes
//! back as the `Event` frame the driver would have sent, which nobody
//! sends. The node id in that frame is how a click becomes an [`Action`].
//!
//! The chrome always covers the whole window, and the application draws
//! over the part of it below [`Chrome::content_top`] — so the strip and the
//! address bar are one list and the application is another, composited by
//! `Renderer::render` with an origin (see `Target::origin`).

use std::collections::HashMap;

use eui_proto::{
    AlignItems, Batch, ColorRef, Dim, Display, EventKind, FlatNode, FontWeight, Frame, Handler, Justify, NodeKind, Op, Overflow, StyleRecord, Subtree, TextAlign, TextRef, ThemeMode, Value,
};
use eui_render::DrawList;
use eui_theme::Role;

use crate::driver::{Driver, Input};

/// The tab strip's height, and the address row's, in device-independent px.
///
/// They are the theme's own control heights (`05-theme.md` §2: 28 / 36 / 44)
/// rather than numbers of this file's own: a tab is a small control and the
/// address bar is a large one.
const STRIP_H: u16 = 36;
const ADDR_H: u16 = 44;

/// What a click on the chrome meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Show tab `n`.
    Select(usize),
    /// Close tab `n`.
    Close(usize),
    /// Open an empty tab after the last one.
    NewTab,
    /// Put the address bar into editing, with everything selected.
    EditAddress,
    /// The address bar was committed: open this in the active tab.
    Open(String),
    /// Open the active tab's address again, from nothing.
    Reload,
    /// A step back through the active tab's trail.
    Back,
    /// A step forward through it.
    Forward,
    /// Put the consent sheet up again for the active tab, so what an
    /// application was allowed can be seen and changed after the fact.
    Permissions,
    /// Put the active tab's application in the desktop's own launcher, or
    /// take it out again.
    Install,
    /// See [`Action::Install`].
    Uninstall,
    /// Leave the address bar and show the address again. Enter on an
    /// unchanged address produces this and nothing else: the driver emits
    /// `Change` only when the text actually moved, so without this a person
    /// who clicked into the bar and changed their mind had no way out.
    LeaveAddress,
    /// Take this address off the recent list and draw the page again.
    Forget(String),
    /// The chrome's own state moved — the keyboard walked the recent list —
    /// and the tree has to be built again to show it. The chrome cannot do
    /// that itself: a rebuild needs the tabs, which the shell owns.
    Rebuild,
}

/// How much the origin in the address bar is trusted (08 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// `wss://`, and the publisher's key is pinned.
    Pinned,
    /// `ws://` on loopback, allowed by a debug build or an embedding host.
    Local,
    /// Connected, but the manifest did not check out.
    Unverified,
}

/// One tab, as the strip needs to see it. Borrowed for the rebuild and not
/// kept: the shell owns the sessions, this is only what they look like.
pub struct TabView<'a> {
    /// The manifest's name where there is one, else the last path segment.
    pub title: &'a str,
    /// `wss://host`, the half of the URL a key is pinned to.
    pub origin: &'a str,
    /// Everything after the origin.
    pub path: &'a str,
    /// `None` for a tab with no application in it yet.
    pub trust: Option<Trust>,
    /// Whether this tab has somewhere to go back to, and somewhere to go
    /// forward to. The chrome draws the arrows dim and inert otherwise —
    /// rather than hiding them, which would move the address bar sideways
    /// every time a page was opened.
    pub can_back: bool,
    /// See `can_back`.
    pub can_forward: bool,
    /// What the socket is doing, when that is worth a word: `reconnecting`
    /// while the client is getting the session back, `offline` once it has
    /// given up, `page` for a tab that has no socket because it never
    /// needed one (01 §2.4). `None` while it is simply talking.
    ///
    /// Spec 01 §4: a window that stopped talking to its application must
    /// not look like one that is merely idle. And the converse, which is
    /// what `page` is for: a window holding no session must not look like
    /// one that is, which it otherwise does — the address bar shows the
    /// manifest's `entry`, and that is a `wss://` whether or not anything
    /// was ever dialled.
    pub link: Option<&'a str>,
    /// What this application's manifest asked for, once it has arrived, so
    /// the address row can offer the answer back. `None` before the manifest
    /// and for a manifest that asks for nothing — there is no question to
    /// reopen, and an icon that opens an empty sheet is worse than no icon.
    pub grants: Option<u32>,
    /// Whether this application is in the desktop's launcher, when it is
    /// the kind of application that can be: `Some(true)` installed,
    /// `Some(false)` installable and not installed.
    ///
    /// `None` is most applications. An application is installable only if
    /// its manifest publishes an icon, because an entry has to have a
    /// picture and the publisher's is the only one that is the
    /// application's own — so the control is absent rather than inert,
    /// which is the same choice the padlock makes.
    pub installed: Option<bool>,
}

/// The chrome: a driver, a tree, and what its node ids mean.
pub struct Chrome {
    driver: Driver,
    /// Node id to what clicking it does, rebuilt with the tree.
    actions: HashMap<u32, Action>,
    /// The address bar is in editing rather than display.
    editing: bool,
    /// True while the active tab has no application, so the chrome owns the
    /// whole window and there is no address row.
    blank: bool,
    /// Applications worth offering on a blank page, newest first.
    recents: Vec<crate::recent::Recent>,
    /// Which of them the keyboard is on, if it has left the address field.
    /// `None` means the field has it, which is where a blank page starts.
    picked: Option<usize>,
    /// The next rebuild is the chrome's own doing — the keyboard walked the
    /// list — and must not take half a typed address with it. A rebuild
    /// replaces the tree, and the blank page's field is built empty.
    keep_address: bool,
    /// Why the last application in the active tab stopped, if it did.
    ///
    /// Kept here rather than on `TabView` so that setting it is one call and
    /// not a change every caller has to make — the same reason `recents`
    /// lives here.
    trouble: Option<String>,
    /// The batch number, which has to rise.
    seq: u64,
    /// The atom and the style catalogue have been sent. They are sent once
    /// and only once: a mount does not clear the session's tables.
    defined: bool,
}

/// Node ids. Fixed for the parts there is one of, and strided for tabs so a
/// click's node id gives back which tab it was without a lookup table for
/// every child.
const ROOT: u32 = 1;
const STRIP: u32 = 2;
const PLUS: u32 = 3;
const PLUS_TEXT: u32 = 17;
const RELOAD: u32 = 18;
const RELOAD_TEXT: u32 = 19;
const BACK: u32 = 22;
const BACK_TEXT: u32 = 23;
const FWD: u32 = 24;
const FWD_TEXT: u32 = 25;
const LINK: u32 = 20;
const GRANTS: u32 = 26;
const GRANTS_TEXT: u32 = 27;
const INSTALL: u32 = 28;
const INSTALL_TEXT: u32 = 29;
const LINK_TEXT: u32 = 21;
const ADDR: u32 = 4;
const FIELD: u32 = 5;
const CHIP: u32 = 6;
const CHIP_TEXT: u32 = 7;
const ORIGIN: u32 = 8;
const PATH: u32 = 9;
const INPUT: u32 = 10;
const CONTENT: u32 = 11;
const BLANK_MARK: u32 = 12;
const BLANK_MARK_TEXT: u32 = 13;
const BLANK_HEAD: u32 = 14;
const BLANK_SUB: u32 = 15;
const BLANK_FIELD: u32 = 16;
/// Why the last application in this tab stopped, on the page it left behind.
/// An `input` and not a `text`, so the reason can be selected and copied:
/// the client owns caret and clipboard inside an editable (03 §3) and owns
/// neither on a label, and a diagnostic nobody can paste into a bug report
/// is half a diagnostic.
const BLANK_WHY: u32 = 17;
/// The recent list on the blank page. Eight ids to a row, of which five are
/// used: the row, its sigil box, the letter in it, the name and the host. A
/// stride that only just fits is a collision waiting for the next label —
/// at four, the first row's host *was* the second row's box.
const RECENT_BASE: u32 = 200;
const RECENT_STRIDE: u32 = 8;
/// How many of a row's ids answer a click: all of them, so the sigil, either
/// label and the Delete hint open the same application as the row.
const RECENT_PARTS: u32 = 6;
/// The atom for an icon's `name` prop. The chrome defines two atoms and
/// this is the second; ids are the chrome's own and nothing on the wire
/// ever sees them.
const ATOM_NAME: u32 = 2;
const TAB_BASE: u32 = 100;
const TAB_STRIDE: u32 = 8;

impl Chrome {
    /// A chrome for a window of `w × h` at `scale`.
    pub fn new(w: f32, h: f32, scale: f32) -> Self {
        Self {
            driver: Driver::new(w, h, scale, 0),
            actions: HashMap::new(),
            editing: false,
            blank: true,
            recents: Vec::new(),
            picked: None,
            keep_address: false,
            trouble: None,
            seq: 0,
            defined: false,
        }
    }

    /// Where the application's viewport starts, in device-independent px.
    /// The whole window when the active tab is still empty — then there is
    /// no application, and the chrome draws the page itself.
    pub fn content_top(&self) -> f32 {
        if self.blank {
            f32::INFINITY
        } else {
            f32::from(STRIP_H) + f32::from(ADDR_H)
        }
    }

    /// True when the chrome, not an application, owns the area below the
    /// strip.
    pub fn is_blank(&self) -> bool {
        self.blank
    }

    /// The window changed size.
    pub fn resized(&mut self, w: f32, h: f32, scale: f32) {
        let _ = self.driver.input(Input::Resized(w, h, scale));
    }

    /// Follow the desktop palette, as an application's driver does.
    pub fn set_desktop_theme(&mut self, mode: Option<ThemeMode>, colors: Vec<(Role, u32)>) {
        let _ = self.driver.set_desktop_theme(mode, colors);
    }

    /// The palette the machine is in, told to the chrome as it is built.
    ///
    /// The chrome draws the box under everything, so a chrome born light
    /// on a dark desktop *is* the flash of a light window, whatever the
    /// application above it does.
    pub fn set_mode(&mut self, mode: ThemeMode) {
        let _ = self.driver.input(Input::Mode(mode));
    }

    /// Whether the chrome wants the next frame drawn.
    pub fn needs_redraw(&self) -> bool {
        self.driver.needs_redraw()
    }

    /// The pointer shape the chrome asks for.
    pub fn cursor(&self) -> eui_proto::Cursor {
        self.driver.cursor()
    }

    /// Text the address bar asked to put on the clipboard — a copy or a cut
    /// out of it. The window owns the clipboard; a driver never touches it.
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.driver.take_clipboard()
    }

    /// Where an input method should sit, while the address bar is editing.
    pub fn ime_area(&self) -> Option<eui_layout::Rect> {
        self.driver.ime_area()
    }

    /// This frame's draw list, and the atlases it drew into.
    pub fn paint(&mut self, device_w: u32, device_h: u32) -> std::sync::Arc<DrawList> {
        self.driver.paint(device_w, device_h)
    }

    /// How old the list last handed out is, in seconds (03 §5).
    pub fn list_age(&self, now: crate::time::Instant) -> f32 {
        self.driver.list_age(now)
    }

    /// The chrome's own atlases, which are its alone: it is a session like
    /// any other as far as the renderer is concerned.
    pub fn atlases_mut(&mut self) -> (&mut eui_render::Atlas, &mut eui_render::ImageAtlas) {
        self.driver.atlases_mut()
    }

    /// Hand the chrome an input and read back what it meant.
    ///
    /// The driver answers an event with the frames it would have sent a
    /// server. Nothing sends them; their node ids are the whole point.
    pub fn input(&mut self, i: Input) -> Vec<Action> {
        if let Some(out) = self.walk_recents(&i) {
            return out;
        }
        // Spelling an address is no longer choosing from a list. The key
        // still goes to the field — it is a character, and the field is what
        // it is for — so the highlight is dropped here and the page is drawn
        // again after it, with what was typed kept (`keep_address`).
        let typing = matches!(i, Input::Text(_) | Input::Paste(_)) && self.picked.is_some();
        if typing {
            self.picked = None;
            self.keep_address = true;
        }
        let frames = self.driver.input(i);
        let mut out = Vec::new();
        if typing {
            out.push(Action::Rebuild);
        }
        for f in frames {
            let Frame::Event(e) = f else { continue };
            match e.event {
                // A committed field: `Change` carries the text, and is what
                // Enter produces before `Submit`.
                EventKind::Change if e.node == INPUT || e.node == BLANK_FIELD => {
                    if let Value::Str(s) = &e.payload {
                        let url = s.trim();
                        if !url.is_empty() {
                            out.push(Action::Open(url.to_owned()));
                        }
                    }
                }
                // Enter, after whatever `Change` had to say about the text.
                EventKind::Submit if e.node == INPUT => out.push(Action::LeaveAddress),
                EventKind::Click => {
                    if let Some(a) = self.actions.get(&e.node) {
                        out.push(a.clone());
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// What the blank page's address field holds right now, read back out
    /// of the tree the driver is showing.
    fn address_text(&self) -> String {
        self.driver.session().lookup(BLANK_FIELD).and_then(|ix| self.driver.session().text_of(ix)).unwrap_or_default().to_owned()
    }

    /// The keyboard on the blank page's list of applications, before the
    /// address field is given a look at the same keys.
    ///
    /// `Some` for a key this took, which is then not passed on: Down and Up
    /// walk the list, Enter opens what they are standing on, Delete takes it
    /// off the list, and Escape hands the keyboard back to the field. Every
    /// one of them means something else in a text field, so a key that
    /// reaches the list must not also reach the field.
    ///
    /// Anything typed puts the keyboard back in the field: a person who
    /// starts spelling an address is no longer choosing from a list.
    fn walk_recents(&mut self, i: &Input) -> Option<Vec<Action>> {
        let shown = self.recents.len().min(crate::recent::KEEP);
        if !self.blank || shown == 0 {
            self.picked = None;
            return None;
        }
        let Input::Key { key, modifiers, down } = i else { return None };
        if !down || modifiers & 0b1110 != 0 {
            return None;
        }
        self.keep_address = true;
        match key.as_str() {
            "ArrowDown" => {
                self.picked = Some(self.picked.map_or(0, |i| i.saturating_add(1).min(shown - 1)));
                Some(vec![Action::Rebuild])
            }
            "ArrowUp" => {
                // Up off the first row is back to the address field, which
                // is where the keyboard was before Down was ever pressed.
                self.picked = self.picked.and_then(|i| i.checked_sub(1));
                Some(vec![Action::Rebuild])
            }
            "Escape" if self.picked.is_some() => {
                self.picked = None;
                Some(vec![Action::Rebuild])
            }
            "Enter" => {
                let url = self.recents.get(self.picked?)?.url.clone();
                self.picked = None;
                Some(vec![Action::Open(url)])
            }
            "Delete" => {
                let at = self.picked?;
                let url = self.recents.get(at)?.url.clone();
                // Stand on the row that moves up into this one, or on the
                // last one when this was the last: the hand is on Delete and
                // the next press should take the next entry, not nothing.
                self.picked = (shown > 1).then(|| at.min(shown - 2));
                Some(vec![Action::Forget(url)])
            }
            _ => None,
        }
    }

    /// The applications the blank page offers. Kept here rather than read
    /// from disk on each rebuild, which happens on every tab switch.
    pub fn set_recents(&mut self, recents: Vec<crate::recent::Recent>) {
        self.recents = recents;
        let shown = self.recents.len().min(crate::recent::KEEP);
        self.picked = self.picked.filter(|_| shown > 0).map(|i| i.min(shown.saturating_sub(1)));
    }

    /// Why the active tab has no application. `None` for a tab that simply
    /// has not opened one yet — which is not a fault and should not read
    /// like one.
    pub fn set_trouble(&mut self, why: Option<String>) {
        self.trouble = why;
    }

    /// Put the address bar into editing and select what is there.
    pub fn edit_address(&mut self) {
        self.editing = true;
    }

    /// Leave the address bar, so the keyboard goes back to the application.
    pub fn leave_address(&mut self) {
        self.editing = false;
    }

    /// Whether the address bar holds the keyboard: it is being edited, or
    /// this tab has no application and the page is the chrome's own.
    pub fn holds_keys(&self) -> bool {
        self.blank || self.editing
    }

    /// Rebuild the tree for this set of tabs.
    ///
    /// Cheap enough to do on every change: a dozen nodes and no text
    /// shaping that was not already cached. It is not done on every *frame*
    /// — a rebuild resets the field's edit state, so what is being typed
    /// would go with it.
    pub fn rebuild(&mut self, tabs: &[TabView<'_>], active: usize) {
        let mut b = Builder::new();
        self.actions.clear();
        let keep = std::mem::take(&mut self.keep_address);
        self.blank = tabs.get(active).map_or(true, |t| t.trust.is_none());
        let editing = self.editing && !self.blank;

        // ------------------------------------------------------- styles
        let role = ColorRef::role;
        let surface = role(Role::SurfaceBase.id());
        let sunken = role(Role::SurfaceSunken.id());
        let muted = role(Role::TextMuted.id());
        let text = role(Role::TextDefault.id());

        let s_root = b.style(StyleRecord { display: Display::Column, width: Dim::Percent(10000), height: Dim::Percent(10000), bg: surface, ..Default::default() });
        let s_strip = b.style(StyleRecord {
            display: Display::Row,
            height: Dim::Px(STRIP_H),
            // The tabs sit on the floor of the strip so the active one can
            // stand a little taller and meet the surface below it.
            align_items: AlignItems::End,
            gap: 1,
            padding: [0, 2, 0, 2],
            bg: sunken,
            overflow: Overflow::Clip,
            ..Default::default()
        });
        let s_tab = b.style(StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            height: Dim::Px(30),
            min_width: Dim::Px(92),
            max_width: Dim::Px(200),
            grow: 1,
            shrink: 1,
            basis: Dim::Px(168),
            gap: 2,
            padding: [0, 2, 0, 3],
            radius: 2,
            fg: muted,
            cursor: eui_proto::Cursor::Pointer,
            overflow: Overflow::Clip,
            ..Default::default()
        });
        let s_tab_on = b.style(StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            height: Dim::Px(32),
            min_width: Dim::Px(92),
            max_width: Dim::Px(200),
            grow: 1,
            shrink: 1,
            basis: Dim::Px(168),
            gap: 2,
            padding: [0, 2, 0, 3],
            radius: 2,
            // The active tab takes the surface of the application below it,
            // which is what makes the two read as one thing, and the only
            // colour in the strip sits along its top edge.
            bg: surface,
            border_width: [2, 0, 0, 0],
            border_color: role(Role::AccentBase.id()),
            fg: text,
            cursor: eui_proto::Cursor::Pointer,
            overflow: Overflow::Clip,
            ..Default::default()
        });
        // One sigil style per tone, defined whatever the tabs are: the
        // catalogue below has to come out the same on every rebuild, since
        // it is only sent once.
        let s_sigils: Vec<u32> = TINTS
            .iter()
            .map(|r| {
                b.style(StyleRecord {
                    width: Dim::Px(16),
                    height: Dim::Px(16),
                    display: Display::Row,
                    justify: Justify::Center,
                    align_items: AlignItems::Center,
                    radius: 1,
                    bg: role(r.id()),
                    fg: role(Role::TextInverted.id()),
                    ..Default::default()
                })
            })
            .collect();
        let s_sigil_off = b.style(StyleRecord {
            width: Dim::Px(16),
            height: Dim::Px(16),
            display: Display::Row,
            justify: Justify::Center,
            align_items: AlignItems::Center,
            radius: 1,
            bg: role(Role::BorderDefault.id()),
            fg: role(Role::TextInverted.id()),
            ..Default::default()
        });
        let s_sigil_text = b.style(StyleRecord { font_size: 0, font_weight: FontWeight::Bold, ..Default::default() });
        let s_name = b.style(StyleRecord { font_size: 1, grow: 1, shrink: 1, basis: Dim::Px(0), line_clamp: 1, ..Default::default() });
        let s_name_on = b.style(StyleRecord { font_size: 1, font_weight: FontWeight::Bold, grow: 1, shrink: 1, basis: Dim::Px(0), line_clamp: 1, ..Default::default() });
        let s_close_text = b.style(StyleRecord { font_size: 1, ..Default::default() });
        let s_plus_text = b.style(StyleRecord { font_size: 2, ..Default::default() });
        let s_close = b.style(StyleRecord {
            width: Dim::Px(16),
            height: Dim::Px(16),
            display: Display::Row,
            justify: Justify::Center,
            align_items: AlignItems::Center,
            radius: 1,
            font_size: 1,
            fg: muted,
            cursor: eui_proto::Cursor::Pointer,
            ..Default::default()
        });
        let s_plus = b.style(StyleRecord {
            width: Dim::Px(28),
            height: Dim::Px(28),
            display: Display::Row,
            justify: Justify::Center,
            align_items: AlignItems::Center,
            radius: 2,
            font_size: 2,
            fg: muted,
            cursor: eui_proto::Cursor::Pointer,
            ..Default::default()
        });
        let s_addr = b.style(StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            height: Dim::Px(ADDR_H),
            gap: 2,
            padding: [0, 3, 0, 3],
            bg: surface,
            border_width: [0, 0, 1, 0],
            border_color: role(Role::BorderSubtle.id()),
            ..Default::default()
        });
        // One style, edited or not. Focus is not this box's to draw: 03 §3
        // has the painter put a two-pixel ring around whatever holds it,
        // outside the border box. Colouring this border `FocusRing` as well
        // drew a second blue rectangle just inside the first — two nested
        // outlines with the text wedged between them.
        let s_field = b.style(StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            height: Dim::Px(28),
            grow: 1,
            gap: 2,
            padding: [0, 3, 0, 3],
            radius: 2,
            bg: sunken,
            border_width: [1; 4],
            border_color: role(Role::BorderSubtle.id()),
            cursor: eui_proto::Cursor::Text,
            overflow: Overflow::Clip,
            ..Default::default()
        });
        let s_chip_text = b.style(StyleRecord { font_size: 0, font_weight: FontWeight::Bold, ..Default::default() });
        let mut chip_style = |tone: Role| {
            b.style(StyleRecord {
                display: Display::Row,
                align_items: AlignItems::Center,
                height: Dim::Px(20),
                padding: [0, 2, 0, 2],
                radius: 1,
                bg: role(subtle_of(tone).id()),
                fg: role(tone.id()),
                ..Default::default()
            })
        };
        let s_chip_ok = chip_style(Role::SuccessBase);
        let s_chip_local = chip_style(Role::WarningBase);
        let s_chip_bad = chip_style(Role::DangerBase);
        let s_origin = b.style(StyleRecord { font_size: 1, font_family: eui_proto::FontFamily::Mono, fg: text, line_clamp: 1, ..Default::default() });
        let s_path = b.style(StyleRecord { font_size: 1, font_family: eui_proto::FontFamily::Mono, fg: muted, grow: 1, shrink: 1, basis: Dim::Px(0), line_clamp: 1, ..Default::default() });
        // Inset by two pixels on every side, and shorter than the row it
        // sits in, so that the focus ring 03 §3 draws *outside* this box has
        // somewhere to land. Flush against the field, the ring fell under
        // the field's own `overflow: clip` and survived only as its two
        // vertical edges — a pair of blue bars with the address between
        // them, and no sign of what they belonged to.
        let s_input = b.style(StyleRecord { font_size: 1, font_family: eui_proto::FontFamily::Mono, fg: text, grow: 1, height: Dim::Px(20), margin: [1, 1, 1, 1], ..Default::default() });

        // ------------------------------------------------------- the tree
        //
        // Pre-order with child counts, so every subtree is written parent
        // first and the counts below have to match what follows them.
        let tab_count = tabs.len();
        // The strip, then either the page the chrome draws itself or the
        // address row and the hole the application is composited into. A
        // count that does not match what follows it is not an error the
        // graft reports where it happens: the extra node is taken for a
        // sibling of the root, and the *next* mount then finds its id
        // already in the tree.
        b.open(NodeKind::Box, ROOT, s_root, if self.blank { 2 } else { 3 });

        // -- strip: one node per tab, then the new-tab button
        b.open(NodeKind::Box, STRIP, s_strip, tab_count as u32 + 1);
        for (i, t) in tabs.iter().enumerate() {
            let on = i == active;
            let id = TAB_BASE + (i as u32) * TAB_STRIDE;
            self.actions.insert(id, Action::Select(i));
            self.actions.insert(id + 1, Action::Select(i));
            self.actions.insert(id + 2, Action::Select(i));
            self.actions.insert(id + 3, Action::Close(i));
            self.actions.insert(id + 6, Action::Close(i));
            b.click();
            b.open(NodeKind::Box, id, if on { s_tab_on } else { s_tab }, 3);
            {
                // The sigil: the first letter of the name. Once four tabs
                // have elided their titles it is the only thing telling
                // them apart.
                let s = match t.trust {
                    Some(_) => s_sigils.get(tint_index([t.origin, t.path])).copied().unwrap_or(s_sigil_off),
                    None => s_sigil_off,
                };
                b.open(NodeKind::Box, id + 1, s, 1);
                let letter = t.title.chars().next().unwrap_or('·').to_uppercase().to_string();
                b.text(id + 5, s_sigil_text, &letter);
                b.close();
            }
            b.text(id + 2, if on { s_name_on } else { s_name }, t.title);
            b.click();
            b.open(NodeKind::Box, id + 3, s_close, 1);
            b.text(id + 6, s_close_text, "×");
            b.close();
            b.close();
        }
        self.actions.insert(PLUS, Action::NewTab);
        self.actions.insert(PLUS_TEXT, Action::NewTab);
        b.click();
        b.open(NodeKind::Box, PLUS, s_plus, 1);
        b.text(PLUS_TEXT, s_plus_text, "+");
        b.close();
        b.close();

        // -- below the strip: an address row and the application, or the
        //    page a tab shows before it has one.
        // Every style the tree could use, defined whatever this rebuild
        // draws. `Op::Mount` replaces the document and leaves the tables
        // alone, so the catalogue is sent once and has to be identical
        // every time — a style defined only in one branch would shift the
        // ids of everything after it.
        let s_blank =
            b.style(StyleRecord { display: Display::Column, grow: 1, justify: Justify::Center, align_items: AlignItems::Center, gap: 4, padding: [8, 6, 9, 6], bg: surface, ..Default::default() });
        // Why the last application stopped. An `input` and not a `text`,
        // because the client owns caret, selection and `Ctrl+C` inside an
        // editable (03 §3) and owns none of them on a label — and a
        // diagnostic nobody can paste into a bug report is half a
        // diagnostic. It is styled to read as a sentence rather than a
        // field: no border, no fill, just the words, in the colour the rest
        // of the page uses for something that went wrong.
        let s_why = b.style(StyleRecord {
            width: Dim::Px(560),
            padding: [1, 2, 1, 2],
            font_family: eui_proto::FontFamily::Mono,
            font_size: 0,
            fg: role(Role::DangerBase.id()),
            bg: ColorRef::NONE,
            ..Default::default()
        });
        let s_mark = {
            b.style(StyleRecord {
                width: Dim::Px(44),
                height: Dim::Px(44),
                display: Display::Row,
                justify: Justify::Center,
                align_items: AlignItems::Center,
                radius: 3,
                bg: role(Role::AccentBase.id()),
                fg: role(Role::AccentOn.id()),
                ..Default::default()
            })
        };
        let s_mark_text = b.style(StyleRecord { font_size: 2, font_weight: FontWeight::Bold, ..Default::default() });
        let s_head = b.style(StyleRecord { font_size: 3, font_weight: FontWeight::Bold, fg: text, text_align: TextAlign::Center, ..Default::default() });
        let s_sub = b.style(StyleRecord { font_size: 1, fg: muted, text_align: TextAlign::Center, max_width: Dim::Px(420), ..Default::default() });
        let s_big = {
            b.style(StyleRecord {
                font_size: 1,
                font_family: eui_proto::FontFamily::Mono,
                fg: text,
                width: Dim::Px(520),
                max_width: Dim::Percent(10000),
                height: Dim::Px(38),
                padding: [0, 4, 0, 4],
                radius: 2,
                bg: sunken,
                border_width: [1, 1, 1, 1],
                border_color: role(Role::FocusRing.id()),
                cursor: eui_proto::Cursor::Text,
                ..Default::default()
            })
        };
        let s_reload = b.style(StyleRecord {
            width: Dim::Px(28),
            height: Dim::Px(28),
            display: Display::Row,
            justify: Justify::Center,
            align_items: AlignItems::Center,
            radius: 2,
            fg: muted,
            cursor: eui_proto::Cursor::Pointer,
            ..Default::default()
        });
        let s_reload_text = b.style(StyleRecord { font_size: 2, ..Default::default() });
        // Back and forward. The same box as reload, so the three read as one
        // row of controls; the one with nowhere to go keeps its place and
        // loses its pointer, because an arrow that vanishes takes the
        // address bar sideways with it every time a page is opened.
        let s_step_off = b.style(StyleRecord {
            display: Display::Row,
            width: Dim::Px(28),
            height: Dim::Px(28),
            justify: Justify::Center,
            align_items: AlignItems::Center,
            radius: 2,
            // `text.disabled`, not `border.subtle`. A hairline colour on a
            // glyph is not a dim control, it is an invisible one — and a tab
            // opens with nothing behind it and nothing ahead, so *both*
            // arrows are in this state the first time anybody looks at them.
            // Which is how a feature that was there read as a feature that
            // was missing.
            fg: ColorRef::role(Role::TextDisabled.id()),
            ..Default::default()
        });
        // 16 px, the size the client's own icons are drawn beside a label.
        let s_step_icon = b.style(StyleRecord { width: Dim::Px(16), height: Dim::Px(16), ..Default::default() });
        let s_recents = b.style(StyleRecord { display: Display::Column, gap: 1, width: Dim::Px(520), max_width: Dim::Percent(10000), ..Default::default() });
        let s_recent = b.style(StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            gap: 3,
            height: Dim::Px(40),
            padding: [0, 3, 0, 3],
            radius: 2,
            bg: role(Role::SurfaceRaised.id()),
            border_width: [1; 4],
            border_color: role(Role::BorderSubtle.id()),
            cursor: eui_proto::Cursor::Pointer,
            overflow: Overflow::Clip,
            ..Default::default()
        });
        // The row the keyboard is standing on. Same box, so nothing moves
        // when the pick does: the border carries it, as a focus ring would.
        let s_recent_on = b.style(StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            gap: 3,
            height: Dim::Px(40),
            padding: [0, 3, 0, 3],
            radius: 2,
            bg: role(Role::SurfaceBase.id()),
            border_width: [1; 4],
            border_color: role(Role::AccentBase.id()),
            cursor: eui_proto::Cursor::Pointer,
            overflow: Overflow::Clip,
            ..Default::default()
        });
        let s_recent_name = b.style(StyleRecord { font_size: 1, fg: text, line_clamp: 1, ..Default::default() });
        // What Delete does, said on the row it would do it to and nowhere
        // else: a key nobody can see is a key nobody presses.
        let s_recent_del = b.style(StyleRecord {
            font_size: 0,
            font_family: eui_proto::FontFamily::Mono,
            fg: muted,
            padding: [0, 2, 0, 2],
            radius: 1,
            bg: sunken,
            border_width: [1; 4],
            border_color: role(Role::BorderSubtle.id()),
            ..Default::default()
        });
        let s_recent_where = b.style(StyleRecord {
            font_size: 0,
            font_family: eui_proto::FontFamily::Mono,
            fg: muted,
            grow: 1,
            shrink: 1,
            basis: Dim::Px(0),
            line_clamp: 1,
            text_align: TextAlign::End,
            ..Default::default()
        });

        // The application's own area. It draws nothing — the application's
        // list is composited over it — but it has to be in the tree so the
        // strip and the row are the height they are.
        let s_hole = b.style(StyleRecord { grow: 1, ..Default::default() });

        if self.blank {
            let shown = self.recents.len().min(crate::recent::KEEP);
            let why = self.trouble.as_deref().unwrap_or("");
            b.open(NodeKind::Box, CONTENT, s_blank, 4 + u32::from(!why.is_empty()) + u32::from(shown > 0));
            b.open(NodeKind::Box, BLANK_MARK, s_mark, 1);
            b.text(BLANK_MARK_TEXT, s_mark_text, "EUI");
            b.close();
            b.text(BLANK_HEAD, s_head, "Open an application");
            let sub = if shown > 0 { "Type an address and press Enter, or ↓ to pick one below." } else { "Type an address and press Enter." };
            b.text(BLANK_SUB, s_sub, sub);
            b.change();
            // Empty, unless this rebuild is one the chrome asked for itself:
            // walking the recent list must not take a half-typed address
            // with it.
            let typed = if keep { self.address_text() } else { String::new() };
            b.text_node(NodeKind::Input, BLANK_FIELD, s_big, Some(&typed));
            if !why.is_empty() {
                // A field, so the reason can be selected and copied, and a
                // `TextArea` so it wraps: a refusal that names a host and a
                // pin does not fit on one line, and what does not fit is
                // scrolled out of a single-line field and out of reach.
                b.text_node(NodeKind::TextArea, BLANK_WHY, s_why, Some(why));
            }
            if shown > 0 {
                b.open(NodeKind::Box, RECENT_BASE - 1, s_recents, shown as u32);
                for (i, r) in self.recents.iter().take(shown).enumerate() {
                    let id = RECENT_BASE + (i as u32) * RECENT_STRIDE;
                    let open = Action::Open(r.url.clone());
                    for n in 0..RECENT_PARTS {
                        self.actions.insert(id + n, open.clone());
                    }
                    let (origin, _) = split_origin_str(&r.url);
                    let host = origin.trim_start_matches("wss://").trim_start_matches("ws://");
                    // The component leads, because it is what differs; the
                    // manifest's name follows it, muted, with the host.
                    let component = component_of(&r.url);
                    let lead = if component.is_empty() { r.name.as_str() } else { component };
                    let under = if component.is_empty() || r.name.is_empty() { host.to_owned() } else { format!("{} · {host}", r.name) };
                    let on = self.picked == Some(i);
                    b.click();
                    b.open(NodeKind::Box, id, if on { s_recent_on } else { s_recent }, 3 + u32::from(on));
                    let letter = lead.chars().next().unwrap_or('*').to_uppercase().to_string();
                    b.open(NodeKind::Box, id + 1, s_sigils.get(tint_index([&r.url, ""])).copied().unwrap_or(s_sigil_off), 1);
                    b.text(id + 2, s_sigil_text, &letter);
                    b.close();
                    b.text(id + 3, s_recent_name, lead);
                    b.text(id + 4, s_recent_where, &under);
                    if on {
                        b.text(id + 5, s_recent_del, "Del");
                    }
                    b.close();
                }
                b.close();
            }
            b.close();
        } else {
            let Some(t) = tabs.get(active) else { return };
            b.open(NodeKind::Box, ADDR, s_addr, 4);
            // Before the address, where a browser has always put them, and
            // in that order: back, forward, then where you are.
            for (id, text_id, on, action) in [(BACK, BACK_TEXT, t.can_back, Action::Back), (FWD, FWD_TEXT, t.can_forward, Action::Forward)] {
                if on {
                    self.actions.insert(id, action.clone());
                    self.actions.insert(text_id, action);
                    b.click();
                }
                b.open(NodeKind::Box, id, if on { s_reload } else { s_step_off }, 1);
                b.icon(text_id, s_step_icon, if id == BACK { "chevron_left" } else { "chevron_right" });
                b.close();
            }
            let chip = t.trust.map(|tr| match tr {
                Trust::Pinned => ("pinned", s_chip_ok),
                Trust::Local => ("local", s_chip_local),
                Trust::Unverified => ("unverified", s_chip_bad),
            });
            self.actions.insert(FIELD, Action::EditAddress);
            self.actions.insert(ORIGIN, Action::EditAddress);
            self.actions.insert(PATH, Action::EditAddress);
            // Before everything else in the field, where a browser puts its
            // padlock: what this application was allowed, reachable without
            // having to remember that it was ever asked. A person who said
            // yes too quickly, or who wants the camera back, has nowhere
            // else to go — the sheet is shown once, before the first
            // connection, and never again on its own.
            if t.grants.is_some() {
                self.actions.insert(GRANTS, Action::Permissions);
                self.actions.insert(GRANTS_TEXT, Action::Permissions);
            }
            // Beside the padlock, which is the other thing in the field
            // that is about the application rather than about the address.
            // An arrow down to put it in the launcher, a tick once it is
            // there — and the tick takes it out again, because the only
            // place a person would look to undo this is the place they
            // did it.
            if let Some(there) = t.installed {
                let what = if there { Action::Uninstall } else { Action::Install };
                self.actions.insert(INSTALL, what.clone());
                self.actions.insert(INSTALL_TEXT, what);
            }
            b.click();
            b.open(
                NodeKind::Box,
                FIELD,
                s_field,
                u32::from(t.grants.is_some()) + u32::from(t.installed.is_some()) + u32::from(chip.is_some()) + u32::from(t.link.is_some()) + if editing { 1 } else { 2 },
            );
            if t.grants.is_some() {
                b.click();
                b.open(NodeKind::Box, GRANTS, s_reload, 1);
                b.icon(GRANTS_TEXT, s_step_icon, "lock");
                b.close();
            }
            if let Some(there) = t.installed {
                b.click();
                b.open(NodeKind::Box, INSTALL, s_reload, 1);
                b.icon(INSTALL_TEXT, s_step_icon, if there { "check" } else { "arrow_down" });
                b.close();
            }
            if let Some((label, s)) = chip {
                b.open(NodeKind::Box, CHIP, s, 1);
                b.text(CHIP_TEXT, s_chip_text, label);
                b.close();
            }
            // The socket, when it is not simply up. It sits beside the
            // trust chip because that is where a person already looks to
            // ask what this address is doing.
            if let Some(word) = t.link {
                b.open(NodeKind::Box, LINK, s_chip_local, 1);
                b.text(LINK_TEXT, s_chip_text, word);
                b.close();
            }
            if editing {
                let mut url = String::with_capacity(t.origin.len() + t.path.len());
                url.push_str(t.origin);
                url.push_str(t.path);
                b.change();
                b.submit();
                b.text_node(NodeKind::Input, INPUT, s_input, Some(&url));
            } else {
                // The origin is legible and the path is muted: the origin
                // is the half of the address a publisher key was pinned to.
                b.click();
                b.text(ORIGIN, s_origin, t.origin);
                b.click();
                b.text(PATH, s_path, t.path);
            }
            b.close();
            // Reload: the address opened again from nothing — a fresh
            // worker, a fresh connection, a fresh tree. A server that was
            // restarted, or a view that failed to render once, leaves
            // nothing else to try.
            self.actions.insert(RELOAD, Action::Reload);
            self.actions.insert(RELOAD_TEXT, Action::Reload);
            b.click();
            b.open(NodeKind::Box, RELOAD, s_reload, 1);
            b.text(RELOAD_TEXT, s_reload_text, "\u{21bb}");
            b.close();
            b.close();

            b.node(NodeKind::Spacer, CONTENT, s_hole, 0);
        }
        b.close();

        self.seq += 1;
        let batch = Batch { seq: self.seq, ops: b.finish(self.defined) };
        self.defined = true;
        if self.driver.handle_frame(Frame::Batch(batch)).is_empty() {
            // The driver answers a mount with an `Ack`, which nobody wants
            // here; an empty answer means it refused the batch.
            crate::driver::trace(|| "chrome: the driver would not take the tree".into());
        }
        // Focus is by node id, and the driver works in tree indices.
        let want = if editing {
            Some(INPUT)
        } else if self.blank {
            // A new tab exists to be typed into, so the first keystroke is
            // already part of an address.
            Some(BLANK_FIELD)
        } else {
            None
        };
        if let Some(ix) = want.and_then(|id| self.driver.session().lookup(id)) {
            let _ = self.driver.focus_node(ix);
        }
    }
}

/// What a session URL calls this particular application: the last segment
/// of its path.
///
/// The manifest's name is the *server's* name, and one server serves many:
/// a Soli application with a gallery, a feed and a music view answers
/// `demo-app` for all three, so a list of them read `demo-app` three
/// times over. The component is the half that differs.
pub fn component_of(url: &str) -> &str {
    let (_, path) = split_origin_str(url);
    path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("")
}

/// The component a **session address** names, or `None` when the address is
/// not one.
///
/// [`component_of`] is the last path segment and nothing more, which is right
/// for a tab's label and wrong for anything that acts on the answer:
/// `wss://host/blog/site` ends in `site`, and a fetch built from that would
/// cheerfully mount a component called `site` that has nothing to do with the
/// address. This requires the whole shape, so a path that is not a session
/// path has no component rather than a plausible one.
pub fn session_component(url: &str) -> Option<&str> {
    let (_, path) = split_origin_str(url);
    let name = path.strip_prefix("/_eui/session/")?.trim_end_matches('/');
    (!name.is_empty() && !name.contains('/')).then_some(name)
}

/// The origin half of a URL, for a sigil's colour and a fallback label.
fn split_origin_str(url: &str) -> (&str, &str) {
    let after = url.find("//").map_or(0, |i| i + 2);
    match url.get(after..).and_then(|rest| rest.find('/')) {
        Some(i) => url.split_at(after + i),
        None => (url, ""),
    }
}

/// The colour a tab's sigil takes, from its origin.
///
/// Identity, not status: two tabs on one host share it, and once four tabs
/// have elided their titles it is the only thing telling them apart. It is
/// drawn from the theme's own tones rather than a palette of this file's,
/// so it turns with the desktop like everything else.
const TINTS: [Role; 5] = [Role::AccentBase, Role::SuccessBase, Role::WarningBase, Role::InfoBase, Role::DangerBase];

fn tint_index(parts: [&str; 2]) -> usize {
    // FNV-1a, for a stable answer across runs and machines — a tab that
    // changed colour between launches would be worse than no colour.
    //
    // Over the whole address, not the origin alone: one server serves many
    // applications, so hashing the host gave every tab of it the same
    // colour — which is exactly the case a sigil exists to disentangle.
    let mut h: u32 = 0x811c_9dc5;
    for byte in parts.iter().flat_map(|p| p.bytes()) {
        h ^= u32::from(byte);
        h = h.wrapping_mul(0x0100_0193);
    }
    (h % TINTS.len() as u32) as usize
}

/// The subtle companion of a tone role, for a chip's fill.
fn subtle_of(tone: Role) -> Role {
    match tone {
        Role::SuccessBase => Role::SuccessSubtle,
        Role::WarningBase => Role::WarningSubtle,
        _ => Role::DangerSubtle,
    }
}

/// Builds a `Subtree` in pre-order, keeping the child counts honest.
///
/// A tree written by hand is a tree written wrong: `child_count` has to
/// match what follows the node, and every miscount is a decode error a long
/// way from the line that caused it. `open`/`close` keeps the count where
/// the children are.
struct Builder {
    styles: Vec<StyleRecord>,
    tree: Subtree,
    /// Indices into `tree.nodes` of the nodes still open, innermost last.
    stack: Vec<usize>,
    /// Handlers staged for the next node pushed.
    staged: Vec<(EventKind, Handler)>,
}

impl Builder {
    fn new() -> Self {
        Self { styles: Vec::new(), tree: Subtree::default(), stack: Vec::new(), staged: Vec::new() }
    }

    fn style(&mut self, r: StyleRecord) -> u32 {
        self.styles.push(r);
        self.styles.len() as u32
    }

    /// The next node pushed answers clicks. The handler names atom 1,
    /// which nothing reads: a chrome click never leaves this process, and
    /// the node id is what identifies the action.
    fn click(&mut self) {
        self.staged.push((EventKind::Click, Handler::Server(1)));
    }

    /// The next node pushed reports its value when an edit settles. Without
    /// it the driver keeps the text and tells nobody, so Enter in the
    /// address bar did nothing at all.
    fn change(&mut self) {
        self.staged.push((EventKind::Change, Handler::Server(1)));
    }

    /// The next node pushed reports Enter. A driver emits an event only for
    /// a node that carries a handler for it, so without this the `Submit`
    /// that Enter produces never left the client — and Enter on an address
    /// nobody had changed did nothing at all.
    fn submit(&mut self) {
        self.staged.push((EventKind::Submit, Handler::Server(1)));
    }

    fn push(&mut self, kind: NodeKind, id: u32, style: u32, text: Option<&str>, children: u32) -> usize {
        let handlers = if self.staged.is_empty() {
            (0, 0)
        } else {
            let at = self.tree.handlers.len() as u32;
            let n = self.staged.len() as u32;
            self.tree.handlers.append(&mut self.staged);
            (at, n)
        };
        self.tree.nodes.push(FlatNode { kind, id, style, key: 0, text: text.map(|t| TextRef::Inline(t.to_owned())), props: (0, 0), handlers, child_count: children });
        self.tree.nodes.len() - 1
    }

    fn open(&mut self, kind: NodeKind, id: u32, style: u32, children: u32) {
        let at = self.push(kind, id, style, None, children);
        self.stack.push(at);
    }

    fn close(&mut self) {
        self.stack.pop();
    }

    fn node(&mut self, kind: NodeKind, id: u32, style: u32, children: u32) {
        self.push(kind, id, style, None, children);
    }

    fn text(&mut self, id: u32, style: u32, s: &str) {
        self.push(NodeKind::Text, id, style, Some(s), 0);
    }

    fn text_node(&mut self, kind: NodeKind, id: u32, style: u32, s: Option<&str>) {
        self.push(kind, id, style, s, 0);
    }

    /// A named vector icon. The name is a prop, not text (03 §1), so it is
    /// stroked from the client's own table rather than shaped — which is
    /// how it comes out the same weight as every other icon in the client,
    /// at whatever size the style asks for.
    fn icon(&mut self, id: u32, style: u32, name: &str) {
        let at = self.push(NodeKind::Icon, id, style, None, 0);
        let first = self.tree.props.len() as u32;
        self.tree.props.push((ATOM_NAME, Value::Str(name.to_owned())));
        if let Some(node) = self.tree.nodes.get_mut(at) {
            node.props = (first, 1);
        }
    }

    /// The batch for this tree.
    ///
    /// `defined` says the session already has the atom and the style
    /// catalogue from an earlier rebuild. It matters: `Op::Mount` replaces
    /// the document and leaves the tables where they are, so defining
    /// either twice is a duplicate and the whole batch is refused — which
    /// left the chrome blank from its second rebuild on, i.e. from the
    /// first time a tab was clicked.
    fn finish(self, defined: bool) -> Vec<Op> {
        if defined {
            return vec![Op::Mount(self.tree)];
        }
        let mut ops: Vec<Op> = self.styles.into_iter().enumerate().map(|(i, record)| Op::DefStyle { id: i as u32 + 1, record }).collect();
        ops.insert(0, Op::DefAtom { id: 1, value: "chrome".into() });
        ops.insert(1, Op::DefAtom { id: ATOM_NAME, value: "name".into() });
        ops.push(Op::Mount(self.tree));
        ops
    }
}

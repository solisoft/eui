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
    /// What the address bar last showed, so a rebuild that changes nothing
    /// does not throw away what is being typed.
    seq: u64,
}

/// Node ids. Fixed for the parts there is one of, and strided for tabs so a
/// click's node id gives back which tab it was without a lookup table for
/// every child.
const ROOT: u32 = 1;
const STRIP: u32 = 2;
const PLUS: u32 = 3;
const PLUS_TEXT: u32 = 17;
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
const TAB_BASE: u32 = 100;
const TAB_STRIDE: u32 = 8;

impl Chrome {
    /// A chrome for a window of `w × h` at `scale`.
    pub fn new(w: f32, h: f32, scale: f32) -> Self {
        Self { driver: Driver::new(w, h, scale, 0), actions: HashMap::new(), editing: false, blank: true, seq: 0 }
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

    /// Whether the chrome wants the next frame drawn.
    pub fn needs_redraw(&self) -> bool {
        self.driver.needs_redraw()
    }

    /// The pointer shape the chrome asks for.
    pub fn cursor(&self) -> eui_proto::Cursor {
        self.driver.cursor()
    }

    /// Where an input method should sit, while the address bar is editing.
    pub fn ime_area(&self) -> Option<eui_layout::Rect> {
        self.driver.ime_area()
    }

    /// This frame's draw list, and the atlases it drew into.
    pub fn paint(&mut self, device_w: u32, device_h: u32) -> DrawList {
        self.driver.paint(device_w, device_h)
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
        let frames = self.driver.input(i);
        let mut out = Vec::new();
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

    /// Put the address bar into editing and select what is there.
    pub fn edit_address(&mut self) {
        self.editing = true;
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
        let s_field = b.style(StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            height: Dim::Px(28),
            grow: 1,
            gap: 2,
            padding: [0, 3, 0, 3],
            radius: 2,
            bg: sunken,
            border_width: [1, 1, 1, 1],
            border_color: role(if editing { Role::FocusRing.id() } else { Role::BorderSubtle.id() }),
            cursor: eui_proto::Cursor::Text,
            overflow: Overflow::Clip,
            ..Default::default()
        });
        let s_chip_text = b.style(StyleRecord { font_size: 0, font_weight: FontWeight::Bold, ..Default::default() });
        let s_origin = b.style(StyleRecord { font_size: 1, font_family: eui_proto::FontFamily::Mono, fg: text, line_clamp: 1, ..Default::default() });
        let s_path = b.style(StyleRecord { font_size: 1, font_family: eui_proto::FontFamily::Mono, fg: muted, grow: 1, shrink: 1, basis: Dim::Px(0), line_clamp: 1, ..Default::default() });
        let s_input = b.style(StyleRecord { font_size: 1, font_family: eui_proto::FontFamily::Mono, fg: text, grow: 1, ..Default::default() });

        // ------------------------------------------------------- the tree
        //
        // Pre-order with child counts, so every subtree is written parent
        // first and the counts below have to match what follows them.
        let tab_count = tabs.len();
        b.open(NodeKind::Box, ROOT, s_root, 2);

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
                let s = if t.trust.is_some() {
                    b.style(StyleRecord {
                        width: Dim::Px(16),
                        height: Dim::Px(16),
                        display: Display::Row,
                        justify: Justify::Center,
                        align_items: AlignItems::Center,
                        radius: 1,
                        bg: role(tint(t.origin).id()),
                        fg: role(Role::TextInverted.id()),
                        ..Default::default()
                    })
                } else {
                    s_sigil_off
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
        if self.blank {
            let s_blank =
                b.style(StyleRecord { display: Display::Column, grow: 1, justify: Justify::Center, align_items: AlignItems::Center, gap: 4, padding: [8, 6, 9, 6], bg: surface, ..Default::default() });
            let s_mark = b.style(StyleRecord {
                width: Dim::Px(44),
                height: Dim::Px(44),
                display: Display::Row,
                justify: Justify::Center,
                align_items: AlignItems::Center,
                radius: 3,
                bg: role(Role::AccentBase.id()),
                fg: role(Role::AccentOn.id()),
                ..Default::default()
            });
            let s_mark_text = b.style(StyleRecord { font_size: 2, font_weight: FontWeight::Bold, ..Default::default() });
            let s_head = b.style(StyleRecord { font_size: 3, font_weight: FontWeight::Bold, fg: text, text_align: TextAlign::Center, ..Default::default() });
            let s_sub = b.style(StyleRecord { font_size: 1, fg: muted, text_align: TextAlign::Center, max_width: Dim::Px(420), ..Default::default() });
            let s_big = b.style(StyleRecord {
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
            });
            b.open(NodeKind::Box, CONTENT, s_blank, 4);
            b.open(NodeKind::Box, BLANK_MARK, s_mark, 1);
            b.text(BLANK_MARK_TEXT, s_mark_text, "EUI");
            b.close();
            b.text(BLANK_HEAD, s_head, "Open an application");
            b.text(BLANK_SUB, s_sub, "Type an address and press Enter.");
            b.text_node(NodeKind::Input, BLANK_FIELD, s_big, Some(""));
            b.close();
        } else {
            let Some(t) = tabs.get(active) else { return };
            b.open(NodeKind::Box, ADDR, s_addr, 1);
            let chip = t.trust.map(|tr| match tr {
                Trust::Pinned => ("pinned", Role::SuccessBase),
                Trust::Local => ("local", Role::WarningBase),
                Trust::Unverified => ("unverified", Role::DangerBase),
            });
            self.actions.insert(FIELD, Action::EditAddress);
            self.actions.insert(ORIGIN, Action::EditAddress);
            self.actions.insert(PATH, Action::EditAddress);
            b.click();
            b.open(NodeKind::Box, FIELD, s_field, if editing { 1 } else { 2 + u32::from(chip.is_some()) });
            if editing {
                let mut url = String::with_capacity(t.origin.len() + t.path.len());
                url.push_str(t.origin);
                url.push_str(t.path);
                b.text_node(NodeKind::Input, INPUT, s_input, Some(&url));
            } else {
                if let Some((label, tone)) = chip {
                    let s = b.style(StyleRecord {
                        display: Display::Row,
                        align_items: AlignItems::Center,
                        height: Dim::Px(20),
                        padding: [0, 2, 0, 2],
                        radius: 1,
                        bg: role(subtle_of(tone).id()),
                        fg: role(tone.id()),
                        ..Default::default()
                    });
                    b.open(NodeKind::Box, CHIP, s, 1);
                    b.text(CHIP_TEXT, s_chip_text, label);
                    b.close();
                }
                // The origin is legible and the path is muted: the origin
                // is the half of the address a publisher key was pinned to.
                b.click();
                b.text(ORIGIN, s_origin, t.origin);
                b.click();
                b.text(PATH, s_path, t.path);
            }
            b.close();
            b.close();

            // The application's own area. It draws nothing — the
            // application's list is composited over it — but it has to be
            // in the tree so the strip and the row are the height they are.
            let s_hole = b.style(StyleRecord { grow: 1, ..Default::default() });
            b.node(NodeKind::Spacer, CONTENT, s_hole, 0);
        }
        b.close();

        self.seq += 1;
        let batch = Batch { seq: self.seq, ops: b.finish() };
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

/// The colour a tab's sigil takes, from its origin.
///
/// Identity, not status: two tabs on one host share it, and once four tabs
/// have elided their titles it is the only thing telling them apart. It is
/// drawn from the theme's own tones rather than a palette of this file's,
/// so it turns with the desktop like everything else.
fn tint(origin: &str) -> Role {
    // FNV-1a, for a stable answer across runs and machines — a tab that
    // changed colour between launches would be worse than no colour.
    let mut h: u32 = 0x811c_9dc5;
    for byte in origin.bytes() {
        h ^= u32::from(byte);
        h = h.wrapping_mul(0x0100_0193);
    }
    match h % 5 {
        0 => Role::AccentBase,
        1 => Role::SuccessBase,
        2 => Role::WarningBase,
        3 => Role::InfoBase,
        _ => Role::DangerBase,
    }
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

    fn finish(self) -> Vec<Op> {
        let mut ops: Vec<Op> = self.styles.into_iter().enumerate().map(|(i, record)| Op::DefStyle { id: i as u32 + 1, record }).collect();
        ops.insert(0, Op::DefAtom { id: 1, value: "chrome".into() });
        ops.push(Op::Mount(self.tree));
        ops
    }
}

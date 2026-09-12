//! Spec 03 §6: the tree an assistive technology sees. Built from the session
//! and the layout on demand — only when a screen reader has asked — as
//! plain data, then handed to the platform through AccessKit: AT-SPI, UIA
//! or AX.
//!
//! The mapping has two halves. A node may **declare** what it is, in props
//! the client reads — `role`, `label`, `checked`, `expanded` and the rest of
//! §6's vocabulary — and where it does not, the kind is **inferred** as it
//! always was: a node with a `click` handler is a button named by the text
//! inside it, editable nodes are text fields whose value is their text, text
//! is a label, the rest are containers.
//!
//! Declaring costs nothing on the wire, because props already travel. It is
//! what lets a checkbox arrive as a checkbox rather than as a button named
//! by its label, and what lets a disabled control still say that it is one:
//! disabling drops the handler map, and without a declared role there would
//! be no button left to infer.
//!
//! Nothing here reaches the server: an assistive technology's actions become
//! the same `Input`s a keyboard produces.
//!
//! The snapshot is plain data so it can cross the process boundary of
//! [`crate::worker`]: the worker builds it, the window hands it to the
//! platform.

use eui_proto::{EventKind, NodeKind, Value};
use eui_tree::NodeIx;

use crate::driver::Driver;

/// What a node is to an assistive technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AccessRole {
    /// The window itself; always node `0`.
    Window = 0,
    /// Something a click activates.
    Button = 1,
    /// A single-line editable field.
    TextInput = 2,
    /// A multi-line editable field.
    MultilineTextInput = 3,
    /// Static text.
    Label = 4,
    /// A picture or icon.
    Image = 5,
    /// A scrolling region.
    ScrollView = 6,
    /// A list.
    List = 7,
    /// Anything else that holds children.
    Container = 8,
    /// A link.
    Link = 9,
    /// A box that can be ticked.
    CheckBox = 10,
    /// One of a set of exclusive choices.
    RadioButton = 11,
    /// The set those choices belong to.
    RadioGroup = 12,
    /// An on/off control.
    Switch = 13,
    /// One tab of a strip.
    Tab = 14,
    /// The strip.
    TabList = 15,
    /// What a tab reveals.
    TabPanel = 16,
    /// A menu.
    Menu = 17,
    /// One of its items.
    MenuItem = 18,
    /// A bar of menus.
    MenuBar = 19,
    /// A field that opens a list.
    ComboBox = 20,
    /// That list.
    ListBox = 21,
    /// One of its options.
    ListBoxOption = 22,
    /// A value dragged along a track.
    Slider = 23,
    /// A value stepped up and down.
    SpinButton = 24,
    /// A progress indicator.
    Progress = 25,
    /// A dialog.
    Dialog = 26,
    /// A dialog that interrupts.
    AlertDialog = 27,
    /// An urgent message.
    Alert = 28,
    /// A message that is not urgent.
    Status = 29,
    /// A tooltip.
    Tooltip = 30,
    /// A tree.
    Tree = 31,
    /// One of its items.
    TreeItem = 32,
    /// A toolbar.
    Toolbar = 33,
    /// A navigation region.
    Navigation = 34,
    /// A table, and its parts.
    Table = 35,
    /// A row of one.
    Row = 36,
    /// A cell of one.
    Cell = 37,
    /// An editable table.
    Grid = 38,
    /// A cell of one.
    GridCell = 39,
    /// A column header.
    ColumnHeader = 40,
    /// A heading, with a level.
    Heading = 41,
    /// A separator, including a split-pane divider.
    Separator = 42,
}

impl AccessRole {
    /// Decode.
    pub const fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Window,
            1 => Self::Button,
            2 => Self::TextInput,
            3 => Self::MultilineTextInput,
            4 => Self::Label,
            5 => Self::Image,
            6 => Self::ScrollView,
            7 => Self::List,
            8 => Self::Container,
            9 => Self::Link,
            10 => Self::CheckBox,
            11 => Self::RadioButton,
            12 => Self::RadioGroup,
            13 => Self::Switch,
            14 => Self::Tab,
            15 => Self::TabList,
            16 => Self::TabPanel,
            17 => Self::Menu,
            18 => Self::MenuItem,
            19 => Self::MenuBar,
            20 => Self::ComboBox,
            21 => Self::ListBox,
            22 => Self::ListBoxOption,
            23 => Self::Slider,
            24 => Self::SpinButton,
            25 => Self::Progress,
            26 => Self::Dialog,
            27 => Self::AlertDialog,
            28 => Self::Alert,
            29 => Self::Status,
            30 => Self::Tooltip,
            31 => Self::Tree,
            32 => Self::TreeItem,
            33 => Self::Toolbar,
            34 => Self::Navigation,
            35 => Self::Table,
            36 => Self::Row,
            37 => Self::Cell,
            38 => Self::Grid,
            39 => Self::GridCell,
            40 => Self::ColumnHeader,
            41 => Self::Heading,
            42 => Self::Separator,
            _ => return None,
        })
    }

    /// The name a server writes in a `role` prop.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "button" => Self::Button,
            "link" => Self::Link,
            "check_box" => Self::CheckBox,
            "radio" => Self::RadioButton,
            "radio_group" => Self::RadioGroup,
            "switch" => Self::Switch,
            "tab" => Self::Tab,
            "tab_list" => Self::TabList,
            "tab_panel" => Self::TabPanel,
            "menu" => Self::Menu,
            "menu_item" => Self::MenuItem,
            "menu_bar" => Self::MenuBar,
            "combo_box" => Self::ComboBox,
            "list_box" => Self::ListBox,
            "option" => Self::ListBoxOption,
            "slider" => Self::Slider,
            "spin_button" => Self::SpinButton,
            "progress" => Self::Progress,
            "dialog" => Self::Dialog,
            "alert_dialog" => Self::AlertDialog,
            "alert" => Self::Alert,
            "status" => Self::Status,
            "tooltip" => Self::Tooltip,
            "tree" => Self::Tree,
            "tree_item" => Self::TreeItem,
            "toolbar" => Self::Toolbar,
            "navigation" => Self::Navigation,
            "table" => Self::Table,
            "row" => Self::Row,
            "cell" => Self::Cell,
            "grid" => Self::Grid,
            "grid_cell" => Self::GridCell,
            "column_header" => Self::ColumnHeader,
            "heading" => Self::Heading,
            "separator" => Self::Separator,
            "group" => Self::Container,
            "label" => Self::Label,
            "image" => Self::Image,
            _ => return None,
        })
    }

    /// Whether this role is named by the text inside it and exposed without
    /// children. True of the things a person activates; false of the things
    /// that *hold* them, which would otherwise swallow their own contents.
    #[must_use]
    pub const fn is_leaf(self) -> bool {
        matches!(self, Self::Button | Self::Link | Self::CheckBox | Self::RadioButton | Self::Switch | Self::Tab | Self::MenuItem | Self::ListBoxOption | Self::TreeItem | Self::ColumnHeader)
    }
}

/// A tri-state tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Checked {
    /// Not ticked.
    No = 0,
    /// Ticked.
    Yes = 1,
    /// Partly ticked: some of what it stands for, not all.
    Mixed = 2,
}

/// What a node declares about itself, beyond its role. Every field is
/// optional and absent by default, so a node that declares nothing is
/// exposed exactly as it was before any of this existed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccessState {
    /// A longer description, read after the name.
    pub description: String,
    /// Ticked, unticked, or partly.
    pub checked: Option<Checked>,
    /// Open or shut, for a disclosure or a combo box's anchor.
    pub expanded: Option<bool>,
    /// Chosen, for a tab, an option or a row.
    pub selected: Option<bool>,
    /// Present but unavailable.
    pub disabled: bool,
    /// Editable in principle, not now.
    pub read_only: bool,
    /// Must be filled in.
    pub required: bool,
    /// Filled in wrongly.
    pub invalid: bool,
    /// Working.
    pub busy: bool,
    /// Owns the window while it is up.
    pub modal: bool,
    /// Where a value sits, and the ends of its range.
    pub value_now: Option<f64>,
    /// The low end.
    pub value_min: Option<f64>,
    /// The high end.
    pub value_max: Option<f64>,
    /// Place in a set, counting from one. `0` is absent.
    pub pos_in_set: u32,
    /// How big that set is, including what virtualisation left out. `0` is absent.
    pub set_size: u32,
    /// Depth, for a heading or a tree item. `0` is absent.
    pub level: u32,
    /// `1` horizontal, `2` vertical, `0` absent.
    pub orientation: u8,
    /// `1` polite, `2` assertive, `0` absent.
    pub live: u8,
}

/// One node of the accessibility tree.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessNode {
    /// Stable id: the arena index plus one; `0` is the window.
    pub id: u64,
    /// Role.
    pub role: AccessRole,
    /// `x, y, w, h` in logical px.
    pub bounds: [f32; 4],
    /// The accessible name.
    pub label: String,
    /// The value, for fields and text.
    pub value: String,
    /// Accepts a Click action.
    pub click: bool,
    /// Accepts a Focus action.
    pub focus: bool,
    /// Accepts the custom "move before" action of 03 §6: there is somewhere
    /// ahead of this node to put it. It does what `Ctrl` with an arrow does,
    /// so nothing an assistive technology can do exceeds what a keyboard user
    /// can — which is the whole of why it may exist at all.
    pub move_prev: bool,
    /// The same, the other way.
    pub move_next: bool,
    /// Children, in order.
    pub children: Vec<u64>,
    /// What the node declared about itself.
    pub state: AccessState,
}

/// The whole tree, as of one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessSnapshot {
    /// Every node, the window last.
    pub nodes: Vec<AccessNode>,
    /// The focused node's id, or `0`.
    pub focus: u64,
    /// The display scale the window applies to bounds.
    pub scale: f32,
}

fn id_of(ix: NodeIx) -> u64 {
    u64::from(ix.raw()).saturating_add(1)
}

impl Driver {
    /// The whole tree, for an assistive technology that just connected or
    /// after a frame changed anything.
    /// Resolve the well-known atoms once, rather than per node per prop.
    fn access_atoms(&self) -> AccessAtoms {
        let s = self.session();
        let id = |n: &str| s.atom_id(n);
        AccessAtoms {
            role: id("role"),
            label: id("label"),
            description: id("description"),
            checked: id("checked"),
            expanded: id("expanded"),
            selected: id("selected"),
            disabled: id("disabled"),
            read_only: id("read_only"),
            required: id("required"),
            invalid: id("invalid"),
            busy: id("busy"),
            modal: id("modal"),
            value_now: id("value_now"),
            value_min: id("value_min"),
            value_max: id("value_max"),
            pos_in_set: id("pos_in_set"),
            set_size: id("set_size"),
            level: id("level"),
            orientation: id("orientation"),
            live: id("live"),
        }
    }

    /// The whole tree, for an assistive technology that just connected or
    /// after a frame changed anything.
    pub fn access_snapshot(&self) -> AccessSnapshot {
        let session = self.session();
        let layout = self.layout();
        let at = self.access_atoms();
        let mut nodes: Vec<AccessNode> = Vec::new();
        let window = |children: Vec<u64>| AccessNode {
            id: 0,
            role: AccessRole::Window,
            bounds: [0.0; 4],
            label: "EUI".into(),
            value: String::new(),
            click: false,
            focus: false,
            move_prev: false,
            move_next: false,
            children,
            state: AccessState::default(),
        };
        let Some(root) = session.root() else {
            return AccessSnapshot { nodes: vec![window(Vec::new())], focus: 0, scale: self.scale() };
        };
        let mut stack = vec![root];
        while let Some(ix) = stack.pop() {
            let Some(node) = session.node(ix) else { continue };
            let Some(rect) = layout.rect(ix) else { continue };
            if layout.is_virtual(ix) {
                continue;
            }
            let state = at.state(node);
            // A declared role wins; a name this client does not know falls
            // back to the kind, which is the forward-compatibility rule the
            // rest of the vocabulary keeps.
            let declared = at.role.and_then(|a| node.prop(a)).and_then(str_of).and_then(AccessRole::from_name);
            // Disabling a control drops its handler map, so `activatable`
            // alone would lose it. Saying `disabled` keeps it a control.
            let activatable = node.handler(EventKind::Click).is_some();
            let role = declared.unwrap_or(match node.kind {
                _ if activatable => AccessRole::Button,
                NodeKind::Input => AccessRole::TextInput,
                NodeKind::TextArea => AccessRole::MultilineTextInput,
                NodeKind::Text => AccessRole::Label,
                NodeKind::Image | NodeKind::Icon => AccessRole::Image,
                NodeKind::Scroll => AccessRole::ScrollView,
                NodeKind::List => AccessRole::List,
                _ => AccessRole::Container,
            });
            let mut a = AccessNode {
                id: id_of(ix),
                role,
                bounds: [rect.x, rect.y, rect.w, rect.h],
                label: String::new(),
                value: String::new(),
                click: false,
                focus: false,
                move_prev: false,
                move_next: false,
                children: Vec::new(),
                state,
            };
            let editable = matches!(node.kind, NodeKind::Input | NodeKind::TextArea);
            if editable {
                a.value = session.text_of(ix).unwrap_or("").to_owned();
                a.focus = !a.state.disabled;
            } else if role.is_leaf() {
                // A leaf is named by everything written inside it: the text
                // is the name, not a child.
                let label: Vec<&str> = session.preorder(ix).filter_map(|n| session.text_of(n)).collect();
                a.label = label.join(" ");
                a.click = activatable;
                a.focus = activatable;
            } else if node.kind == NodeKind::Text {
                let text = session.text_of(ix).unwrap_or("");
                a.label = text.to_owned();
                a.value = text.to_owned();
            }
            if !editable && !role.is_leaf() {
                a.children = session.children(ix).iter().filter(|c| layout.rect(**c).is_some() && !layout.is_virtual(**c)).map(|c| id_of(*c)).collect();
                stack.extend(session.children(ix).iter().rev());
                if activatable {
                    a.click = true;
                    a.focus = true;
                }
            }
            // 03 §6: a node that can be picked up offers the two moves, and
            // only where there is somewhere to go. `pos_in_set`/`set_size`
            // carry the rest of the story, and the announcement is the
            // server's through a `live` node — prose is content.
            if session.atoms().drag.is_some_and(|at| node.prop(at).is_some_and(|v| !matches!(v, Value::Bool(false)))) {
                let siblings = session.children(node.parent);
                let at = siblings.iter().position(|c| *c == ix);
                a.move_prev = at.is_some_and(|i| i > 0);
                a.move_next = at.is_some_and(|i| i + 1 < siblings.len());
                a.focus = true;
            }
            // A declared label overrides the text gathered from inside.
            if let Some(text) = at.label.and_then(|x| node.prop(x)).and_then(str_of) {
                a.label = text.to_owned();
            }
            // Unavailable: it keeps its role and its name, and accepts nothing.
            if a.state.disabled {
                a.click = false;
                a.focus = false;
            }
            nodes.push(a);
        }
        nodes.push(window(vec![id_of(root)]));
        AccessSnapshot { nodes, focus: self.focused().map_or(0, id_of), scale: self.scale() }
    }

    /// The node an assistive technology named, if it is still in the tree.
    pub fn node_for_accessibility(&self, id: u64) -> Option<NodeIx> {
        let raw = u32::try_from(id.checked_sub(1)?).ok()?;
        let root = self.session().root()?;
        self.session().preorder(root).find(|ix| ix.raw() == raw)
    }
}

/// The atoms §6's vocabulary uses, resolved once per snapshot.
#[derive(Debug, Clone, Copy, Default)]
struct AccessAtoms {
    role: Option<u32>,
    label: Option<u32>,
    description: Option<u32>,
    checked: Option<u32>,
    expanded: Option<u32>,
    selected: Option<u32>,
    disabled: Option<u32>,
    read_only: Option<u32>,
    required: Option<u32>,
    invalid: Option<u32>,
    busy: Option<u32>,
    modal: Option<u32>,
    value_now: Option<u32>,
    value_min: Option<u32>,
    value_max: Option<u32>,
    pos_in_set: Option<u32>,
    set_size: Option<u32>,
    level: Option<u32>,
    orientation: Option<u32>,
    live: Option<u32>,
}

fn str_of(v: &eui_proto::Value) -> Option<&str> {
    match v {
        eui_proto::Value::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

fn bool_of(v: &eui_proto::Value) -> Option<bool> {
    match v {
        eui_proto::Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn num_of(v: &eui_proto::Value) -> Option<f64> {
    match v {
        eui_proto::Value::Int(i) => Some(*i as f64),
        eui_proto::Value::Float(f) => Some(*f),
        _ => None,
    }
}

impl AccessAtoms {
    fn flag(self, node: &eui_tree::Node, atom: Option<u32>) -> bool {
        atom.and_then(|a| node.prop(a)).and_then(bool_of).unwrap_or(false)
    }

    fn tri(self, node: &eui_tree::Node, atom: Option<u32>) -> Option<bool> {
        atom.and_then(|a| node.prop(a)).and_then(bool_of)
    }

    fn count(self, node: &eui_tree::Node, atom: Option<u32>) -> u32 {
        atom.and_then(|a| node.prop(a)).and_then(num_of).map_or(0, |n| if n > 0.0 { n as u32 } else { 0 })
    }

    fn state(self, node: &eui_tree::Node) -> AccessState {
        let checked = self.checked.and_then(|a| node.prop(a)).and_then(|v| match v {
            eui_proto::Value::Bool(true) => Some(Checked::Yes),
            eui_proto::Value::Bool(false) => Some(Checked::No),
            eui_proto::Value::Str(s) if s == "mixed" => Some(Checked::Mixed),
            _ => None,
        });
        let word = |atom: Option<u32>, a: &str, b: &str| -> u8 {
            match atom.and_then(|x| node.prop(x)).and_then(str_of) {
                Some(s) if s == a => 1,
                Some(s) if s == b => 2,
                _ => 0,
            }
        };
        AccessState {
            description: self.description.and_then(|a| node.prop(a)).and_then(str_of).unwrap_or_default().to_owned(),
            checked,
            expanded: self.tri(node, self.expanded),
            selected: self.tri(node, self.selected),
            disabled: self.flag(node, self.disabled),
            read_only: self.flag(node, self.read_only),
            required: self.flag(node, self.required),
            invalid: self.flag(node, self.invalid),
            busy: self.flag(node, self.busy),
            modal: self.flag(node, self.modal),
            value_now: self.value_now.and_then(|a| node.prop(a)).and_then(num_of),
            value_min: self.value_min.and_then(|a| node.prop(a)).and_then(num_of),
            value_max: self.value_max.and_then(|a| node.prop(a)).and_then(num_of),
            pos_in_set: self.count(node, self.pos_in_set),
            set_size: self.count(node, self.set_size),
            level: self.count(node, self.level),
            orientation: word(self.orientation, "horizontal", "vertical"),
            live: word(self.live, "polite", "assertive"),
        }
    }
}

/// The snapshot in AccessKit's terms, for the platform adapter. The window
/// wraps the root and carries the display scale, so bounds stay in logical
/// px like everything else in the client.
#[cfg(has_a11y)]
pub fn to_update(snapshot: &AccessSnapshot) -> accesskit::TreeUpdate {
    use accesskit::{Action, Affine, CustomAction, Invalid, Live, Node, NodeId, Orientation, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate};
    let mut nodes: Vec<(NodeId, Node)> = Vec::with_capacity(snapshot.nodes.len());
    for n in &snapshot.nodes {
        let role = match n.role {
            AccessRole::Window => Role::Window,
            AccessRole::Button => Role::Button,
            AccessRole::TextInput => Role::TextInput,
            AccessRole::MultilineTextInput => Role::MultilineTextInput,
            AccessRole::Label => Role::Label,
            AccessRole::Image => Role::Image,
            AccessRole::ScrollView => Role::ScrollView,
            AccessRole::List => Role::List,
            AccessRole::Container => Role::GenericContainer,
            AccessRole::Link => Role::Link,
            AccessRole::CheckBox => Role::CheckBox,
            AccessRole::RadioButton => Role::RadioButton,
            AccessRole::RadioGroup => Role::RadioGroup,
            AccessRole::Switch => Role::Switch,
            AccessRole::Tab => Role::Tab,
            AccessRole::TabList => Role::TabList,
            AccessRole::TabPanel => Role::TabPanel,
            AccessRole::Menu => Role::Menu,
            AccessRole::MenuItem => Role::MenuItem,
            AccessRole::MenuBar => Role::MenuBar,
            AccessRole::ComboBox => Role::ComboBox,
            AccessRole::ListBox => Role::ListBox,
            AccessRole::ListBoxOption => Role::ListBoxOption,
            AccessRole::Slider => Role::Slider,
            AccessRole::SpinButton => Role::SpinButton,
            AccessRole::Progress => Role::ProgressIndicator,
            AccessRole::Dialog => Role::Dialog,
            AccessRole::AlertDialog => Role::AlertDialog,
            AccessRole::Alert => Role::Alert,
            AccessRole::Status => Role::Status,
            AccessRole::Tooltip => Role::Tooltip,
            AccessRole::Tree => Role::Tree,
            AccessRole::TreeItem => Role::TreeItem,
            AccessRole::Toolbar => Role::Toolbar,
            AccessRole::Navigation => Role::Navigation,
            AccessRole::Table => Role::Table,
            AccessRole::Row => Role::Row,
            AccessRole::Cell => Role::Cell,
            AccessRole::Grid => Role::Grid,
            AccessRole::GridCell => Role::GridCell,
            AccessRole::ColumnHeader => Role::ColumnHeader,
            AccessRole::Heading => Role::Heading,
            AccessRole::Separator => Role::Splitter,
        };
        let mut a = Node::new(role);
        if n.role == AccessRole::Window {
            a.set_label(n.label.as_str());
            a.set_transform(Affine::scale(f64::from(snapshot.scale)));
        } else {
            let [x, y, w, h] = n.bounds;
            a.set_bounds(Rect { x0: f64::from(x), y0: f64::from(y), x1: f64::from(x + w), y1: f64::from(y + h) });
            if !n.label.is_empty() {
                a.set_label(n.label.as_str());
            }
            if !n.value.is_empty() || matches!(n.role, AccessRole::TextInput | AccessRole::MultilineTextInput) {
                a.set_value(n.value.as_str());
            }
        }
        if n.click {
            a.add_action(Action::Click);
        }
        if n.focus {
            a.add_action(Action::Focus);
        }
        // 03 §6: the two moves, as the only vocabulary the platform has left
        // for them. The indices are what comes back in `ActionData`, and they
        // are the action bytes the driver expects.
        let mut moves = Vec::new();
        if n.move_prev {
            moves.push(CustomAction { id: 2, description: "Move before".into() });
        }
        if n.move_next {
            moves.push(CustomAction { id: 3, description: "Move after".into() });
        }
        if !moves.is_empty() {
            a.add_action(Action::CustomAction);
            a.set_custom_actions(moves);
        }
        let st = &n.state;
        if !st.description.is_empty() {
            a.set_description(st.description.as_str());
        }
        match st.checked {
            Some(Checked::Yes) => a.set_toggled(Toggled::True),
            Some(Checked::No) => a.set_toggled(Toggled::False),
            Some(Checked::Mixed) => a.set_toggled(Toggled::Mixed),
            None => {}
        }
        if let Some(v) = st.expanded {
            a.set_expanded(v);
        }
        if let Some(v) = st.selected {
            a.set_selected(v);
        }
        if st.disabled {
            a.set_disabled();
        }
        if st.read_only {
            a.set_read_only();
        }
        if st.required {
            a.set_required();
        }
        if st.invalid {
            a.set_invalid(Invalid::True);
        }
        if st.busy {
            a.set_busy();
        }
        if st.modal {
            a.set_modal();
        }
        if let Some(v) = st.value_now {
            a.set_numeric_value(v);
        }
        if let Some(v) = st.value_min {
            a.set_min_numeric_value(v);
        }
        if let Some(v) = st.value_max {
            a.set_max_numeric_value(v);
        }
        if st.pos_in_set > 0 {
            a.set_position_in_set(st.pos_in_set as usize);
        }
        if st.set_size > 0 {
            a.set_size_of_set(st.set_size as usize);
        }
        if st.level > 0 {
            a.set_level(st.level as usize);
        }
        match st.orientation {
            1 => a.set_orientation(Orientation::Horizontal),
            2 => a.set_orientation(Orientation::Vertical),
            _ => {}
        }
        match st.live {
            1 => a.set_live(Live::Polite),
            2 => a.set_live(Live::Assertive),
            _ => {}
        }
        if !n.children.is_empty() {
            a.set_children(n.children.iter().map(|c| NodeId(*c)).collect::<Vec<_>>());
        }
        nodes.push((NodeId(n.id), a));
    }
    TreeUpdate { nodes, tree: Some(TreeInfo::new(NodeId(0))), tree_id: TreeId::ROOT, focus: NodeId(snapshot.focus) }
}

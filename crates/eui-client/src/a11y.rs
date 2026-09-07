//! Spec 03 §6: the tree an assistive technology sees. Built from the session
//! and the layout on demand — only when a screen reader has asked — as
//! plain data, then handed to the platform through AccessKit: AT-SPI, UIA
//! or AX.
//!
//! The mapping is deliberately small. A node with a `click` handler is a
//! button named by the text inside it; editable nodes are text fields whose
//! value is their text; text is a label; the rest are containers. Nothing
//! here reaches the server: an assistive technology's actions become the
//! same `Input`s a keyboard produces.
//!
//! The snapshot is plain data so it can cross the process boundary of
//! [`crate::worker`]: the worker builds it, the window hands it to the
//! platform.

use eui_proto::{EventKind, NodeKind};
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
            _ => return None,
        })
    }
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
    /// Children, in order.
    pub children: Vec<u64>,
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
    pub fn access_snapshot(&self) -> AccessSnapshot {
        let session = self.session();
        let layout = self.layout();
        let mut nodes: Vec<AccessNode> = Vec::new();
        let window = |children: Vec<u64>| AccessNode { id: 0, role: AccessRole::Window, bounds: [0.0; 4], label: "EUI".into(), value: String::new(), click: false, focus: false, children };
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
            let activatable = node.handler(EventKind::Click).is_some();
            let role = match node.kind {
                _ if activatable => AccessRole::Button,
                NodeKind::Input => AccessRole::TextInput,
                NodeKind::TextArea => AccessRole::MultilineTextInput,
                NodeKind::Text => AccessRole::Label,
                NodeKind::Image | NodeKind::Icon => AccessRole::Image,
                NodeKind::Scroll => AccessRole::ScrollView,
                NodeKind::List => AccessRole::List,
                _ => AccessRole::Container,
            };
            let mut a = AccessNode { id: id_of(ix), role, bounds: [rect.x, rect.y, rect.w, rect.h], label: String::new(), value: String::new(), click: false, focus: false, children: Vec::new() };
            match node.kind {
                _ if activatable => {
                    // A button is named by everything written inside it and
                    // is a leaf: the text is the name, not a child.
                    let label: Vec<&str> = session.preorder(ix).filter_map(|n| session.text_of(n)).collect();
                    a.label = label.join(" ");
                    a.click = true;
                    a.focus = true;
                }
                NodeKind::Input | NodeKind::TextArea => {
                    a.value = session.text_of(ix).unwrap_or("").to_owned();
                    a.focus = true;
                }
                NodeKind::Text => {
                    let text = session.text_of(ix).unwrap_or("");
                    a.label = text.to_owned();
                    a.value = text.to_owned();
                }
                _ => {
                    a.children = session.children(ix).iter().filter(|c| layout.rect(**c).is_some() && !layout.is_virtual(**c)).map(|c| id_of(*c)).collect();
                    stack.extend(session.children(ix).iter().rev());
                }
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

/// The snapshot in AccessKit's terms, for the platform adapter. The window
/// wraps the root and carries the display scale, so bounds stay in logical
/// px like everything else in the client.
#[cfg(feature = "a11y")]
pub fn to_update(snapshot: &AccessSnapshot) -> accesskit::TreeUpdate {
    use accesskit::{Action, Affine, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};
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
        if !n.children.is_empty() {
            a.set_children(n.children.iter().map(|c| NodeId(*c)).collect::<Vec<_>>());
        }
        nodes.push((NodeId(n.id), a));
    }
    TreeUpdate { nodes, tree: Some(TreeInfo::new(NodeId(0))), tree_id: TreeId::ROOT, focus: NodeId(snapshot.focus) }
}

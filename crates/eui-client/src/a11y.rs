//! Spec 03 §6: the tree an assistive technology sees. Built from the session
//! and the layout on demand — only when a screen reader has asked — and
//! handed to the platform through AccessKit: AT-SPI, UIA or AX.
//!
//! The mapping is deliberately small. A node with a `click` handler is a
//! button named by the text inside it; editable nodes are text fields whose
//! value is their text; text is a label; the rest are containers. Nothing
//! here reaches the server: an assistive technology's actions become the
//! same `Input`s a keyboard produces.

use accesskit::{Action, Affine, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};
use eui_proto::{EventKind, NodeKind};
use eui_tree::NodeIx;

use crate::driver::Driver;

fn id_of(ix: NodeIx) -> NodeId {
    NodeId(u64::from(ix.raw()).saturating_add(1))
}

impl Driver {
    /// The whole tree, for an assistive technology that just connected or
    /// after a frame changed anything.
    pub fn accessibility_tree(&self) -> TreeUpdate {
        let session = self.session();
        let layout = self.layout();
        let mut nodes: Vec<(NodeId, Node)> = Vec::new();
        let Some(root) = session.root() else {
            let mut window = Node::new(Role::Window);
            window.set_label("EUI");
            return TreeUpdate { nodes: vec![(NodeId(0), window)], tree: Some(TreeInfo::new(NodeId(0))), tree_id: TreeId::ROOT, focus: NodeId(0) };
        };
        let mut stack = vec![root];
        while let Some(ix) = stack.pop() {
            let Some(node) = session.node(ix) else { continue };
            let Some(rect) = layout.rect(ix) else { continue };
            if layout.is_virtual(ix) {
                continue;
            }
            let activatable = node.handler(EventKind::Click).is_some();
            let mut a = match node.kind {
                _ if activatable => Role::Button,
                NodeKind::Input => Role::TextInput,
                NodeKind::TextArea => Role::MultilineTextInput,
                NodeKind::Text => Role::Label,
                NodeKind::Image | NodeKind::Icon => Role::Image,
                NodeKind::Scroll => Role::ScrollView,
                NodeKind::List => Role::List,
                _ => Role::GenericContainer,
            }
            .pipe(Node::new);
            a.set_bounds(Rect { x0: f64::from(rect.x), y0: f64::from(rect.y), x1: f64::from(rect.x + rect.w), y1: f64::from(rect.y + rect.h) });
            match node.kind {
                _ if activatable => {
                    // A button is named by everything written inside it and
                    // is a leaf: the text is the name, not a child.
                    let label: Vec<&str> = session.preorder(ix).filter_map(|n| session.text_of(n)).collect();
                    a.set_label(label.join(" "));
                    a.add_action(Action::Click);
                    a.add_action(Action::Focus);
                }
                NodeKind::Input | NodeKind::TextArea => {
                    a.set_value(session.text_of(ix).unwrap_or(""));
                    a.add_action(Action::Focus);
                }
                NodeKind::Text => {
                    let text = session.text_of(ix).unwrap_or("");
                    a.set_label(text);
                    a.set_value(text);
                }
                _ => {
                    let children: Vec<NodeId> = session.children(ix).iter().filter(|c| layout.rect(**c).is_some() && !layout.is_virtual(**c)).map(|c| id_of(*c)).collect();
                    a.set_children(children);
                    stack.extend(session.children(ix).iter().rev());
                }
            }
            nodes.push((id_of(ix), a));
        }
        // The window wraps the root and carries the display scale, so bounds
        // stay in logical px like everything else in the client.
        let mut window = Node::new(Role::Window);
        window.set_label("EUI");
        window.set_transform(Affine::scale(f64::from(self.scale())));
        window.set_children(vec![id_of(root)]);
        nodes.push((NodeId(0), window));
        let focus = self.focused().map_or(NodeId(0), id_of);
        TreeUpdate { nodes, tree: Some(TreeInfo::new(NodeId(0))), tree_id: TreeId::ROOT, focus }
    }

    /// The node an assistive technology named, if it is still in the tree.
    pub fn node_for_accessibility(&self, id: NodeId) -> Option<NodeIx> {
        let raw = u32::try_from(id.0.checked_sub(1)?).ok()?;
        let root = self.session().root()?;
        self.session().preorder(root).find(|ix| ix.raw() == raw)
    }
}

/// `x.pipe(f)`: `f(x)`, so a `match` can feed a constructor.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

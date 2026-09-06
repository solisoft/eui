//! Nodes, values, handlers, and the flat subtree they decode into.
//!
//! A subtree decodes into pre-order arrays, not into a tree of boxes. That is
//! not a micro-optimisation: it is what lets the decoder run **iteratively**,
//! with an explicit work stack, so that a 10 000-deep hostile tree costs a
//! bounds check rather than the call stack.

use crate::error::{DecodeError, Result};
use crate::limits::{
    HASH_BYTES, MAX_CHILDREN, MAX_HANDLERS, MAX_INLINE_STR, MAX_NODES, MAX_PROPS, MAX_TREE_DEPTH,
    MAX_VALUE_DEPTH, MAX_VALUE_LIST,
};
use crate::reader::Reader;
use crate::style::ColorRef;
use crate::writer::Writer;

/// The closed set of primitive node kinds (`spec/02-wire-format.md` §4.1).
///
/// Everything a user would call a widget — button, dialog, table, date picker —
/// is composed from these on the server. That is the whole reason the widget
/// catalogue can grow without shipping a new client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NodeKind {
    /// A styled rectangle that arranges children.
    Box = 0x01,
    /// A run of text. Leaf.
    Text = 0x02,
    /// A raster image, referenced by content hash.
    Image = 0x03,
    /// A vector glyph from the icon set. Leaf.
    Icon = 0x04,
    /// A single-line editable field.
    Input = 0x05,
    /// A multi-line editable field.
    TextArea = 0x06,
    /// A clipping viewport with scroll offsets.
    Scroll = 0x07,
    /// A virtualised child list: only the visible window is laid out.
    List = 0x08,
    /// A retained path list, for charts and custom marks.
    Canvas = 0x09,
    /// Flexible empty space. Leaf.
    Spacer = 0x0A,
    /// A hairline rule. Leaf.
    Divider = 0x0B,
    /// A layer above the normal flow: menus, tooltips, dialogs.
    Overlay = 0x0C,
    /// A named insertion point for composed content.
    Slot = 0x0D,
    /// An invisible box that only imposes constraints.
    Sizer = 0x0E,
}

impl NodeKind {
    /// Decode from the wire.
    pub const fn from_u8(v: u8) -> Result<Self> {
        match v {
            0x01 => Ok(Self::Box),
            0x02 => Ok(Self::Text),
            0x03 => Ok(Self::Image),
            0x04 => Ok(Self::Icon),
            0x05 => Ok(Self::Input),
            0x06 => Ok(Self::TextArea),
            0x07 => Ok(Self::Scroll),
            0x08 => Ok(Self::List),
            0x09 => Ok(Self::Canvas),
            0x0A => Ok(Self::Spacer),
            0x0B => Ok(Self::Divider),
            0x0C => Ok(Self::Overlay),
            0x0D => Ok(Self::Slot),
            0x0E => Ok(Self::Sizer),
            _ => Err(DecodeError::UnknownTag("node kind")),
        }
    }

    /// The wire discriminant.
    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// True for kinds that MUST NOT have children.
    pub const fn is_leaf(self) -> bool {
        matches!(self, Self::Text | Self::Icon | Self::Spacer | Self::Divider)
    }

    /// True for kinds that carry no text, props, or handlers.
    pub const fn is_inert(self) -> bool {
        matches!(self, Self::Spacer | Self::Divider)
    }
}

/// Input and lifecycle events (`spec/06-events.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EventKind {
    /// Primary activation.
    Click = 0x01,
    /// Two activations within the platform's double-click interval.
    DoubleClick = 0x02,
    /// A pointer button went down.
    PointerDown = 0x03,
    /// A pointer button came up.
    PointerUp = 0x04,
    /// The pointer moved within the node. Coalesced per frame.
    PointerMove = 0x05,
    /// The pointer entered the node.
    PointerEnter = 0x06,
    /// The pointer left the node.
    PointerLeave = 0x07,
    /// A key went down.
    KeyDown = 0x08,
    /// A key came up.
    KeyUp = 0x09,
    /// Composed text was committed, IME included.
    TextInput = 0x0A,
    /// The node took focus.
    Focus = 0x0B,
    /// The node lost focus.
    Blur = 0x0C,
    /// An editable node's value settled.
    Change = 0x0D,
    /// A form asked to be submitted.
    Submit = 0x0E,
    /// A scroll offset changed. Coalesced per frame.
    Scroll = 0x0F,
    /// The node's box changed size.
    Resize = 0x10,
    /// Secondary activation.
    ContextMenu = 0x11,
    /// A drag began on the node.
    DragStart = 0x12,
    /// A drag is hovering the node.
    DragOver = 0x13,
    /// A drag was released on the node.
    Drop = 0x14,
    /// A press was held past the platform's long-press interval.
    LongPress = 0x15,
}

impl EventKind {
    /// Decode from the wire.
    pub const fn from_u8(v: u8) -> Result<Self> {
        match v {
            0x01 => Ok(Self::Click),
            0x02 => Ok(Self::DoubleClick),
            0x03 => Ok(Self::PointerDown),
            0x04 => Ok(Self::PointerUp),
            0x05 => Ok(Self::PointerMove),
            0x06 => Ok(Self::PointerEnter),
            0x07 => Ok(Self::PointerLeave),
            0x08 => Ok(Self::KeyDown),
            0x09 => Ok(Self::KeyUp),
            0x0A => Ok(Self::TextInput),
            0x0B => Ok(Self::Focus),
            0x0C => Ok(Self::Blur),
            0x0D => Ok(Self::Change),
            0x0E => Ok(Self::Submit),
            0x0F => Ok(Self::Scroll),
            0x10 => Ok(Self::Resize),
            0x11 => Ok(Self::ContextMenu),
            0x12 => Ok(Self::DragStart),
            0x13 => Ok(Self::DragOver),
            0x14 => Ok(Self::Drop),
            0x15 => Ok(Self::LongPress),
            _ => Err(DecodeError::UnknownTag("event kind")),
        }
    }

    /// The wire discriminant.
    pub const fn to_u8(self) -> u8 {
        self as u8
    }
}

/// A string: interned in the session's atom table, or carried inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextRef {
    /// An atom id.
    Atom(u32),
    /// A one-off string, at most [`MAX_INLINE_STR`] bytes.
    Inline(String),
}

impl TextRef {
    /// Decode.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0x00 => Ok(Self::Atom(r.varint32()?)),
            0x01 => Ok(Self::Inline(r.str(MAX_INLINE_STR, "inline string")?.to_owned())),
            _ => Err(DecodeError::UnknownTag("TextRef")),
        }
    }

    /// Encode.
    pub fn encode(&self, w: &mut Writer) {
        match self {
            Self::Atom(id) => {
                w.u8(0x00).varint32(*id);
            }
            Self::Inline(s) => {
                w.u8(0x01).str(s);
            }
        }
    }
}

/// A property value (`spec/02-wire-format.md` §4.4).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Absent.
    Null,
    /// Boolean.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// Finite float. NaN and infinities are rejected at decode.
    Float(f64),
    /// An atom id.
    Atom(u32),
    /// An inline string.
    Str(String),
    /// A BLAKE3 content hash naming an asset.
    Asset([u8; HASH_BYTES]),
    /// A colour role or literal.
    Color(ColorRef),
    /// A homogeneous or mixed list, nested at most [`MAX_VALUE_DEPTH`] deep.
    List(Vec<Value>),
}

impl Value {
    /// Decode, tracking nesting so a self-similar list cannot recurse away the
    /// stack.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Self::decode_at(r, 1)
    }

    fn decode_at(r: &mut Reader<'_>, depth: u32) -> Result<Self> {
        if depth > MAX_VALUE_DEPTH {
            return Err(DecodeError::LimitExceeded("value nesting"));
        }
        match r.u8()? {
            0x00 => Ok(Self::Null),
            0x01 => match r.u8()? {
                0 => Ok(Self::Bool(false)),
                1 => Ok(Self::Bool(true)),
                _ => Err(DecodeError::IllegalValue("bool must be 0 or 1")),
            },
            0x02 => Ok(Self::Int(r.svarint()?)),
            0x03 => Ok(Self::Float(r.f64()?)),
            0x04 => Ok(Self::Atom(r.varint32()?)),
            0x05 => Ok(Self::Str(r.str(MAX_INLINE_STR, "inline string")?.to_owned())),
            0x06 => {
                Ok(Self::Asset(r.array::<HASH_BYTES>()?))
            }
            0x07 => Ok(Self::Color(ColorRef(r.u16()?))),
            0x08 => {
                let count = r.varint32_max(MAX_VALUE_LIST, "value list length")?;
                // Bounded above, so reserving up front is safe and saves the
                // regrowth churn on the common small list.
                let mut items = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    items.push(Self::decode_at(r, depth.saturating_add(1))?);
                }
                Ok(Self::List(items))
            }
            _ => Err(DecodeError::UnknownTag("Value")),
        }
    }

    /// Encode.
    pub fn encode(&self, w: &mut Writer) {
        match self {
            Self::Null => {
                w.u8(0x00);
            }
            Self::Bool(b) => {
                w.u8(0x01).u8(u8::from(*b));
            }
            Self::Int(n) => {
                w.u8(0x02).svarint(*n);
            }
            Self::Float(f) => {
                w.u8(0x03).f64(*f);
            }
            Self::Atom(id) => {
                w.u8(0x04).varint32(*id);
            }
            Self::Str(s) => {
                w.u8(0x05).str(s);
            }
            Self::Asset(h) => {
                w.u8(0x06).raw(h);
            }
            Self::Color(c) => {
                w.u8(0x07).u16(c.0);
            }
            Self::List(items) => {
                w.u8(0x08).varint32(items.len() as u32);
                for item in items {
                    item.encode(w);
                }
            }
        }
    }
}

/// What an event does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    /// Round-trip: the client emits an `Event` frame naming this atom.
    Server(u32),
    /// Runs entirely on the client, in the metered VM. No network traffic.
    Local(u32),
    /// Runs locally for the immediate feedback, then tells the server.
    LocalThenServer {
        /// Bytecode chunk id.
        chunk: u32,
        /// Atom naming the server-side event.
        name: u32,
    },
}

impl Handler {
    /// Decode.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0x00 => Ok(Self::Server(r.varint32()?)),
            0x01 => Ok(Self::Local(r.varint32()?)),
            0x02 => Ok(Self::LocalThenServer {
                chunk: r.varint32()?,
                name: r.varint32()?,
            }),
            _ => Err(DecodeError::UnknownTag("Handler")),
        }
    }

    /// Encode.
    pub fn encode(&self, w: &mut Writer) {
        match self {
            Self::Server(name) => {
                w.u8(0x00).varint32(*name);
            }
            Self::Local(chunk) => {
                w.u8(0x01).varint32(*chunk);
            }
            Self::LocalThenServer { chunk, name } => {
                w.u8(0x02).varint32(*chunk).varint32(*name);
            }
        }
    }
}

/// One node, with its props and handlers held as ranges into the owning
/// [`Subtree`]'s side arrays.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatNode {
    /// Primitive kind.
    pub kind: NodeKind,
    /// Server-assigned, non-zero, unique while the node exists.
    pub id: u32,
    /// Style table id.
    pub style: u32,
    /// Identity for list reconciliation; 0 means positional.
    pub key: u32,
    /// Text content, if any.
    pub text: Option<TextRef>,
    /// `(start, len)` into [`Subtree::props`].
    pub props: (u32, u32),
    /// `(start, len)` into [`Subtree::handlers`].
    pub handlers: (u32, u32),
    /// Number of immediate children, which follow this node in pre-order.
    pub child_count: u32,
}

/// A subtree, stored pre-order.
///
/// Reconstructing the shape needs nothing but `child_count`: a node's first
/// child is the next entry, and its next sibling is found by skipping that
/// child's own descendants.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Subtree {
    /// Nodes in pre-order. `nodes[0]` is the root.
    pub nodes: Vec<FlatNode>,
    /// `(property atom, value)` pairs, sliced by [`FlatNode::props`].
    pub props: Vec<(u32, Value)>,
    /// `(event, handler)` pairs, sliced by [`FlatNode::handlers`].
    pub handlers: Vec<(EventKind, Handler)>,
}

impl Subtree {
    /// The root node, or `None` for an empty subtree.
    pub fn root(&self) -> Option<&FlatNode> {
        self.nodes.first()
    }

    /// The properties of `node`.
    pub fn props_of(&self, node: &FlatNode) -> &[(u32, Value)] {
        let (start, len) = node.props;
        self.props
            .get(start as usize..(start as usize).saturating_add(len as usize))
            .unwrap_or(&[])
    }

    /// The handlers of `node`.
    pub fn handlers_of(&self, node: &FlatNode) -> &[(EventKind, Handler)] {
        let (start, len) = node.handlers;
        self.handlers
            .get(start as usize..(start as usize).saturating_add(len as usize))
            .unwrap_or(&[])
    }

    /// Decode one subtree.
    ///
    /// Iterative by construction: `pending` holds, for each open ancestor, how
    /// many of its children are still to come. Depth is checked before a push,
    /// so `MAX_TREE_DEPTH` bounds the *stack vector*, and the real call stack
    /// never grows with the tree at all.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let mut out = Self::default();
        let mut pending: Vec<u32> = Vec::new();

        loop {
            if out.nodes.len() as u32 >= MAX_NODES {
                return Err(DecodeError::LimitExceeded("node count"));
            }
            let child_count = out.decode_one(r)?;

            if child_count > 0 {
                if pending.len() as u32 >= MAX_TREE_DEPTH {
                    return Err(DecodeError::LimitExceeded("tree depth"));
                }
                pending.push(child_count);
            } else {
                // Close every ancestor whose last child this was.
                while let Some(top) = pending.last_mut() {
                    *top = top.saturating_sub(1);
                    if *top == 0 {
                        pending.pop();
                    } else {
                        break;
                    }
                }
            }

            if pending.is_empty() {
                return Ok(out);
            }
        }
    }

    /// Decode a single node into `self`, returning its declared child count.
    fn decode_one(&mut self, r: &mut Reader<'_>) -> Result<u32> {
        let kind = NodeKind::from_u8(r.u8()?)?;
        let flags = r.u8()?;
        if flags & 0xF0 != 0 {
            return Err(DecodeError::IllegalValue("reserved node flags set"));
        }

        let id = r.varint32()?;
        if id == 0 {
            return Err(DecodeError::IllegalValue("node id must be non-zero"));
        }
        let style = r.varint32()?;

        let key = if flags & 0x01 != 0 { r.varint32()? } else { 0 };
        let text = if flags & 0x02 != 0 { Some(TextRef::decode(r)?) } else { None };

        let props_start = self.props.len() as u32;
        let mut props_len = 0u32;
        if flags & 0x04 != 0 {
            let count = r.varint32_max(MAX_PROPS, "props per node")?;
            for _ in 0..count {
                let prop = r.varint32()?;
                let value = Value::decode(r)?;
                self.props.push((prop, value));
            }
            props_len = count;
        }

        let handlers_start = self.handlers.len() as u32;
        let mut handlers_len = 0u32;
        if flags & 0x08 != 0 {
            let count = r.varint32_max(MAX_HANDLERS, "handlers per node")?;
            for _ in 0..count {
                let event = EventKind::from_u8(r.u8()?)?;
                let handler = Handler::decode(r)?;
                self.handlers.push((event, handler));
            }
            handlers_len = count;
        }

        if kind.is_inert() && (text.is_some() || props_len > 0 || handlers_len > 0) {
            return Err(DecodeError::IllegalValue("inert node kind carries content"));
        }

        let child_count = r.varint32_max(MAX_CHILDREN, "children per node")?;
        if kind.is_leaf() && child_count > 0 {
            return Err(DecodeError::NotALeaf);
        }

        self.nodes.push(FlatNode {
            kind,
            id,
            style,
            key,
            text,
            props: (props_start, props_len),
            handlers: (handlers_start, handlers_len),
            child_count,
        });
        Ok(child_count)
    }

    /// Encode the subtree, pre-order.
    pub fn encode(&self, w: &mut Writer) {
        for node in &self.nodes {
            let props = self.props_of(node);
            let handlers = self.handlers_of(node);

            let mut flags = 0u8;
            if node.key != 0 {
                flags |= 0x01;
            }
            if node.text.is_some() {
                flags |= 0x02;
            }
            if !props.is_empty() {
                flags |= 0x04;
            }
            if !handlers.is_empty() {
                flags |= 0x08;
            }

            w.u8(node.kind.to_u8())
                .u8(flags)
                .varint32(node.id)
                .varint32(node.style);

            if flags & 0x01 != 0 {
                w.varint32(node.key);
            }
            if let Some(text) = &node.text {
                text.encode(w);
            }
            if flags & 0x04 != 0 {
                w.varint32(props.len() as u32);
                for (prop, value) in props {
                    w.varint32(*prop);
                    value.encode(w);
                }
            }
            if flags & 0x08 != 0 {
                w.varint32(handlers.len() as u32);
                for (event, handler) in handlers {
                    w.u8(event.to_u8());
                    handler.encode(w);
                }
            }
            w.varint32(node.child_count);
        }
    }
}

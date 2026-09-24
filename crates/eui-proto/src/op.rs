//! Batch operations (`spec/02-wire-format.md` §5).
//!
//! These are structural only. Whether an op is *coherent* — that the node it
//! names exists, that the atom it references was defined — is session state,
//! and belongs to `eui-tree`. Keeping the two apart is what lets this crate be
//! fuzzed against raw bytes with no setup.

use crate::error::{DecodeError, Result};
use crate::limits::{HASH_BYTES, MAX_ATOM_BYTES, MAX_CHUNK_BYTES, MAX_FACES_PER_ROLE, MAX_FONT_ROLE, MAX_NOTIFY_BODY, MAX_NOTIFY_PER_BATCH, MAX_NOTIFY_TAG, MAX_NOTIFY_TITLE, MAX_OPS_PER_BATCH};
use crate::node::{EventKind, Handler, Subtree, TextRef, Value};
use crate::reader::Reader;
use crate::style::{Gradient, StyleRecord};
use crate::writer::Writer;

/// One operation in a batch.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Intern a string for the rest of the session.
    DefAtom {
        /// Atom id, non-zero.
        id: u32,
        /// The string.
        value: String,
    },
    /// Define a computed style record.
    DefStyle {
        /// Style id, non-zero.
        id: u32,
        /// The record.
        record: StyleRecord,
    },
    /// Define a literal colour.
    DefColor {
        /// Colour id, non-zero.
        id: u32,
        /// `0xRRGGBBAA`, sRGB.
        rgba: u32,
    },
    /// Name a bytecode chunk by content hash.
    DefChunk {
        /// Chunk id, non-zero.
        id: u32,
        /// BLAKE3 of the chunk asset.
        hash: [u8; HASH_BYTES],
    },
    /// Bind a font role to the faces that draw it (`spec/02-wire-format.md` §5).
    ///
    /// The faces are assets, fetched and hash-checked like any other; until
    /// they arrive the role draws in `sans`. Roles `0` and `1` are the
    /// client's own sans and mono, and binding one replaces it for this
    /// session only — which is what a theme's `font_sans` asset means (05 §3).
    DefFont {
        /// Font role, at most [`MAX_FONT_ROLE`].
        role: u8,
        /// BLAKE3 of each face, at least one and at most
        /// [`MAX_FACES_PER_ROLE`]. `font_weight` chooses among them.
        faces: Vec<[u8; HASH_BYTES]>,
    },
    /// Define a linear gradient a record's `bg` may name
    /// (`spec/02-wire-format.md` §5.3). Version 6.
    DefGradient {
        /// Gradient id, non-zero, at most [`MAX_GRADIENTS`](crate::limits::MAX_GRADIENTS).
        id: u32,
        /// The angle and the stops.
        gradient: Gradient,
    },
    /// Deliver a bytecode chunk inline (`spec/07-bytecode.md` §2).
    DefChunkBytes {
        /// Chunk id, non-zero.
        id: u32,
        /// The chunk, at most [`MAX_CHUNK_BYTES`].
        bytes: Vec<u8>,
    },
    /// Replace the whole document. The session's tables — atoms, styles,
    /// colours, chunks, font roles — are not cleared: they outlive the tree
    /// that referenced them, so a second page costs no second `DefAtom`
    /// (02 §5).
    Mount(Subtree),
    /// Replace one node and its descendants.
    Replace {
        /// Node to replace.
        node: u32,
        /// Its replacement.
        subtree: Subtree,
    },
    /// Point a node at a different style record.
    SetStyle {
        /// Node.
        node: u32,
        /// New style id.
        style: u32,
    },
    /// Change a node's text.
    SetText {
        /// Node.
        node: u32,
        /// New text.
        text: TextRef,
    },
    /// Set one property.
    SetProp {
        /// Node.
        node: u32,
        /// Property name atom.
        prop: u32,
        /// New value.
        value: Value,
    },
    /// Insert a subtree among a node's children.
    InsertChild {
        /// Parent.
        parent: u32,
        /// Position among existing children.
        index: u32,
        /// What to insert.
        subtree: Subtree,
    },
    /// Remove a contiguous run of children.
    RemoveChild {
        /// Parent.
        parent: u32,
        /// First child to remove.
        index: u32,
        /// How many.
        count: u32,
    },
    /// Move one child within its parent.
    ///
    /// This is what makes keyed reconciliation cheap: reordering a
    /// thousand-row table is *n* moves, not a rebuild.
    MoveChild {
        /// Parent.
        parent: u32,
        /// Current position.
        from: u32,
        /// Target position.
        to: u32,
    },
    /// Attach a handler.
    SetHandler {
        /// Node.
        node: u32,
        /// Which event.
        event: EventKind,
        /// What it does.
        handler: Handler,
    },
    /// Detach a handler.
    ClearHandler {
        /// Node.
        node: u32,
        /// Which event.
        event: EventKind,
    },
    /// Move keyboard focus.
    Focus {
        /// Node to focus.
        node: u32,
    },
    /// Set a scroll viewport's offsets.
    ScrollTo {
        /// A `scroll` or `list` node.
        node: u32,
        /// Horizontal offset in px.
        x: i64,
        /// Vertical offset in px.
        y: i64,
    },
    /// Say one line to the person through the machine they are using
    /// (`spec/02-wire-format.md` §5.2).
    ///
    /// The only op that names no node: what it changes is not the document
    /// but what somebody is told, and it is the only one a client may
    /// decline outright -- without `notifications` (01 §2.1) it is decoded,
    /// counted and dropped. Nothing goes back either way, so an application
    /// learns neither that it was shown nor that it was not.
    Notify {
        /// The line, at most [`MAX_NOTIFY_TITLE`] bytes. Required in
        /// practice: a notification with no title is a blank rectangle.
        title: String,
        /// What is under it, at most [`MAX_NOTIFY_BODY`] bytes. May be
        /// empty.
        body: String,
        /// An identity, at most [`MAX_NOTIFY_TAG`] bytes. A second
        /// notification carrying the tag of one still on screen replaces
        /// it rather than stacking beside it -- ten replies to one thread
        /// are one notification. Empty means this one stands alone.
        tag: String,
    },
}

impl Op {
    /// Decode one op, opcode included.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0x10 => Ok(Self::DefAtom { id: nonzero(r.varint32()?, "atom id")?, value: r.str(MAX_ATOM_BYTES, "atom value")?.to_owned() }),
            0x11 => Ok(Self::DefStyle { id: nonzero(r.varint32()?, "style id")?, record: StyleRecord::decode(r)? }),
            0x12 => Ok(Self::DefColor { id: nonzero(r.varint32()?, "color id")?, rgba: r.u32()? }),
            0x13 => {
                let id = nonzero(r.varint32()?, "chunk id")?;
                Ok(Self::DefChunk { id, hash: r.array::<HASH_BYTES>()? })
            }
            0x14 => Ok(Self::DefChunkBytes { id: nonzero(r.varint32()?, "chunk id")?, bytes: r.bytes(MAX_CHUNK_BYTES, "chunk bytes")?.to_vec() }),
            0x15 => {
                let role = r.u8()?;
                if role > MAX_FONT_ROLE {
                    return Err(DecodeError::LimitExceeded("font role"));
                }
                let count = r.varint32()?;
                // A role with no face is not an empty binding, it is a
                // sentence that stops halfway: the style says "draw this in
                // role 2" and nothing ever says what role 2 is. Rejected
                // here so the session never holds one.
                if count == 0 {
                    return Err(DecodeError::IllegalValue("font faces"));
                }
                if count > MAX_FACES_PER_ROLE {
                    return Err(DecodeError::LimitExceeded("font faces"));
                }
                let mut faces = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    faces.push(r.array::<HASH_BYTES>()?);
                }
                Ok(Self::DefFont { role, faces })
            }
            0x16 => Ok(Self::DefGradient { id: nonzero(r.varint32()?, "gradient id")?, gradient: Gradient::decode(r)? }),
            0x20 => Ok(Self::Mount(Subtree::decode(r)?)),
            0x21 => Ok(Self::Replace { node: nonzero(r.varint32()?, "node id")?, subtree: Subtree::decode(r)? }),
            0x22 => Ok(Self::SetStyle { node: nonzero(r.varint32()?, "node id")?, style: r.varint32()? }),
            0x23 => Ok(Self::SetText { node: nonzero(r.varint32()?, "node id")?, text: TextRef::decode(r)? }),
            0x24 => Ok(Self::SetProp { node: nonzero(r.varint32()?, "node id")?, prop: r.varint32()?, value: Value::decode(r)? }),
            0x25 => Ok(Self::InsertChild { parent: nonzero(r.varint32()?, "node id")?, index: r.varint32()?, subtree: Subtree::decode(r)? }),
            0x26 => Ok(Self::RemoveChild { parent: nonzero(r.varint32()?, "node id")?, index: r.varint32()?, count: r.varint32()? }),
            0x27 => Ok(Self::MoveChild { parent: nonzero(r.varint32()?, "node id")?, from: r.varint32()?, to: r.varint32()? }),
            0x28 => Ok(Self::SetHandler { node: nonzero(r.varint32()?, "node id")?, event: EventKind::from_u8(r.u8()?)?, handler: Handler::decode(r)? }),
            0x29 => Ok(Self::ClearHandler { node: nonzero(r.varint32()?, "node id")?, event: EventKind::from_u8(r.u8()?)? }),
            0x2A => Ok(Self::Focus { node: nonzero(r.varint32()?, "node id")? }),
            0x2B => Ok(Self::ScrollTo { node: nonzero(r.varint32()?, "node id")?, x: r.svarint()?, y: r.svarint()? }),
            0x2C => Ok(Self::Notify {
                title: r.str(MAX_NOTIFY_TITLE, "notification title")?.to_owned(),
                body: r.str(MAX_NOTIFY_BODY, "notification body")?.to_owned(),
                tag: r.str(MAX_NOTIFY_TAG, "notification tag")?.to_owned(),
            }),
            _ => Err(DecodeError::UnknownTag("opcode")),
        }
    }

    /// Encode one op, opcode included.
    pub fn encode(&self, w: &mut Writer) {
        match self {
            Self::DefAtom { id, value } => {
                w.u8(0x10).varint32(*id).str(value);
            }
            Self::DefStyle { id, record } => {
                w.u8(0x11).varint32(*id);
                record.encode(w);
            }
            Self::DefColor { id, rgba } => {
                w.u8(0x12).varint32(*id).u32(*rgba);
            }
            Self::DefChunk { id, hash } => {
                w.u8(0x13).varint32(*id).raw(hash);
            }
            Self::DefChunkBytes { id, bytes } => {
                w.u8(0x14).varint32(*id).bytes(bytes);
            }
            Self::DefFont { role, faces } => {
                w.u8(0x15).u8(*role).varint32(faces.len() as u32);
                for face in faces {
                    w.raw(face);
                }
            }
            Self::DefGradient { id, gradient } => {
                w.u8(0x16).varint32(*id);
                gradient.encode(w);
            }
            Self::Mount(subtree) => {
                w.u8(0x20);
                subtree.encode(w);
            }
            Self::Replace { node, subtree } => {
                w.u8(0x21).varint32(*node);
                subtree.encode(w);
            }
            Self::SetStyle { node, style } => {
                w.u8(0x22).varint32(*node).varint32(*style);
            }
            Self::SetText { node, text } => {
                w.u8(0x23).varint32(*node);
                text.encode(w);
            }
            Self::SetProp { node, prop, value } => {
                w.u8(0x24).varint32(*node).varint32(*prop);
                value.encode(w);
            }
            Self::InsertChild { parent, index, subtree } => {
                w.u8(0x25).varint32(*parent).varint32(*index);
                subtree.encode(w);
            }
            Self::RemoveChild { parent, index, count } => {
                w.u8(0x26).varint32(*parent).varint32(*index).varint32(*count);
            }
            Self::MoveChild { parent, from, to } => {
                w.u8(0x27).varint32(*parent).varint32(*from).varint32(*to);
            }
            Self::SetHandler { node, event, handler } => {
                w.u8(0x28).varint32(*node).u8(event.to_u8());
                handler.encode(w);
            }
            Self::ClearHandler { node, event } => {
                w.u8(0x29).varint32(*node).u8(event.to_u8());
            }
            Self::Focus { node } => {
                w.u8(0x2A).varint32(*node);
            }
            Self::ScrollTo { node, x, y } => {
                w.u8(0x2B).varint32(*node).svarint(*x).svarint(*y);
            }
            Self::Notify { title, body, tag } => {
                w.u8(0x2C).str(title).str(body).str(tag);
            }
        }
    }
}

/// An ordered run of ops carrying a sequence number.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Batch {
    /// Monotonically increasing. The client acks the last one it applied.
    pub seq: u64,
    /// Applied in order, all or nothing.
    pub ops: Vec<Op>,
}

impl Batch {
    /// Decode a batch payload.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let seq = r.varint()?;
        let count = r.varint32_max(MAX_OPS_PER_BATCH, "ops per batch")?;
        let mut ops = Vec::with_capacity((count as usize).min(1024));
        // The one op counted here rather than in `eui-tree`: every other
        // limit bounds what the client must hold, and holding four
        // notifications costs nothing. What this bounds is how often one
        // batch may interrupt somebody, which is not session state and has
        // no natural ceiling anywhere else (02 §5.2).
        let mut notifications: u32 = 0;
        for _ in 0..count {
            let op = Op::decode(r)?;
            if matches!(op, Op::Notify { .. }) {
                notifications = notifications.saturating_add(1);
                if notifications > MAX_NOTIFY_PER_BATCH {
                    return Err(DecodeError::LimitExceeded("notifications per batch"));
                }
            }
            ops.push(op);
        }
        Ok(Self { seq, ops })
    }

    /// Encode a batch payload.
    pub fn encode(&self, w: &mut Writer) {
        w.varint(self.seq).varint32(self.ops.len() as u32);
        for op in &self.ops {
            op.encode(w);
        }
    }
}

fn nonzero(v: u32, what: &'static str) -> Result<u32> {
    if v == 0 {
        Err(DecodeError::IllegalValue(what))
    } else {
        Ok(v)
    }
}

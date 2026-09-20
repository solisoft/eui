//! Protocol limits, normative in `spec/02-wire-format.md` §6.
//!
//! Every one of these is enforced during decoding, before the memory it would
//! bound is allocated. A hostile server can be annoying; it cannot make the
//! client exhaust itself.

/// Largest frame payload accepted, in bytes.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// Deepest node nesting accepted.
pub const MAX_TREE_DEPTH: u32 = 256;
/// Most nodes a session may hold.
pub const MAX_NODES: u32 = 1_000_000;
/// Highest atom id.
pub const MAX_ATOMS: u32 = 65_535;
/// Largest single atom value, in bytes.
pub const MAX_ATOM_BYTES: usize = 64 * 1024;
/// Largest total of all atom values in a session, in bytes.
pub const MAX_ATOM_TOTAL_BYTES: usize = 8 * 1024 * 1024;
/// Highest style id.
pub const MAX_STYLES: u32 = 65_535;
/// Highest literal colour id.
pub const MAX_COLORS: u32 = 4_095;
/// Highest bytecode chunk id.
pub const MAX_CHUNKS: u32 = 4_095;
/// Most children a single node may have.
pub const MAX_CHILDREN: u32 = 65_535;
/// Most properties a single node may carry.
pub const MAX_PROPS: u32 = 64;
/// Most handlers a single node may carry.
pub const MAX_HANDLERS: u32 = 16;
/// Most ops a single batch may carry.
pub const MAX_OPS_PER_BATCH: u32 = 65_535;
/// Largest inline (non-interned) string, in bytes.
pub const MAX_INLINE_STR: usize = 4 * 1024;
/// Deepest nesting of `Value::List`.
pub const MAX_VALUE_DEPTH: u32 = 4;
/// Most elements in a `Value::List`: a windowed list (04 §7.1) sends one
/// integer per row, and a feed has many rows. Bounded in bytes by
/// [`MAX_FRAME_BYTES`] regardless.
pub const MAX_VALUE_LIST: u32 = 1_000_000;

/// Largest inline bytecode chunk, in bytes.
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;

/// Largest `bytes` field of one `Upload` or `Blob` frame (01 §6). Small
/// enough that a file moves as a stream the window can interleave with
/// drawing, rather than as one message it must hold twice.
pub const MAX_TRANSFER_CHUNK_BYTES: usize = 256 * 1024;
/// Largest single upload the protocol allows, in bytes. A node's `pick`
/// prop may ask for less; nothing may ask for more.
pub const MAX_UPLOAD_BYTES: u64 = 64 * 1024 * 1024;
/// Largest upload a `pick` that names no ceiling of its own accepts.
pub const DEFAULT_UPLOAD_BYTES: u64 = 16 * 1024 * 1024;
/// Largest file a server may write through one `save`, in bytes. The
/// person chose where it goes; they did not agree to fill the disk.
pub const MAX_SAVE_BYTES: u64 = 256 * 1024 * 1024;
/// Largest reason string on an aborted transfer, in bytes.
pub const MAX_ABORT_REASON: usize = 256;

/// Largest title on a `Notify` op (02 §5.2), in bytes. A notification is a
/// line somebody reads on their way past, and every platform truncates a
/// long one anyway -- so the protocol truncates nothing and refuses this
/// instead, which an application finds out about while it is being written
/// rather than on a machine whose notifier is stricter than the last one.
pub const MAX_NOTIFY_TITLE: usize = 256;
/// Largest body on a `Notify` op, in bytes.
pub const MAX_NOTIFY_BODY: usize = 1024;
/// Largest tag on a `Notify` op, in bytes. It is an identity, not prose:
/// long enough for a record's key, short enough that it is never a message.
pub const MAX_NOTIFY_TAG: usize = 64;
/// Most `Notify` ops one batch may carry (02 §5.2).
///
/// The one op whose cost is not the client's memory but the person's
/// attention, and the only defence against a server that spends it all at
/// once is a number. Four is a burst -- a message, a reply and two more --
/// past which a batch is not notifying somebody, it is shouting.
pub const MAX_NOTIFY_PER_BATCH: u32 = 4;

/// Size of a `StyleRecord` on the wire, in bytes.
pub const STYLE_RECORD_BYTES: usize = 64;
/// Size of a BLAKE3 asset hash, in bytes.
pub const HASH_BYTES: usize = 32;

/// Highest font role an application may bind (02 §5). Roles `0` and `1` are
/// the client's own sans and mono, which a `DefFont` may replace; `2..=9` are
/// the application's to name. Eight is a typographic system, not a font
/// menu: past that the session is shipping a catalogue, and every face is
/// bytes the viewer waits for.
pub const MAX_FONT_ROLE: u8 = 9;
/// Most faces one role may carry — a weight and a slant each, twice over.
/// `font_weight` selects among them; nothing else does.
pub const MAX_FACES_PER_ROLE: u32 = 8;

/// Most frames one `application/vnd.eui.frames` body may carry (01 §2.4).
///
/// [`MAX_FRAME_BYTES`](crate::limits::MAX_FRAME_BYTES) bounds one frame and
/// nothing bounded a *count*, because nothing ever handed a client a
/// concatenation before. A body of two-byte frames at that ceiling is eight
/// million iterations before the first pixel.
pub const MAX_VIEW_FRAMES: usize = 64;

/// Most bytes one such body may be.
pub const MAX_VIEW_BYTES: usize = 4 * 1024 * 1024;

/// Most sessions one page may open for its islands (01 §2.7, 10 §1).
///
/// A ceiling and not a target: most pages want none and a page that wants
/// one wants one. It is here because **a tree is data** — a view that
/// derived an island per row would open a socket per row, and a client that
/// obeyed it would be doing the one thing the whole feature exists to avoid.
/// Past it a client opens no more and leaves those nodes as they were
/// rendered, which is the same degradation as an island whose session never
/// answers: content that is out of date rather than missing.
pub const MAX_ISLANDS: usize = 8;

//! What a module from the network may be, as numbers.
//!
//! Every one of these is a cap on *shape*, settled before the module is
//! compiled and without running anything. That is the whole idea: the
//! bytecode verifier bounds a chunk's steps at run time with fuel, and a
//! shader has no run time it can be stopped in — a fragment stage is
//! already on the GPU when it misbehaves. So the bound has to be proved
//! from the shape, or not at all.

/// The source a module may be, in bytes.
///
/// The same number as `eui-proto`'s `MAX_CHUNK_BYTES`, and for the same
/// reason: it is the size at which a thing the network sends stops being a
/// handler and starts being a program.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Steps one fragment invocation may take.
///
/// The same number as `eui-vm`'s fuel, deliberately. A chunk gets 4 096
/// steps because that is enough to decide something and not enough to sit
/// in the worker; a fragment gets 4 096 because the same sentence is true
/// of a GPU. Two verifiers, one budget, one argument.
///
/// **What this does not bound**, and the spec has to say so: it counts
/// steps per invocation. It does not count invocations — that is the size
/// of the target, which the client owns — and it does not price a step:
/// a dependent texture fetch on a cold cache is not one cycle. The real
/// cost is `steps × fragments × hardware`, and only the first factor is
/// here.
pub const MAX_STEPS: u64 = 4_096;

/// Trips a single loop may take, before nesting is multiplied in.
///
/// Redundant with [`MAX_STEPS`] for any loop with a body, and not
/// redundant for an empty one, which would otherwise be free.
pub const MAX_LOOP_TRIPS: u64 = 4_096;

/// Functions a module may declare, entry points included.
pub const MAX_FUNCTIONS: usize = 64;

/// Statements a module may contain, across every function.
pub const MAX_STATEMENTS: usize = 4_096;

/// Expressions a module may contain, across every function.
pub const MAX_EXPRESSIONS: usize = 16_384;

/// Types a module may declare.
pub const MAX_TYPES: usize = 256;

/// The one binding a module may have: the client's uniform block.
///
/// Not a convention the server is asked to follow — the only one it is
/// allowed. A module that declares a second binding is refused rather than
/// bound to something the client did not offer.
pub const UNIFORM_GROUP: u32 = 0;
/// The binding within [`UNIFORM_GROUP`].
pub const UNIFORM_BINDING: u32 = 0;

/// The uniform block's size in bytes, which fixes its type.
///
/// The server does not define this struct; it receives it. Checking the
/// size is how a module that declared its own is caught here rather than
/// at pipeline creation, where the message would come from the driver and
/// name the driver (08 §8).
pub const UNIFORM_BYTES: u32 = 128;

/// The fragment entry point's name. Fixed, so that the client never reads a
/// name — or a `@location` — that the server chose.
pub const FRAGMENT_ENTRY: &str = "fs_main";

/// The vertex entry point's name, where the module brings its own.
pub const VERTEX_ENTRY: &str = "vs_main";

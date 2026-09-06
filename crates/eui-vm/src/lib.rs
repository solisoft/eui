//! # EUI local-handler VM
//!
//! The only code a client runs that it did not ship with, and therefore the
//! most constrained thing in the protocol (`spec/07-bytecode.md`). A chunk
//! can read and write the component's local state, set a node's text or a
//! prop, and queue a server event. It cannot do anything else, because the
//! [`Host`] trait has no other methods.
//!
//! - [`Chunk::verify`] decodes every instruction, checks every jump lands on
//!   an instruction boundary, and proves the operand stack never underflows
//!   or exceeds the declared maximum along any path. A chunk that fails is
//!   never run.
//! - [`run`] interprets a verified chunk with a fuel budget; a type error or
//!   fuel exhaustion aborts the run, and the host is told nothing about what
//!   would have followed.
//!
//! No `unsafe`, no dependencies beyond `eui-proto`'s reader.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

use eui_proto::Reader;

/// Largest chunk accepted.
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
/// Deepest operand stack a chunk may declare.
pub const MAX_STACK: u8 = 64;
/// Instructions a single run may execute.
pub const FUEL: u32 = 4096;
/// Longest string a chunk may build.
pub const MAX_STR: usize = 4 * 1024;

const MAGIC: &[u8; 4] = b"EUIC";

/// A value on the operand stack or in local state.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Absent.
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i64),
    /// String.
    Str(String),
}

/// Why a chunk was rejected or a run aborted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VmError {
    /// Verification: bad magic, version, size, or truncated instruction.
    Malformed(&'static str),
    /// Verification: an opcode the protocol does not define.
    UnknownOp(u8),
    /// Verification: a jump outside the code or between instructions.
    BadJump,
    /// Verification: stack underflow, overflow, or inconsistent depth.
    Stack(&'static str),
    /// Run: an operand of the wrong type.
    Type(&'static str),
    /// Run: out of fuel.
    Fuel,
    /// Run: a string grew past [`MAX_STR`].
    StringTooLong,
    /// Run: the host refused an effect (unknown node, no tree).
    Host(&'static str),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(w) => write!(f, "malformed chunk: {w}"),
            Self::UnknownOp(op) => write!(f, "unknown opcode {op:#04x}"),
            Self::BadJump => f.write_str("jump outside the code or between instructions"),
            Self::Stack(w) => write!(f, "stack: {w}"),
            Self::Type(w) => write!(f, "type error: {w}"),
            Self::Fuel => f.write_str("out of fuel"),
            Self::StringTooLong => f.write_str("string too long"),
            Self::Host(w) => write!(f, "host refused: {w}"),
        }
    }
}

impl std::error::Error for VmError {}

/// One decoded instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Instr {
    PushInt(i64),
    PushStr(u32),
    PushBool(bool),
    Load(u32),
    Store(u32),
    Dup,
    Pop,
    Add,
    Sub,
    Mul,
    Neg,
    Not,
    Eq,
    Lt,
    Gt,
    And,
    Or,
    ToStr,
    Concat,
    /// Absolute target index into the instruction list, resolved by the verifier.
    Jump(usize),
    /// Absolute target index into the instruction list, resolved by the verifier.
    JumpIfFalse(usize),
    SetText(u32),
    SetProp(u32, u32),
    /// Point the node with this key at a style table id, locally.
    SetStyle(u32, u32),
    Emit(u32),
    Return,
}

impl Instr {
    /// `(pops, pushes)`.
    const fn effect(self) -> (u8, u8) {
        match self {
            Self::PushInt(_) | Self::PushStr(_) | Self::PushBool(_) | Self::Load(_) => (0, 1),
            Self::Store(_) | Self::Pop | Self::JumpIfFalse(_) | Self::SetText(_) | Self::SetProp(..) => (1, 0),
            Self::SetStyle(..) => (0, 0),
            Self::Dup => (1, 2),
            Self::Add | Self::Sub | Self::Mul | Self::Eq | Self::Lt | Self::Gt | Self::And | Self::Or | Self::Concat => (2, 1),
            Self::Neg | Self::Not | Self::ToStr => (1, 1),
            Self::Jump(_) | Self::Emit(_) | Self::Return => (0, 0),
        }
    }
}

/// A verified chunk, ready to run any number of times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    code: Vec<Instr>,
    max_stack: u8,
}

impl Chunk {
    /// Decode and verify (`spec/07-bytecode.md` §4).
    pub fn verify(bytes: &[u8]) -> Result<Self, VmError> {
        if bytes.len() > MAX_CHUNK_BYTES {
            return Err(VmError::Malformed("chunk too large"));
        }
        let mut r = Reader::new(bytes);
        if r.array::<4>().map_err(|_| VmError::Malformed("truncated"))? != *MAGIC {
            return Err(VmError::Malformed("bad magic"));
        }
        if r.u8().map_err(|_| VmError::Malformed("truncated"))? != 1 {
            return Err(VmError::Malformed("unsupported version"));
        }
        let max_stack = r.u8().map_err(|_| VmError::Malformed("truncated"))?;
        if max_stack > MAX_STACK {
            return Err(VmError::Stack("declared depth above the limit"));
        }
        let code_start = r.position();

        // Pass 1: decode, recording each instruction's byte offset.
        let mut code = Vec::new();
        let mut offsets = Vec::new();
        let mut raw_jumps: Vec<(usize, i64)> = Vec::new(); // (instr index, target byte offset)
        let trunc = |_| VmError::Malformed("truncated operand");
        while !r.is_empty() {
            offsets.push(r.position().saturating_sub(code_start));
            let op = r.u8().map_err(trunc)?;
            let instr = match op {
                0x01 => Instr::PushInt(r.svarint().map_err(trunc)?),
                0x02 => Instr::PushStr(r.varint32().map_err(trunc)?),
                0x03 => match r.u8().map_err(trunc)? {
                    0 => Instr::PushBool(false),
                    1 => Instr::PushBool(true),
                    _ => return Err(VmError::Malformed("bool operand")),
                },
                0x04 => Instr::Load(r.varint32().map_err(trunc)?),
                0x05 => Instr::Store(r.varint32().map_err(trunc)?),
                0x06 => Instr::Dup,
                0x07 => Instr::Pop,
                0x10 => Instr::Add,
                0x11 => Instr::Sub,
                0x12 => Instr::Mul,
                0x13 => Instr::Neg,
                0x14 => Instr::Not,
                0x15 => Instr::Eq,
                0x16 => Instr::Lt,
                0x17 => Instr::Gt,
                0x18 => Instr::And,
                0x19 => Instr::Or,
                0x1A => Instr::ToStr,
                0x1B => Instr::Concat,
                0x20 | 0x21 => {
                    let rel = i64::from(i16::from_le_bytes(r.array::<2>().map_err(trunc)?));
                    let next = r.position().saturating_sub(code_start) as i64;
                    raw_jumps.push((code.len(), next.saturating_add(rel)));
                    if op == 0x20 { Instr::Jump(usize::MAX) } else { Instr::JumpIfFalse(usize::MAX) }
                }
                0x30 => Instr::SetText(r.varint32().map_err(trunc)?),
                0x31 => {
                    let node = r.varint32().map_err(trunc)?;
                    let atom = r.varint32().map_err(trunc)?;
                    Instr::SetProp(node, atom)
                }
                0x32 => Instr::Emit(r.varint32().map_err(trunc)?),
                0x33 => {
                    let key = r.varint32().map_err(trunc)?;
                    let style = r.varint32().map_err(trunc)?;
                    Instr::SetStyle(key, style)
                }
                0x40 => Instr::Return,
                other => return Err(VmError::UnknownOp(other)),
            };
            code.push(instr);
        }
        if code.is_empty() {
            return Err(VmError::Malformed("no code"));
        }
        // Resolve jumps to instruction indices: a target must be a boundary.
        let end = bytes.len().saturating_sub(code_start);
        for (at, target) in raw_jumps {
            let idx = if target == end as i64 {
                code.len() // jumping to the end is a return
            } else {
                offsets.iter().position(|o| *o as i64 == target).ok_or(VmError::BadJump)?
            };
            match code.get_mut(at) {
                Some(Instr::Jump(t)) | Some(Instr::JumpIfFalse(t)) => *t = idx,
                _ => return Err(VmError::BadJump),
            }
        }
        // The last instruction must not fall off the end.
        match code.last() {
            Some(Instr::Return | Instr::Jump(_)) => {}
            _ => return Err(VmError::Malformed("code does not end in return or jump")),
        }

        // Pass 2: stack depth along every path.
        let mut depth_at: Vec<Option<u8>> = vec![None; code.len().saturating_add(1)];
        let mut work = vec![(0usize, 0u8)];
        while let Some((pc, depth)) = work.pop() {
            if pc == code.len() {
                continue; // fell off the end via a jump-to-end: fine
            }
            match depth_at.get(pc).copied().flatten() {
                Some(seen) if seen == depth => continue,
                Some(_) => return Err(VmError::Stack("inconsistent depth at a merge")),
                None => {}
            }
            if let Some(slot) = depth_at.get_mut(pc) {
                *slot = Some(depth);
            }
            let Some(instr) = code.get(pc).copied() else { break };
            let (pops, pushes) = instr.effect();
            if depth < pops {
                return Err(VmError::Stack("underflow"));
            }
            let after = depth.saturating_sub(pops).saturating_add(pushes);
            if after > max_stack {
                return Err(VmError::Stack("depth above the declared maximum"));
            }
            match instr {
                Instr::Return => {}
                Instr::Jump(t) => work.push((t, after)),
                Instr::JumpIfFalse(t) => {
                    work.push((t, after));
                    work.push((pc.saturating_add(1), after));
                }
                _ => work.push((pc.saturating_add(1), after)),
            }
        }
        Ok(Self { code, max_stack })
    }

    /// Instructions, for diagnostics.
    pub fn instructions(&self) -> &[Instr] {
        &self.code
    }
}

/// What a chunk may reach. The client implements this over its session; a
/// test implements it over a hash map.
pub trait Host {
    /// The value of an atom, or `None` for an undefined id.
    fn atom(&self, id: u32) -> Option<&str>;
    /// A root prop, `Null` if absent.
    fn load(&self, atom: u32) -> Value;
    /// Set a root prop.
    fn store(&mut self, atom: u32, value: Value) -> bool;
    /// Set a node's text.
    fn set_text(&mut self, node: u32, text: String) -> bool;
    /// Set a node's prop.
    fn set_prop(&mut self, node: u32, atom: u32, value: Value) -> bool;
    /// Point a node at a style id.
    fn set_style(&mut self, node: u32, style: u32) -> bool;
    /// Queue a server event named by `atom`.
    fn emit(&mut self, atom: u32);
}

/// Run a verified chunk against a host with the default fuel.
pub fn run(chunk: &Chunk, host: &mut dyn Host) -> Result<(), VmError> {
    run_with_fuel(chunk, host, FUEL)
}

/// Run with an explicit fuel budget.
pub fn run_with_fuel(chunk: &Chunk, host: &mut dyn Host, mut fuel: u32) -> Result<(), VmError> {
    let mut stack: Vec<Value> = Vec::with_capacity(usize::from(chunk.max_stack));
    let mut pc = 0usize;
    // Verified: pops never underflow, so `pop()` failing is an internal error
    // reported as a stack fault rather than a panic.
    fn pop(stack: &mut Vec<Value>) -> Result<Value, VmError> {
        stack.pop().ok_or(VmError::Stack("underflow at run time"))
    }
    fn int(v: Value, what: &'static str) -> Result<i64, VmError> {
        match v {
            Value::Int(n) => Ok(n),
            _ => Err(VmError::Type(what)),
        }
    }
    fn boolean(v: Value, what: &'static str) -> Result<bool, VmError> {
        match v {
            Value::Bool(b) => Ok(b),
            _ => Err(VmError::Type(what)),
        }
    }
    fn string(v: Value, what: &'static str) -> Result<String, VmError> {
        match v {
            Value::Str(s) => Ok(s),
            _ => Err(VmError::Type(what)),
        }
    }
    while let Some(instr) = chunk.code.get(pc).copied() {
        fuel = fuel.checked_sub(1).ok_or(VmError::Fuel)?;
        pc = pc.saturating_add(1);
        match instr {
            Instr::PushInt(n) => stack.push(Value::Int(n)),
            Instr::PushStr(a) => stack.push(Value::Str(host.atom(a).ok_or(VmError::Host("unknown atom"))?.to_owned())),
            Instr::PushBool(b) => stack.push(Value::Bool(b)),
            Instr::Load(a) => stack.push(host.load(a)),
            Instr::Store(a) => {
                let v = pop(&mut stack)?;
                if !host.store(a, v) {
                    return Err(VmError::Host("store refused"));
                }
            }
            Instr::Dup => {
                let v = stack.last().cloned().ok_or(VmError::Stack("underflow at run time"))?;
                stack.push(v);
            }
            Instr::Pop => {
                pop(&mut stack)?;
            }
            Instr::Add | Instr::Sub | Instr::Mul => {
                let b = int(pop(&mut stack)?, "arithmetic on a non-integer")?;
                let a = int(pop(&mut stack)?, "arithmetic on a non-integer")?;
                stack.push(Value::Int(match instr {
                    Instr::Add => a.wrapping_add(b),
                    Instr::Sub => a.wrapping_sub(b),
                    _ => a.wrapping_mul(b),
                }));
            }
            Instr::Neg => {
                let a = int(pop(&mut stack)?, "neg on a non-integer")?;
                stack.push(Value::Int(a.wrapping_neg()));
            }
            Instr::Not => {
                let a = boolean(pop(&mut stack)?, "not on a non-bool")?;
                stack.push(Value::Bool(!a));
            }
            Instr::Eq => {
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(Value::Bool(a == b));
            }
            Instr::Lt | Instr::Gt => {
                let b = int(pop(&mut stack)?, "comparison on a non-integer")?;
                let a = int(pop(&mut stack)?, "comparison on a non-integer")?;
                stack.push(Value::Bool(if matches!(instr, Instr::Lt) { a < b } else { a > b }));
            }
            Instr::And | Instr::Or => {
                let b = boolean(pop(&mut stack)?, "logic on a non-bool")?;
                let a = boolean(pop(&mut stack)?, "logic on a non-bool")?;
                stack.push(Value::Bool(if matches!(instr, Instr::And) { a && b } else { a || b }));
            }
            Instr::ToStr => {
                let v = pop(&mut stack)?;
                stack.push(Value::Str(match v {
                    Value::Null => String::new(),
                    Value::Bool(b) => b.to_string(),
                    Value::Int(n) => n.to_string(),
                    Value::Str(s) => s,
                }));
            }
            Instr::Concat => {
                let b = string(pop(&mut stack)?, "concat on a non-string")?;
                let mut a = string(pop(&mut stack)?, "concat on a non-string")?;
                if a.len().saturating_add(b.len()) > MAX_STR {
                    return Err(VmError::StringTooLong);
                }
                a.push_str(&b);
                stack.push(Value::Str(a));
            }
            Instr::Jump(t) => pc = t,
            Instr::JumpIfFalse(t) => {
                if !boolean(pop(&mut stack)?, "branch on a non-bool")? {
                    pc = t;
                }
            }
            Instr::SetText(node) => {
                let s = string(pop(&mut stack)?, "set_text with a non-string")?;
                if !host.set_text(node, s) {
                    return Err(VmError::Host("set_text refused"));
                }
            }
            Instr::SetProp(node, atom) => {
                let v = pop(&mut stack)?;
                if !host.set_prop(node, atom, v) {
                    return Err(VmError::Host("set_prop refused"));
                }
            }
            Instr::SetStyle(node, style) => {
                if !host.set_style(node, style) {
                    return Err(VmError::Host("set_style refused"));
                }
            }
            Instr::Emit(atom) => host.emit(atom),
            Instr::Return => return Ok(()),
        }
    }
    Ok(())
}

/// A tiny assembler, for tests and for servers that build chunks by hand.
#[derive(Debug, Default, Clone)]
pub struct Asm {
    code: Vec<u8>,
    max_stack: u8,
}

impl Asm {
    /// Start a chunk that needs at most `max_stack` slots.
    pub fn new(max_stack: u8) -> Self {
        Self { code: Vec::new(), max_stack }
    }
    fn varint(&mut self, v: u32) {
        let mut w = eui_proto::Writer::new();
        w.varint32(v);
        self.code.extend_from_slice(w.as_slice());
    }
    /// `push_int`.
    pub fn push_int(mut self, n: i64) -> Self {
        self.code.push(0x01);
        let mut w = eui_proto::Writer::new();
        w.svarint(n);
        self.code.extend_from_slice(w.as_slice());
        self
    }
    /// `push_str`.
    pub fn push_str(mut self, atom: u32) -> Self {
        self.code.push(0x02);
        self.varint(atom);
        self
    }
    /// `push_bool`.
    pub fn push_bool(mut self, b: bool) -> Self {
        self.code.extend_from_slice(&[0x03, u8::from(b)]);
        self
    }
    /// `load`.
    pub fn load(mut self, atom: u32) -> Self {
        self.code.push(0x04);
        self.varint(atom);
        self
    }
    /// `store`.
    pub fn store(mut self, atom: u32) -> Self {
        self.code.push(0x05);
        self.varint(atom);
        self
    }
    /// A zero-operand op by opcode.
    pub fn op(mut self, opcode: u8) -> Self {
        self.code.push(opcode);
        self
    }
    /// `jump` / `jump_if_false` with a relative offset in bytes from the next instruction.
    pub fn jump(mut self, opcode: u8, rel: i16) -> Self {
        self.code.push(opcode);
        self.code.extend_from_slice(&rel.to_le_bytes());
        self
    }
    /// `set_text`.
    pub fn set_text(mut self, node: u32) -> Self {
        self.code.push(0x30);
        self.varint(node);
        self
    }
    /// `set_prop`.
    pub fn set_prop(mut self, node: u32, atom: u32) -> Self {
        self.code.push(0x31);
        self.varint(node);
        self.varint(atom);
        self
    }
    /// `set_style`.
    pub fn set_style(mut self, node: u32, style: u32) -> Self {
        self.code.push(0x33);
        self.varint(node);
        self.varint(style);
        self
    }
    /// `emit`.
    pub fn emit(mut self, atom: u32) -> Self {
        self.code.push(0x32);
        self.varint(atom);
        self
    }
    /// `return`, and the finished bytes.
    pub fn ret(mut self) -> Vec<u8> {
        self.code.push(0x40);
        self.finish()
    }
    /// The finished bytes without appending a return.
    pub fn finish(self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(1);
        out.push(self.max_stack);
        out.extend_from_slice(&self.code);
        out
    }
}

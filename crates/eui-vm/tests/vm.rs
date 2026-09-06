//! The verifier refuses what it should; the interpreter does what the spec
//! says and nothing more.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::collections::HashMap;

use eui_vm::*;

#[derive(Default)]
struct Mem {
    atoms: HashMap<u32, String>,
    state: HashMap<u32, Value>,
    texts: HashMap<u32, String>,
    props: HashMap<(u32, u32), Value>,
    emitted: Vec<u32>,
    styles: HashMap<u32, u32>,
    refuse_nodes: bool,
}

impl Host for Mem {
    fn atom(&self, id: u32) -> Option<&str> {
        self.atoms.get(&id).map(String::as_str)
    }
    fn load(&self, atom: u32) -> Value {
        self.state.get(&atom).cloned().unwrap_or(Value::Null)
    }
    fn store(&mut self, atom: u32, value: Value) -> bool {
        self.state.insert(atom, value);
        true
    }
    fn set_text(&mut self, node: u32, text: String) -> bool {
        if self.refuse_nodes {
            return false;
        }
        self.texts.insert(node, text);
        true
    }
    fn set_prop(&mut self, node: u32, atom: u32, value: Value) -> bool {
        self.props.insert((node, atom), value);
        true
    }
    fn set_style(&mut self, node: u32, style: u32) -> bool {
        self.styles.insert(node, style);
        true
    }
    fn emit(&mut self, atom: u32) {
        self.emitted.push(atom);
    }
}

#[test]
fn set_style_repoints_a_node() {
    let chunk = Chunk::verify(&Asm::new(1).set_style(9, 4).ret()).unwrap();
    let mut m = Mem::default();
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.styles[&9], 4);
}

const COUNT: u32 = 1;
const INC: u32 = 2;
const VALUE_NODE: u32 = 7;

/// `state.count += 1; value.text = state.count.to_s; emit increment`
fn counter_chunk() -> Vec<u8> {
    Asm::new(2).load(COUNT).push_int(1).op(0x10).op(0x06).store(COUNT).op(0x1A).set_text(VALUE_NODE).emit(INC).ret()
}

#[test]
fn the_counter_increments_locally_and_queues_the_server_event() {
    let chunk = Chunk::verify(&counter_chunk()).unwrap();
    let mut m = Mem::default();
    m.atoms.insert(INC, "increment".into());
    m.state.insert(COUNT, Value::Int(41));
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.state[&COUNT], Value::Int(42));
    assert_eq!(m.texts[&VALUE_NODE], "42");
    assert_eq!(m.emitted, vec![INC]);
    // Absent state reads as null; adding to null is a type error and leaves
    // nothing half-done.
    let mut m = Mem::default();
    assert_eq!(run(&chunk, &mut m), Err(VmError::Type("arithmetic on a non-integer")));
    assert!(m.texts.is_empty());
}

#[test]
fn branches_and_comparison() {
    // if count > 3 then text = "big" else text = "small"
    let big = 10;
    let small = 11;
    let chunk = Asm::new(2)
        .load(COUNT).push_int(3).op(0x17)
        .jump(0x21, 7) // over: push_str big (2) + set_text (2) + jump (3) = 7
        .push_str(big).set_text(VALUE_NODE)
        .jump(0x20, 4) // over: push_str small (2) + set_text (2)
        .push_str(small).set_text(VALUE_NODE)
        .ret();
    let chunk = Chunk::verify(&chunk).unwrap();
    let mut m = Mem::default();
    m.atoms.insert(big, "big".into());
    m.atoms.insert(small, "small".into());
    m.state.insert(COUNT, Value::Int(5));
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.texts[&VALUE_NODE], "big");
    m.state.insert(COUNT, Value::Int(1));
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.texts[&VALUE_NODE], "small");
}

#[test]
fn strings_bools_and_props() {
    let a = 20;
    let chunk = Asm::new(3)
        .push_str(a).load(COUNT).op(0x1A).op(0x1B).set_text(1) // "n=" + count
        .push_bool(true).op(0x14).set_prop(1, 5) // prop 5 = !true
        .push_int(2).push_int(2).op(0x15).store(COUNT) // count = (2 == 2)
        .ret();
    let chunk = Chunk::verify(&chunk).unwrap();
    let mut m = Mem::default();
    m.atoms.insert(a, "n=".into());
    m.state.insert(COUNT, Value::Int(9));
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.texts[&1], "n=9");
    assert_eq!(m.props[&(1, 5)], Value::Bool(false));
    assert_eq!(m.state[&COUNT], Value::Bool(true));
}

#[test]
fn verifier_rejects_what_the_spec_says() {
    let err = |bytes: Vec<u8>| Chunk::verify(&bytes).unwrap_err();
    assert_eq!(err(b"NOPE\x01\x02\x40".to_vec()), VmError::Malformed("bad magic"));
    assert_eq!(err(b"EUIC\x02\x02\x40".to_vec()), VmError::Malformed("unsupported version"));
    assert_eq!(err(b"EUIC\x01\x02".to_vec()), VmError::Malformed("no code"));
    assert_eq!(err(Asm::new(65).ret()), VmError::Stack("declared depth above the limit"));
    assert_eq!(err(Asm::new(1).op(0xEE).ret()), VmError::UnknownOp(0xEE));
    assert_eq!(err(Asm::new(1).op(0x10).ret()), VmError::Stack("underflow"));
    assert_eq!(err(Asm::new(1).push_int(1).push_int(2).ret()), VmError::Stack("depth above the declared maximum"));
    assert_eq!(err(Asm::new(1).push_int(1).finish()), VmError::Malformed("code does not end in return or jump"));
    // +1 from the next instruction lands inside the following push_int.
    assert_eq!(err(Asm::new(1).jump(0x20, 1).push_int(5).op(0x07).ret()), VmError::BadJump, "into the middle of an instruction");
    assert_eq!(err(Asm::new(1).jump(0x20, 99).ret()), VmError::BadJump, "past the end");
    assert_eq!(err(Asm::new(1).jump(0x20, -10).ret()), VmError::BadJump, "before the start");
    // Truncated operand.
    assert_eq!(err(b"EUIC\x01\x01\x01".to_vec()), VmError::Malformed("truncated operand"));
    // Two paths reach one instruction at different depths.
    let bad = Asm::new(2).push_bool(true).jump(0x21, 2).push_int(1).op(0x07).ret();
    assert_eq!(err(bad), VmError::Stack("inconsistent depth at a merge"));
    // A chunk over the size limit.
    let mut huge = Asm::new(1);
    for _ in 0..40_000 {
        huge = huge.push_int(1).op(0x07);
    }
    assert_eq!(err(huge.ret()), VmError::Malformed("chunk too large"));
    // Jumping exactly to the end is a return.
    assert!(Chunk::verify(&Asm::new(1).jump(0x20, 0).finish()).is_ok());
}

#[test]
fn fuel_stops_a_loop_and_the_host_can_refuse() {
    // loop: jump -3 forever
    let looping = Chunk::verify(&Asm::new(1).jump(0x20, -3).finish()).unwrap();
    let mut m = Mem::default();
    assert_eq!(run(&looping, &mut m), Err(VmError::Fuel));
    assert_eq!(run_with_fuel(&looping, &mut m, 10), Err(VmError::Fuel));

    let chunk = Chunk::verify(&counter_chunk()).unwrap();
    let mut m = Mem { refuse_nodes: true, ..Default::default() };
    m.atoms.insert(INC, "increment".into());
    m.state.insert(COUNT, Value::Int(0));
    assert_eq!(run(&chunk, &mut m), Err(VmError::Host("set_text refused")));
    assert!(m.emitted.is_empty(), "nothing is emitted after an abort");
}

#[test]
fn string_growth_is_bounded() {
    let a = 30;
    // s = a; repeat 20 times: s = s + s  → 2^20 × len, well past 4 KiB
    let mut asm = Asm::new(2).push_str(a);
    for _ in 0..20 {
        asm = asm.op(0x06).op(0x1B);
    }
    let chunk = Chunk::verify(&asm.set_text(1).ret()).unwrap();
    let mut m = Mem::default();
    m.atoms.insert(a, "abcdefgh".into());
    assert_eq!(run(&chunk, &mut m), Err(VmError::StringTooLong));
}

#[test]
fn arbitrary_bytes_never_panic_in_the_verifier() {
    let mut state = 0x9E37_79B9u64;
    for _ in 0..20_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let len = (state % 48) as usize;
        let mut buf = b"EUIC\x01\x08".to_vec();
        for i in 0..len {
            buf.push(((state >> (i % 56)) & 0xFF) as u8);
        }
        if let Ok(chunk) = Chunk::verify(&buf) {
            let mut m = Mem::default();
            let _ = run(&chunk, &mut m);
        }
    }
}

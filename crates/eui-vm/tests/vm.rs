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
    mode: Option<String>,
    went_back: bool,
    uniforms: HashMap<(u32, u32), f64>,
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
    fn set_mode(&mut self, mode: &str) -> bool {
        if !matches!(mode, "light" | "dark" | "high_contrast" | "toggle") {
            return false;
        }
        self.mode = Some(mode.to_owned());
        true
    }
    fn go_back(&mut self) {
        self.went_back = true;
    }
    fn set_scene_uniform(&mut self, node: u32, index: u32, value: f64) -> bool {
        // The two refusals a real host makes: the block is eight floats
        // wide, and a node that is not a scene has none.
        if self.refuse_nodes || index >= 8 {
            return false;
        }
        self.uniforms.insert((node, index), value);
        true
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
        .load(COUNT)
        .push_int(3)
        .op(0x17)
        .jump(0x21, 7) // over: push_str big (2) + set_text (2) + jump (3) = 7
        .push_str(big)
        .set_text(VALUE_NODE)
        .jump(0x20, 4) // over: push_str small (2) + set_text (2)
        .push_str(small)
        .set_text(VALUE_NODE)
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
        .push_str(a)
        .load(COUNT)
        .op(0x1A)
        .op(0x1B)
        .set_text(1) // "n=" + count
        .push_bool(true)
        .op(0x14)
        .set_prop(1, 5) // prop 5 = !true
        .push_int(2)
        .push_int(2)
        .op(0x15)
        .store(COUNT) // count = (2 == 2)
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

#[test]
fn set_mode_takes_a_string_and_the_host_decides() {
    let chunk = Chunk::verify(&Asm::new(1).push_str(1).set_mode().ret()).unwrap();
    let mut host = Mem::default();
    host.atoms.insert(1, "toggle".into());
    run(&chunk, &mut host).unwrap();
    assert_eq!(host.mode.as_deref(), Some("toggle"));
    // An unknown mode is refused by the host, which stops the run.
    host.atoms.insert(1, "sepia".into());
    assert!(run(&chunk, &mut host).is_err());
    // It pops: nothing on the stack is a verification error.
    assert!(Chunk::verify(&Asm::new(1).set_mode().ret()).is_err());
}

// ---------------------------------------------------------------- floats

/// 07 §3: a chunk can do arithmetic on floats, and every result it keeps is
/// a finite number.
///
/// The reason the VM grew them: a scene's uniform block is eight floats, and
/// a chunk that turns a cube with the pointer has to integrate an angle
/// between frames. Integers could not carry that, and a round trip to the
/// server for each frame is what the whole design is avoiding.
#[test]
fn a_chunk_can_do_float_arithmetic() {
    // (0.5 + 0.25) * 2.0 -> 1.5
    let chunk = Chunk::verify(&Asm::new(2).push_float(0.5).push_float(0.25).op(0x1C).push_float(2.0).op(0x1E).store(1).ret()).unwrap();
    let mut m = Mem::default();
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.state[&1], Value::Float(1.5));
}

#[test]
fn a_float_and_an_integer_are_not_the_same_number() {
    // Deliberate: a chunk that meant 1.0 and wrote 1 is told so at the
    // instruction, rather than having the two types quietly merge.
    let chunk = Chunk::verify(&Asm::new(2).push_float(1.0).push_int(1).op(0x1C).store(1).ret()).unwrap();
    assert_eq!(run(&chunk, &mut Mem::default()), Err(VmError::Type("float arithmetic on a non-float")));
    // `to_float` is the one instruction that bridges them.
    let chunk = Chunk::verify(&Asm::new(2).push_float(1.0).push_int(1).op(0x28).op(0x1C).store(1).ret()).unwrap();
    let mut m = Mem::default();
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.state[&1], Value::Float(2.0));
}

/// The invariant this buys: a `Value::Float` is finite, everywhere.
///
/// The wire format already refuses a non-finite float at decode; the VM is
/// the one place inside the client that could have made one. It aborts
/// instead — a chunk's effects are advisory and the server re-derives them,
/// so stopping costs a frame of optimism, while a NaN in a uniform costs a
/// picture that is nowhere for as long as the node lives.
#[test]
fn arithmetic_that_leaves_the_numbers_aborts_the_chunk() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("divide by zero", Asm::new(2).push_float(1.0).push_float(0.0).op(0x1F).store(1).ret()),
        ("the root of a negative", Asm::new(2).push_float(-1.0).op(0x27).store(1).ret()),
        ("an overflow", Asm::new(2).push_float(f64::MAX).push_float(f64::MAX).op(0x1C).store(1).ret()),
    ];
    for (what, bytes) in cases {
        let chunk = Chunk::verify(&bytes).unwrap();
        let mut m = Mem::default();
        assert_eq!(run(&chunk, &mut m), Err(VmError::NotFinite), "{what}");
        assert!(m.state.is_empty(), "{what}: and nothing was left half-done");
    }
    // A chunk cannot smuggle one in as a literal either.
    let chunk = Chunk::verify(&Asm::new(1).push_float(f64::NAN).store(1).ret()).unwrap();
    assert_eq!(run(&chunk, &mut Mem::default()), Err(VmError::NotFinite));
}

#[test]
fn a_float_reads_back_out_of_local_state_as_the_float_it_was() {
    // What makes an integrator possible: load, add, store, frame after frame.
    let chunk = Chunk::verify(&Asm::new(2).load(1).push_float(0.25).op(0x1C).op(0x06).store(1).set_uniform(9, 0).ret()).unwrap();
    let mut m = Mem::default();
    m.state.insert(1, Value::Float(1.0));
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.state[&1], Value::Float(1.25));
    assert_eq!(m.uniforms[&(9, 0)], 1.25);
}

/// A uniform is written through a door of its own, and the door refuses.
#[test]
fn a_uniform_index_outside_the_block_is_refused() {
    let chunk = Chunk::verify(&Asm::new(1).push_float(1.0).set_uniform(9, 8).ret()).unwrap();
    assert_eq!(run(&chunk, &mut Mem::default()), Err(VmError::Host("set_scene_uniform")));
    // And it takes a float, not an integer.
    let chunk = Chunk::verify(&Asm::new(1).push_int(1).set_uniform(9, 0).ret()).unwrap();
    assert_eq!(run(&chunk, &mut Mem::default()), Err(VmError::Type("a uniform takes a float")));
}

#[test]
fn the_trigonometry_a_turning_cube_needs() {
    // cos(0) -> 1, and sin/cos/sqrt all cost one unit of fuel like anything
    // else: a chunk gets 4 096 steps whatever it spends them on.
    let chunk = Chunk::verify(&Asm::new(1).push_float(0.0).op(0x26).store(1).ret()).unwrap();
    let mut m = Mem::default();
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.state[&1], Value::Float(1.0));
    let chunk = Chunk::verify(&Asm::new(1).push_float(9.0).op(0x27).store(1).ret()).unwrap();
    let mut m = Mem::default();
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.state[&1], Value::Float(3.0));
}

#[test]
fn a_float_becomes_a_string_the_way_a_person_would_write_it() {
    let chunk = Chunk::verify(&Asm::new(1).push_float(0.5).op(0x1A).set_text(3).ret()).unwrap();
    let mut m = Mem::default();
    run(&chunk, &mut m).unwrap();
    assert_eq!(m.texts[&3], "0.5");
}

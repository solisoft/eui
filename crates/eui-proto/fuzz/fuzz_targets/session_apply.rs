//! Decoded batches into a session with small quotas. Whatever the server
//! sends, the arena's live count must equal a fresh walk of the tree.
#![no_main]
use libfuzzer_sys::fuzz_target;
use eui_proto::*;
use eui_tree::{Limits, Session};

fuzz_target!(|data: &[u8]| {
    let mut session = Session::with_limits(Limits { max_nodes: 2_000, max_depth: 32, max_atom_total_bytes: 64 * 1024, ..Default::default() });
    let mut r = Reader::new(data);
    let mut seq = 0u64;
    while !r.is_empty() {
        let Ok(op) = Op::decode(&mut r) else { break };
        seq += 1;
        let _ = session.apply(&Batch { seq, ops: vec![op] });
        if let Some(root) = session.root() {
            assert_eq!(session.preorder(root).count() as u32, session.live_nodes());
        }
    }
});

//! Any bytes into every decoder entry point. Must never panic, hang, or
//! allocate without bound; a decoded frame must re-encode and re-decode to
//! itself.
#![no_main]
use libfuzzer_sys::fuzz_target;
use eui_proto::*;

fuzz_target!(|data: &[u8]| {
    if let Ok(frame) = Frame::decode(data) {
        let again = frame.encode();
        let back = Frame::decode(&again).expect("re-encoded frame decodes");
        assert_eq!(back, frame, "decode(encode(x)) == x");
    }
    let _ = Subtree::decode(&mut Reader::new(data));
    let _ = Op::decode(&mut Reader::new(data));
    let _ = Value::decode(&mut Reader::new(data));
    let _ = StyleRecord::decode(&mut Reader::new(data));
});

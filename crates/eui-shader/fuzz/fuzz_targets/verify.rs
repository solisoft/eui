//! The verifier must answer, never panic, on any text at all.
//!
//! It is the first thing in the client that a server-chosen shader meets,
//! and it runs in the worker: a panic here is a dead worker, which ends the
//! session (08 §10). That is survivable and it is still a bug — the answer
//! to a hostile module is a `Reject`, not a crash.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        // The verdict is thrown away. What is being tested is that there is
        // one -- and that a module which passes really did have its trip
        // counts proved, which the `steps` below re-asserts.
        if let Ok(shape) = eui_shader::verify(src) {
            assert!(shape.steps <= eui_shader::MAX_STEPS);
        }
    }
});

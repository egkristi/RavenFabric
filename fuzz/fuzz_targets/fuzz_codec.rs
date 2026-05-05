//! Fuzz target for the RPC msgpack codec.
//! Tests that arbitrary bytes never cause panics during decode.

#![no_main]
use libfuzzer_sys::fuzz_target;
use rf_rpc::codec;
use rf_rpc::types::{Request, Response};

fuzz_target!(|data: &[u8]| {
    // Try to decode as Request — must not panic
    let _ = codec::decode::<Request>(data);

    // Try to decode as Response — must not panic
    let _ = codec::decode::<Response>(data);
});

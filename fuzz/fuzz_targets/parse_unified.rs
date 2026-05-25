#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let lines = data.split_inclusive(|&b| b == b'\n').map(|l| l.to_vec());
    let _ = patchkit::unified::parse_patches(lines).count();
});

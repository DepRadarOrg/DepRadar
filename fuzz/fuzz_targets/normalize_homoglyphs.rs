#![no_main]

use deprader::typosquat::homoglyph::normalize_homoglyphs;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|name: &str| {
    let _ = normalize_homoglyphs(name);
});

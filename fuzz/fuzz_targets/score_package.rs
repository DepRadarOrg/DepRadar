#![no_main]

use deprader::typosquat::scanner::score_package;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|name: &str| {
    let _ = score_package(name).to_string();
});

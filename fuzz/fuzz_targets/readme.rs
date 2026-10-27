//! Any text as a module README, as the harvester reads every module's
//! documentation from a Kamailio source tree. A README that is read gives
//! only named parameters and functions.
#![no_main]

use kamailio_lsp::catalog;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|txt: &str| {
    if let Ok(doc) = catalog::parse_readme_txt("fuzzed", txt) {
        for item in doc.params.iter().chain(&doc.functions) {
            assert!(!item.name.is_empty(), "an item with no name");
        }
    }
});

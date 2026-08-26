//! Regenerate the built-in core catalogue from a pinned source tree.
//!
//!     cargo run --example gen_core_catalog -- <wiki> <version> [src-tree] > src/core_builtin.json
//!
//! The result is vendored so core-language completion works before the
//! user has configured anything.  A test asserts the vendored file
//! still equals a fresh harvest of the pinned tree, so it cannot drift.

fn main() {
    let mut args = std::env::args().skip(1);
    let tree = args
        .next()
        .expect("usage: gen_core_catalog <tree> <version>");
    let version = args
        .next()
        .expect("usage: gen_core_catalog <tree> <version>");
    let mut core = kamailio_lsp::catalog::harvest_core(std::path::Path::new(&tree));
    // the levels are a switch in the xlog module's source, which is a
    // different checkout from the wiki the rest of core comes from
    if let Some(src) = args.next() {
        core.log_levels = kamailio_lsp::catalog::harvest_log_levels(std::path::Path::new(&src));
    }
    let out = kamailio_lsp::catalog::BuiltinCore { version, core };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

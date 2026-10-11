//! Cost must grow with the size of the config, not with its square.
//!
//! Every scanner used to turn each match offset into a (line, column)
//! by recounting newlines from the start of the text, so a file twice
//! as long took four times as long to scan: `route_blocks` alone took
//! 85 s on the config below.  These tests time the scanners on
//! that config against a budget the linear version meets with a wide
//! margin on a loaded host.

use kamailio_lsp::analyze::{
    calls, code_words, defines, includes, loaded_modules, modparam_calls, pvars, route_blocks,
    route_refs,
};
use std::time::{Duration, Instant};

/// `n` chained request routes, each with a `#!define`, a call, a
/// pvar, a modparam and a route reference: every scanner has
/// thousands of matches to place.
fn big_cfg(n: usize) -> String {
    let mut s = String::from("loadmodule \"tm.so\"\nloadmodule \"sl.so\"\n");
    for i in 0..n {
        s.push_str(&format!(
            "#!define D{i} {i}\n\
             modparam(\"tm\", \"fr_timer\", {i})\n\
             route[r{i}] {{\n    xlog(\"L_INFO\", \"$ru $si {i}\");\n    route(r{});\n}}\n",
            i + 1
        ));
    }
    s
}

const N: usize = 6000;

/// The linear scanners finish this in well under a second in a debug
/// build; the quadratic ones needed tens of seconds.
const BUDGET: Duration = Duration::from_secs(5);

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    let took = t.elapsed();
    assert!(
        took < BUDGET,
        "{what} took {took:?} on a {N}-route config (budget {BUDGET:?})"
    );
    out
}

#[test]
fn scanners_are_linear_in_the_text() {
    let cfg = big_cfg(N);
    assert_eq!(timed("route_blocks", || route_blocks(&cfg)).len(), N);
    assert_eq!(timed("route_refs", || route_refs(&cfg)).len(), N);
    assert!(timed("calls", || calls(&cfg)).len() >= 2 * N);
    assert_eq!(timed("loaded_modules", || loaded_modules(&cfg)).len(), 2);
    assert_eq!(timed("modparam_calls", || modparam_calls(&cfg)).len(), N);
    assert_eq!(timed("pvars", || pvars(&cfg)).len(), 2 * N);
    assert!(timed("includes", || includes(&cfg)).is_empty());
    assert_eq!(timed("defines", || defines(&cfg)).len(), N);
    assert!(timed("code_words", || code_words(&cfg)).len() >= 4 * N);
}

#[test]
fn positions_at_the_end_of_a_large_config_are_exact() {
    // the fast path must place the LAST match exactly where the slow
    // one did: line 2 + 6 lines per route, `route(` indented 4
    let cfg = big_cfg(N);
    let base = 2 + 6 * (N as u32 - 1);
    let last = route_refs(&cfg).pop().unwrap();
    assert_eq!(last.name, format!("r{N}"));
    assert_eq!((last.line, last.col), (base + 4, 10));
    let blk = route_blocks(&cfg).pop().unwrap();
    assert_eq!((blk.line, blk.col), (base + 2, 0));
    assert_eq!((blk.end_line, blk.end_col), (base + 5, 1));
    let def = defines(&cfg).pop().unwrap();
    assert_eq!((def.line, def.col), (base, 9));
}

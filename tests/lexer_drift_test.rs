//! Gates that read the REAL Kamailio lexer rather than restating it.
//!
//! `src/core/cfg.lex` declares the route kinds as `ROUTE*` macros, and
//! that declaration is what the real parser accepts. The analyzer
//! carries its own list. The two agree today, but nothing held them
//! together: a kind upstream adds and the analyzer misses is a route
//! block the server cannot see at all, with every call into it
//! reported undefined — a warning on a configuration that is correct.
//!
//! `analyze_test.rs` cites this file in comments and hard-codes what
//! it says. A comment does not fail when upstream moves.

mod common;

use kamailio_lsp::analyze::route_defs;

/// The route-kind keywords `cfg.lex` declares, from lines of the form
/// `ROUTE_REQUEST request_route`.
///
/// Not every `ROUTE*` macro is a route kind: `ROUTE_LOCKS_SIZE` binds
/// the quoted core parameter `"route_locks_size"`. The quotes are what
/// separates the two, so an unquoted keyword is the test.
fn kinds_from_the_lexer(lex: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lex.lines() {
        let mut it = line.split_whitespace();
        let (Some(macro_name), Some(keyword)) = (it.next(), it.next()) else {
            continue;
        };
        // exactly two tokens: a macro definition, not a rule
        if it.next().is_some() {
            continue;
        }
        if !macro_name.starts_with("ROUTE") {
            continue;
        }
        // a quoted value is a core parameter, not a block keyword
        if !keyword
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        if !out.contains(&keyword.to_string()) {
            out.push(keyword.to_string());
        }
    }
    out
}

fn lexer() -> String {
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    std::fs::read_to_string(std::path::Path::new(&tree).join("src/core/cfg.lex"))
        .expect("src/core/cfg.lex in the pinned tree")
}

#[test]
fn every_route_kind_in_the_real_lexer_is_recognised() {
    let kinds = kinds_from_the_lexer(&lexer());

    // POSITIVE CONTROL. Every assertion below is "for each kind the
    // lexer declares"; if the extraction stopped matching, the loop
    // would run over nothing and pass having proved nothing. 6.1.4
    // declares eight.
    assert!(
        kinds.len() >= 8,
        "only {} route kinds extracted from cfg.lex: {kinds:?}",
        kinds.len()
    );
    for expected in ["request_route", "event_route", "onsend_route"] {
        assert!(
            kinds.iter().any(|k| k == expected),
            "the extraction must find {expected}: {kinds:?}"
        );
    }

    let mut unseen: Vec<String> = Vec::new();
    for kind in &kinds {
        // both shapes the analyzer accepts: a bare block, and a named
        // one. A kind it does not know matches neither.
        let named = format!("{kind}[DRIFT_GATE] {{\n    exit;\n}}\n");
        let bare = format!("{kind} {{\n    exit;\n}}\n");
        let found_named = route_defs(&named).iter().any(|d| d.name == "DRIFT_GATE");
        let found_bare = !route_defs(&bare).is_empty();
        if !found_named && !found_bare {
            unseen.push(kind.clone());
        }
    }
    assert!(
        unseen.is_empty(),
        "cfg.lex declares {} route kind(s) the analyzer does not recognise: {unseen:?}\n\
         (a block of that kind is invisible, and every call into it warns)",
        unseen.len()
    );
    eprintln!("{} route kinds, all recognised: {kinds:?}", kinds.len());
}

/// A `ROUTE*` macro binding a quoted value is a core parameter, not a
/// block keyword. Collecting `route_locks_size` would have the gate
/// demand the analyzer treat a core parameter as a route kind — a
/// gate that can only be satisfied by making the server wrong.
#[test]
fn a_quoted_route_macro_is_a_core_parameter_not_a_kind() {
    let lex = lexer();
    // the hazard is really in the tree, not invented for this test
    assert!(
        lex.contains("ROUTE_LOCKS_SIZE"),
        "cfg.lex no longer declares ROUTE_LOCKS_SIZE; this gate needs rechecking"
    );
    let kinds = kinds_from_the_lexer(&lex);
    assert!(
        !kinds.iter().any(|k| k == "route_locks_size"),
        "a quoted ROUTE* macro must not be read as a route kind: {kinds:?}"
    );
}

/// The reverse direction: the analyzer must not invent a kind the
/// language does not have. A name it treats as a route block but the
/// parser does not is a definition the server believes in and
/// Kamailio rejects.
#[test]
fn the_analyzer_recognises_no_route_kind_the_lexer_lacks() {
    let kinds = kinds_from_the_lexer(&lexer());
    assert!(kinds.len() >= 8, "positive control: {kinds:?}");

    // OpenSIPS's kinds are the realistic way a wrong one gets in: the
    // two languages are close enough that a rule copied across would
    // look right.
    for foreign in [
        "timer_route",
        "error_route",
        "local_route",
        "startup_route",
        "not_a_route",
    ] {
        if kinds.iter().any(|k| k == foreign) {
            continue; // Kamailio really does have it
        }
        let named = format!("{foreign}[DRIFT_GATE] {{\n    exit;\n}}\n");
        assert!(
            !route_defs(&named).iter().any(|d| d.name == "DRIFT_GATE"),
            "'{foreign}' is not in cfg.lex but the analyzer reads it as a route definition"
        );
    }
}

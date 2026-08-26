//! The vendored core catalogue.
//!
//! Core parameters, functions and pseudo-variables are the language,
//! not a module — requiring a source checkout before `log_level`
//! completes makes the extension useless out of the box. The vendored
//! copy fills that in, and must not drift from the version it claims.

mod common;
use kamailio_lsp::catalog;

#[test]
fn the_vendored_catalogue_has_the_core_language_in_it() {
    let b = catalog::builtin_core();
    assert!(!b.version.is_empty(), "it must say which version it is");
    let params: Vec<&str> = b.core.params.iter().map(|p| p.name.as_str()).collect();
    // Kamailio's own spellings — it kept `children` and `listen`
    // where OpenSIPS renamed them, and uses `debug` rather than
    // `log_level`
    for want in ["debug", "children", "listen", "log_facility", "log_name"] {
        assert!(
            params.contains(&want),
            "{want} missing from the built-in params"
        );
    }
    assert!(b.core.functions.len() > 20, "{}", b.core.functions.len());
    assert!(b.core.pvars.len() > 50, "{}", b.core.pvars.len());
}

/// Every entry says where it came from, so nobody mistakes the pinned
/// The catalogue carries no provenance note of its own.
///
/// It used to: the note was baked into every entry as the catalogue
/// loaded, so it could not be turned off and it named a release the
/// user might not be using. It is applied by the server now, only
/// when `versionInHints` asks for it, which is also what lets it name
/// the right release. A note baked in here would be shown regardless
/// of the setting and would defeat both.
#[test]
fn the_catalogue_carries_no_baked_in_provenance_note() {
    let b = catalog::builtin_core();
    let mut checked = 0usize;
    for it in b
        .core
        .params
        .iter()
        .chain(b.core.functions.iter())
        .chain(b.core.pvars.iter())
    {
        checked += 1;
        assert!(
            !it.doc.contains("Built-in"),
            "{} carries a baked-in note: {:?}",
            it.name,
            it.doc
        );
        assert!(
            !it.doc.contains("kamailioSrc"),
            "{} carries a baked-in escape hatch: {:?}",
            it.name,
            it.doc
        );
    }
    // POSITIVE CONTROL: an empty catalogue would satisfy every
    // absence above.
    assert!(checked > 100, "only {checked} entries examined");
}

/// And the note the server applies says what it should.
#[test]
fn the_provenance_note_names_its_catalogue_and_release() {
    let note = catalog::version_note("core", "9.9.9");
    assert!(note.contains("Kamailio 9.9.9"), "{note:?}");
    assert!(note.contains("core documentation"), "{note:?}");
    assert!(note.contains("kamailioSrc"), "{note:?}");
}

/// The freshness gate: the vendored file must still equal a harvest of
/// the pinned wiki checkout.  Regenerate with
/// `cargo run --example gen_core_catalog -- <tree> <version>`.
#[test]
fn the_vendored_catalogue_matches_a_fresh_harvest_of_the_pinned_tree() {
    // The vendored catalogue is built from BOTH pinned inputs — the
    // wiki for the text and the source tree for what the grammar
    // accepts — so a fresh harvest for comparison must use both, in
    // the same order the generator does. Comparing against the wiki
    // alone reports every grammar-derived name as drift.
    let wiki = common::required_env("KAMAILIO_LSP_TEST_WIKI");
    let src = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let mut fresh = catalog::harvest_core(std::path::Path::new(&wiki));
    let (attrs, mods) =
        catalog::harvest_socket_syntax(std::path::Path::new(&src), std::path::Path::new(&wiki));
    fresh.socket_attrs = attrs;
    fresh.listen_modifiers = mods;
    catalog::reconcile_with_tree(&mut fresh, std::path::Path::new(&src));
    let fresh = fresh;
    let b = catalog::builtin_core();
    let names = |v: &[catalog::Item]| -> Vec<String> { v.iter().map(|i| i.name.clone()).collect() };
    assert_eq!(
        names(&b.core.params),
        names(&fresh.params),
        "vendored core params differ from the pinned tree — regenerate"
    );
    assert_eq!(
        names(&b.core.functions),
        names(&fresh.functions),
        "vendored core functions differ from the pinned tree — regenerate"
    );
    assert_eq!(
        names(&b.core.pvars),
        names(&fresh.pvars),
        "vendored core pvars differ from the pinned tree — regenerate"
    );
}

/// Owed 1/2: the vendored catalogue must match in its TEXT, not just
/// its names.
///
/// The drift gate above compares names. The regression that dropped
/// every worked example from every hover — 285 documented, none
/// arriving — changed no name at all, so that gate would have passed
/// it without a murmur. Names are the cheap half of a catalogue; the
/// text is what a reader is actually shown.
#[test]
fn the_vendored_catalogue_matches_a_fresh_harvest_in_its_text_too() {
    let wiki = common::required_env("KAMAILIO_LSP_TEST_WIKI");
    let src = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let mut fresh = catalog::harvest_core(std::path::Path::new(&wiki));
    let (attrs, mods) =
        catalog::harvest_socket_syntax(std::path::Path::new(&src), std::path::Path::new(&wiki));
    fresh.socket_attrs = attrs;
    fresh.listen_modifiers = mods;
    catalog::reconcile_with_tree(&mut fresh, std::path::Path::new(&src));
    let b = catalog::builtin_core();

    let mut differ: Vec<String> = Vec::new();
    let mut compared = 0usize;
    for (what, shipped, harvested) in [
        ("params", &b.core.params, &fresh.params),
        ("functions", &b.core.functions, &fresh.functions),
        ("pvars", &b.core.pvars, &fresh.pvars),
    ] {
        for (s, h) in shipped.iter().zip(harvested.iter()) {
            compared += 1;
            if s.doc != h.doc || s.detail != h.detail {
                differ.push(format!("{what}/{}", s.name));
            }
        }
    }
    // POSITIVE CONTROL: both sides were read.
    assert!(compared > 400, "only {compared} entries compared");
    assert!(
        differ.is_empty(),
        "vendored text differs from the pinned inputs — regenerate. This is the \
         half the name comparison cannot see: {differ:?}"
    );
}

/// Owed 2/2: the gate must cover every field it ships.
///
/// It compared three of the eight. Routes, statements, the two socket
/// syntaxes and the log levels were vendored and never held against
/// anything, so any of them could go stale — or empty — and no test
/// in the suite would notice.
#[test]
fn the_drift_gate_covers_every_field_of_the_catalogue() {
    let wiki = common::required_env("KAMAILIO_LSP_TEST_WIKI");
    let src = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let mut fresh = catalog::harvest_core(std::path::Path::new(&wiki));
    let (attrs, mods) =
        catalog::harvest_socket_syntax(std::path::Path::new(&src), std::path::Path::new(&wiki));
    fresh.socket_attrs = attrs;
    fresh.listen_modifiers = mods;
    fresh.log_levels = catalog::harvest_log_levels(std::path::Path::new(&src));
    catalog::reconcile_with_tree(&mut fresh, std::path::Path::new(&src));
    let b = catalog::builtin_core();

    let names = |v: &[catalog::Item]| -> Vec<String> { v.iter().map(|i| i.name.clone()).collect() };
    for (what, shipped, harvested) in [
        ("routes", &b.core.routes, &fresh.routes),
        ("statements", &b.core.statements, &fresh.statements),
        ("socket_attrs", &b.core.socket_attrs, &fresh.socket_attrs),
        (
            "listen_modifiers",
            &b.core.listen_modifiers,
            &fresh.listen_modifiers,
        ),
    ] {
        // POSITIVE CONTROL: a field that harvests empty would make
        // the comparison below trivially true.
        assert!(!harvested.is_empty(), "{what} harvested empty");
        assert_eq!(
            names(shipped),
            names(harvested),
            "vendored {what} differ from the pinned inputs — regenerate"
        );
    }
    assert!(!fresh.log_levels.is_empty(), "log_levels harvested empty");
    assert_eq!(
        b.core.log_levels, fresh.log_levels,
        "vendored log levels differ from the pinned tree — regenerate"
    );
}

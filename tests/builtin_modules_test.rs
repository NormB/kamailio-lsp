//! The vendored module catalogue.
//!
//! `is_method` is a `sipmsgops` function, not core, so shipping only
//! the core language still left every module call undocumented and
//! `loadmodule "` offering nothing at all.  This is the same bargain
//! the core catalogue strikes, one level up: useful before the user
//! configures anything, pinned to one version, and replaced wholesale
//! by a tree they configure.

mod common;
use kamailio_lsp::catalog;

#[test]
fn the_vendored_catalogue_has_the_modules_a_config_actually_loads() {
    let b = catalog::builtin_modules();
    assert!(!b.version.is_empty(), "it must say which version it is");
    let names: Vec<&str> = b.modules.iter().map(|m| m.name.as_str()).collect();
    for want in ["tm", "sl", "textops", "rr", "uac"] {
        assert!(names.contains(&want), "{want} missing from the built-ins");
    }
    assert!(b.modules.len() > 100, "{}", b.modules.len());

    // the function that prompted this: it must be attributed to the
    // module that exports it, or the loaded-module gate cannot work
    let owner = b
        .modules
        .iter()
        .find(|m| m.functions.iter().any(|f| f.name == "is_method"))
        .map(|m| m.name.as_str());
    assert_eq!(owner, Some("textops"), "is_method belongs to textops");

    let params: usize = b.modules.iter().map(|m| m.params.len()).sum();
    let functions: usize = b.modules.iter().map(|m| m.functions.len()).sum();
    assert!(
        params > 500 && functions > 200,
        "{params} params, {functions} functions"
    );
}

/// Every entry says where it came from.  A module's exports move
/// between releases, so a user reading built-in documentation has to
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
    let b = catalog::builtin_modules();
    let mut checked = 0usize;
    for it in b
        .modules
        .iter()
        .flat_map(|m| m.params.iter().chain(m.functions.iter()))
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
    let note = catalog::version_note("module", "9.9.9");
    assert!(note.contains("Kamailio 9.9.9"), "{note:?}");
    assert!(note.contains("module documentation"), "{note:?}");
    assert!(note.contains("kamailioSrc"), "{note:?}");
}

/// The freshness gate: the vendored file must still equal a harvest of
/// every pinned tree it claims to cover.
///
/// `versioned_catalog_test.rs` proves that base-plus-deltas is a
/// lossless encoding, building one from the trees at test time. That
/// says nothing about the file actually SHIPPED — which is what a
/// user gets, and what goes stale. This reads the vendored file and
/// holds it against every release it names.
///
/// Regenerate with
/// `cargo run --example gen_versioned_catalog -- $(echo "$KAMAILIO_LSP_TEST_TREES" | tr , " ") > src/modules_builtin.json`.
#[test]
fn the_vendored_module_catalogue_matches_a_fresh_harvest_of_every_pinned_tree() {
    let raw = common::required_env("KAMAILIO_LSP_TEST_TREES");
    let trees: Vec<(String, std::path::PathBuf)> = raw
        .split(',')
        .filter_map(|e| {
            let (v, p) = e.split_once('=')?;
            Some((v.to_string(), std::path::PathBuf::from(p)))
        })
        .collect();
    let vendored = catalog::builtin_versioned();

    // the file must claim exactly the releases that were provisioned,
    // or it is covering something nothing here checks
    let provisioned: Vec<&str> = trees.iter().map(|(v, _)| v.as_str()).collect();
    assert_eq!(
        vendored.versions(),
        provisioned,
        "the vendored catalogue covers different releases than the proof environment provides"
    );

    for (version, tree) in &trees {
        let mut fresh = catalog::harvest_tree(tree);
        // the vendored file is stored canonically; a harvest must be
        // put in the same order before equality means anything
        catalog::canonicalize(&mut fresh);
        let got = vendored
            .at(version)
            .unwrap_or_else(|| panic!("{version} must resolve from the vendored file"));

        let names = |v: &[catalog::ModuleDoc]| -> Vec<String> {
            v.iter().map(|m| m.name.clone()).collect()
        };
        assert_eq!(
            names(&got),
            names(&fresh),
            "{version}: vendored modules differ from the pinned tree — regenerate"
        );
        for (a, b) in got.iter().zip(fresh.iter()) {
            assert_eq!(
                a, b,
                "{version}: module '{}' differs from the pinned tree — regenerate",
                a.name
            );
        }
    }
}

/// Parameters the harvester used to throw away.
///
/// Each name here belongs to one of the four README shapes the
/// harvester could not read: `kazoo` groups its parameters into
/// sub-sections, `carrierroute` and `matrix` put theirs in a chapter
/// of their own, `ims_qos` writes the type with no parentheses, and
/// `rtpengine` restarts its numbering partway through the chapter.
/// Sixty-one parameters were missing between them, and a
/// configuration setting any of them was told, in a warning, that the
/// parameter does not exist.
#[test]
fn the_readme_shapes_the_harvester_used_to_miss_are_in_the_catalogue() {
    let b = catalog::builtin_modules();
    for (module, param) in [
        // grouped into sub-sections
        ("kazoo", "node_hostname"),
        ("kazoo", "amqp_max_channels"),
        ("kazoo", "db_url"),
        ("kazoo", "presentity_table"),
        // a parameter chapter of its own
        ("carrierroute", "db_url"),
        ("carrierroute", "carrierroute_table"),
        ("matrix", "matrix_table"),
        // the type written without parentheses
        ("ims_qos", "terminate_dialog_on_rx_failure"),
        ("ims_qos", "recv_mode"),
        ("ims_qos_npn", "dialog_direction"),
        // after the chapter's numbering restarts
        ("rtpengine", "ping_interval"),
        ("rtpengine", "enable_dmq"),
        ("rtpengine", "event_callback"),
    ] {
        let m = b
            .modules
            .iter()
            .find(|m| m.name == module)
            .unwrap_or_else(|| panic!("{module} missing from the catalogue"));
        assert!(
            m.params.iter().any(|p| p.name == param),
            "{module} has no {param}: {:?}",
            m.params.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }
}

/// A catalogue name is what a config writes in `modparam`.
///
/// `3.14. terminate_dialog_on_rx_failure integer` was harvested with
/// the type inside the name, so the entry could never match the call
/// site it was supposed to document.
///
/// Two upstream-prose exceptions used to stand here — `mohqueue`
/// documenting two parameters under one heading, and `slack` writing
/// `slack url` with a space. Both are gone. A name is now taken from
/// the module's `param_export_t` table, and no C string literal
/// carrying a space or a parenthesis is a parameter, so there is
/// nothing left to excuse and the rule holds without exception.
#[test]
fn no_catalogue_entry_carries_its_type_in_its_name() {
    let b = catalog::builtin_modules();
    let mut checked = 0usize;
    for m in &b.modules {
        for p in &m.params {
            checked += 1;
            assert!(
                !p.name.contains('(') && !p.name.contains(' '),
                "{}::{} is not a name a modparam could write",
                m.name,
                p.name
            );
        }
    }
    assert!(
        checked > 2000,
        "suspiciously few catalogue entries checked: {checked}"
    );
}

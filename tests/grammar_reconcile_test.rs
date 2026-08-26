//! Names the grammar accepts and the cookbook never mentions.
//!
//! The sweep that found these is the same sweep the sibling server
//! got; the ANSWER is not. There it was five alias groups and two
//! undocumented settings. Here it is thirty-five alias groups and
//! thirty-five undocumented globals, and one of them runs the other
//! way round: OpenSIPS documents `wdir` and accepts `workdir`, while
//! this cookbook documents `workdir` and accepts `wdir`. A list
//! copied across would have been wrong in both directions.
//!
//! Three rules, all read from this tree:
//!
//! * Alternatives in one alternation are ALIASES — `rewriteuri|seturi`
//!   is one call spelled two ways — and this lexer writes them
//!   UNQUOTED, where the other quotes them.
//! * Globals come from `assign_stm` alone. The `socket = { ... }`
//!   attributes are assignable too (`bind = …;`), and reading every
//!   `TOKEN EQUAL` in the file would turn all seven into core
//!   parameters no configuration can set at the top level.
//! * A call is a script FUNCTION only where its production builds an
//!   action.

mod common;

use kamailio_lsp::catalog::lexer_spellings;

#[test]
fn unquoted_alternatives_are_aliases() {
    assert_eq!(lexer_spellings("workdir|wdir"), vec!["workdir", "wdir"]);
    assert_eq!(
        lexer_spellings("rewriteuri|seturi"),
        vec!["rewriteuri", "seturi"]
    );
}

#[test]
fn shouted_forms_are_not_spellings_of_their_own() {
    assert_eq!(
        lexer_spellings("advertise|ADVERTISE"),
        vec!["advertise"],
        "the upper-case half is the same word"
    );
}

#[test]
fn separate_groups_are_parts_of_one_spelling() {
    assert_eq!(
        lexer_spellings("(\"allow\"|\"ALLOW\")[-_](\"proxy\"|\"PROXY\")"),
        vec!["allow_proxy"]
    );
}

#[test]
fn a_pattern_with_no_word_yields_nothing() {
    assert!(lexer_spellings("{EAT_ABLE}").is_empty());
    assert!(lexer_spellings("").is_empty());
}

/// The same rule against a HARVEST, not the vendored catalogue.
///
/// A test that reads `builtin_core()` guards the shipped artefact and
/// cannot see the rule change until someone regenerates.
#[test]
fn a_harvest_files_an_alias_in_the_same_list_as_what_it_aliases() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let mut core = kamailio_lsp::catalog::harvest_core(&wiki);
    kamailio_lsp::catalog::reconcile_with_tree(&mut core, &tree);

    // POSITIVE CONTROL: the harvest read both lists.
    assert!(core.functions.len() > 40 && core.params.len() > 100);
    let is_fn = |n: &str| core.functions.iter().any(|f| f.name == n);
    let is_param = |n: &str| core.params.iter().any(|p| p.name == n);
    assert!(is_fn("rewriteuri"), "`rewriteuri` is a call");
    assert!(is_fn("seturi"), "so `seturi` must be one");
    assert!(!is_param("seturi"), "and must not also be a parameter");
    assert!(
        is_param("wdir") && !is_fn("wdir"),
        "a parameter alias stays one"
    );
}

/// An alias of a CALL is a call.
///
/// `rewriteuri` is a script action and `seturi` is the same action.
/// Filing the alias under parameters offers it where a parameter
/// belongs and hides it where the call does.
#[test]
fn an_alias_lands_in_the_same_list_as_what_it_aliases() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let fname: Vec<&str> = core.functions.iter().map(|f| f.name.as_str()).collect();
    let pname: Vec<&str> = core.params.iter().map(|p| p.name.as_str()).collect();
    for (documented, alias) in [
        ("rewriteuri", "seturi"),
        ("rewritehost", "sethost"),
        ("rewriteuser", "setuser"),
    ] {
        assert!(fname.contains(&documented), "{documented} is a call");
        assert!(fname.contains(&alias), "`{alias}` must be a call too");
        assert!(!pname.contains(&alias), "`{alias}` is not a parameter");
    }
    // and a parameter alias stays a parameter
    assert!(pname.contains(&"wdir"), "wdir is a parameter alias");
    assert!(!fname.contains(&"wdir"), "and not a call");
}

/// This cookbook documents `workdir`; OpenSIPS documents `wdir`.
#[test]
fn the_spelling_this_cookbook_skips_still_hovers() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let documented = core
        .params
        .iter()
        .find(|p| p.name == "workdir")
        .expect("workdir");
    let alias = core
        .params
        .iter()
        .find(|p| p.name == "wdir")
        .expect("`wdir` is accepted here and must be offered");
    assert_eq!(alias.doc, documented.doc, "one setting, one answer");
    assert!(
        alias.detail.contains("workdir"),
        "and it says which spelling the cookbook uses: {:?}",
        alias.detail
    );
}

#[test]
fn a_global_the_cookbook_never_mentions_is_still_offered() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    // `tcp_source_ipv4` was here until the cookbook's comma-separated
    // headings were parsed properly — it IS documented, under
    // `### tcp_source_ipv4, tcp_source_ipv6`, and calling it
    // undocumented was this reconciliation believing a parser bug.
    for name in ["dns_slow_query_ms", "tcp_close_rst"] {
        let it = core
            .params
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("`{name} = …` is accepted and offers nothing"));
        assert!(
            it.doc.to_lowercase().contains("does not describe"),
            "it must say the cookbook is silent rather than invent one: {:?}",
            it.doc
        );
    }
}

/// The `socket = { }` attributes are assignable, and are not globals.
#[test]
fn socket_block_attributes_do_not_become_core_parameters() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let pname: Vec<&str> = core.params.iter().map(|p| p.name.as_str()).collect();
    for attr in ["bind", "agname", "vrf", "advertise"] {
        assert!(
            !pname.contains(&attr),
            "`{attr}` is assignable only INSIDE a socket block; as a core \
             parameter it would be offered at the top level where it is a \
             syntax error: {} parameters",
            pname.len()
        );
        assert!(
            core.socket_attrs.iter().any(|a| a.name == attr),
            "{attr} belongs in the socket attributes"
        );
    }
}

#[test]
fn a_harvest_reconciles_the_real_tree() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let mut core = kamailio_lsp::catalog::harvest_core(&wiki);
    let before = core.params.len();
    // POSITIVE CONTROL: the cookbook harvest read something first.
    assert!(before > 100, "only {before} parameters before reconciling");

    kamailio_lsp::catalog::reconcile_with_tree(&mut core, &tree);
    assert!(
        core.params.len() > before,
        "the reconciliation added nothing: {before}"
    );
    let names: Vec<&str> = core.params.iter().map(|p| p.name.as_str()).collect();
    for want in ["wdir", "dns_slow_query_ms"] {
        assert!(names.contains(&want), "{want} missing from a real harvest");
    }
    // a digit in the TOKEN NAME, not just the spelling: `TCP_SOURCE_IPV4`
    // was dropped one level above the spelling filter, before its
    // spelling was ever looked at
    for want in ["tcp_source_ipv4", "tcp_source_ipv6"] {
        assert!(
            names.contains(&want),
            "{want} missing — its token name carries a digit"
        );
        let it = core.params.iter().find(|p| p.name == want).unwrap();
        assert!(
            !it.doc.to_lowercase().contains("does not describe"),
            "and the cookbook DOES describe it, under a heading naming several: \
             {:?}",
            it.doc
        );
    }
    assert!(
        !names.contains(&"bind"),
        "a socket attribute became a core parameter"
    );
}

#[test]
fn the_reconciliation_leaves_no_duplicate_names() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let mut core = kamailio_lsp::catalog::harvest_core(&wiki);
    kamailio_lsp::catalog::reconcile_with_tree(&mut core, &tree);
    for (what, items) in [("parameters", &core.params), ("functions", &core.functions)] {
        let mut seen: Vec<&str> = Vec::new();
        let mut dupes: Vec<&str> = Vec::new();
        for i in items {
            if seen.contains(&i.name.as_str()) {
                dupes.push(&i.name);
            } else {
                seen.push(&i.name);
            }
        }
        assert!(dupes.is_empty(), "duplicate {what}: {dupes:?}");
        assert!(seen.len() > 40, "only {} {what}", seen.len());
    }
}

/// A documented entry is never replaced by a generated note.
#[test]
fn the_cookbook_wins_wherever_it_speaks() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let listen = core.params.iter().find(|p| p.name == "listen").unwrap();
    assert!(
        !listen.doc.to_lowercase().contains("does not describe"),
        "`listen` is documented at length; a note must not have replaced it"
    );
    // POSITIVE CONTROL: notes exist, so this is not passing on nothing
    assert!(
        core.params
            .iter()
            .any(|p| p.doc.to_lowercase().contains("does not describe")),
        "no note was generated anywhere — the reconciliation did not run"
    );
}

/// Owed, first. A spelling may contain DIGITS.
///
/// The word filter accepted lower-case letters and underscores only,
/// so `tcp_source_ipv4` and `tcp_source_ipv6` were dropped without a
/// sound — settings the grammar accepts, silently absent from the
/// very reconciliation whose whole job is to find them. A filter that
/// discards its input is the shape of gap this work exists to close.
#[test]
fn a_spelling_may_contain_digits() {
    assert_eq!(
        lexer_spellings("\"tcp_source_ipv4\""),
        vec!["tcp_source_ipv4"],
        "a digit in the middle of a name is part of the name"
    );
    assert_eq!(
        lexer_spellings("\"disable_503_translation\""),
        vec!["disable_503_translation"]
    );
}

/// Owed, second. A bare number is not a spelling.
///
/// Widening the filter to allow digits must not widen it to allow a
/// token that is only digits: a port or a size in a lexer pattern is
/// a value, not a name anyone writes on the left of an `=`.
#[test]
fn a_bare_number_is_not_a_spelling() {
    assert!(
        lexer_spellings("\"5060\"").is_empty(),
        "a number is a value, not a setting name"
    );
    assert!(lexer_spellings("\"4xx\"").is_empty(), "nor is `4xx`");
    // POSITIVE CONTROL: the same shape starting with a letter is one
    assert_eq!(lexer_spellings("\"x4\""), vec!["x4"]);
}

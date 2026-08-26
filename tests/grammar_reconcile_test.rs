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

/// Owed #1: nothing the grammar uses is dropped without a trace.
///
/// Both filters that failed accepted letters and underscores only,
/// and a name they rejected simply vanished — no error, no count,
/// nothing anyone would notice. The tests written when that was
/// found pinned DIGITS; this pins the class, whatever characters
/// upstream reaches for next. The sibling server's tree answers this
/// with `mem-group`: a real parameter spelled with a HYPHEN.
#[test]
fn no_assignable_token_is_silently_dropped_by_the_spelling_reader() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let lex = std::fs::read_to_string(tree.join("src/core/cfg.lex")).expect("cfg.lex");
    let y = std::fs::read_to_string(tree.join("src/core/cfg.y")).expect("cfg.y");

    let assignable: std::collections::BTreeSet<&str> = y
        .match_indices(" EQUAL")
        .filter_map(|(i, _)| y[..i].rsplit(|c: char| c.is_whitespace()).next())
        .filter(|t| !t.is_empty() && t.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        .collect();
    assert!(
        assignable.len() > 100,
        "only {} assignable tokens",
        assignable.len()
    );

    let mut dropped: Vec<&str> = Vec::new();
    let mut seen = 0usize;
    for line in lex.split("\n%%").next().unwrap_or(&lex).lines() {
        let mut it = line.splitn(2, [' ', '\t']);
        let (Some(tok), Some(pat)) = (it.next(), it.next()) else {
            continue;
        };
        if !assignable.contains(tok) {
            continue;
        }
        // punctuation is not a name: `RBRACK` is `]`, and it reaches
        // this list because `... RBRACK EQUAL ...` appears in the
        // grammar. A token whose pattern holds no lower-case letter
        // has no spelling to find, and demanding one would make this
        // gate unfixable.
        if !pat.chars().any(|c| c.is_ascii_lowercase()) {
            continue;
        }
        seen += 1;
        if kamailio_lsp::catalog::lexer_spellings(pat).is_empty() {
            dropped.push(tok);
        }
    }
    assert!(
        seen > 80,
        "only {seen} assignable tokens matched a lexer line"
    );
    assert!(
        dropped.is_empty(),
        "the lexer defines these and the reader yields no spelling for any of \
         them, so they are absent from every hover and completion with nothing \
         anywhere saying so: {dropped:?}"
    );
}

/// Owed #2: a token NAME may carry a digit.
///
/// The spelling filter was fixed first and the catalogue did not
/// move, because the token-name filter one level above threw the line
/// away before its spelling was ever read.
#[test]
fn a_token_name_carrying_a_digit_is_read() {
    const LEX: &str = "TCP_SOURCE_IPV4\t\t\"tcp_source_ipv4\"\n%%\n";
    const Y: &str = "socket_lattr:\n\tTCP_SOURCE_IPV4 EQUAL x { y; }\n\t;\nsocket_lattrs: x;\n";
    assert_eq!(
        kamailio_lsp::catalog::parse_socket_attrs_c(Y, LEX),
        vec!["tcp_source_ipv4"],
        "a digit in the TOKEN name must not discard the line"
    );
}

/// Owed #3: every family heading yields every name in it.
///
/// Against the REAL cookbook, not a fixture: it is the cookbook that
/// decides how many of these there are, and a fixture would keep
/// passing after upstream adds the next one.
#[test]
fn the_real_cookbook_family_headings_all_resolve() {
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let md = kamailio_lsp::catalog::cookbook_core_md(&wiki).expect("core.md");
    let core = kamailio_lsp::catalog::harvest_core(&wiki);
    let names: Vec<&str> = core.params.iter().map(|p| p.name.as_str()).collect();

    let mut families = 0usize;
    let mut missing: Vec<String> = Vec::new();
    for line in md.lines() {
        let Some(h) = line.strip_prefix("### ") else {
            continue;
        };
        if !h.contains(',') {
            continue;
        }
        families += 1;
        for part in h.split(',') {
            let name = part.split_whitespace().next().unwrap_or("");
            if name.is_empty() || name.contains('(') || name.starts_with('$') {
                continue;
            }
            if !names.contains(&name) {
                missing.push(format!("{h} -> {name}"));
            }
        }
    }
    // POSITIVE CONTROL: this cookbook really does use such headings,
    // so an empty `missing` is a measurement and not an empty scan.
    assert!(families >= 2, "only {families} family heading(s) found");
    assert!(
        missing.is_empty(),
        "a heading names several parameters and only some arrived: {missing:?}"
    );
}

/// Owed #4: no catalogue name can contain a separator.
///
/// Taking the first word of `### tcp_source_ipv4, tcp_source_ipv6`
/// produced a parameter literally named `tcp_source_ipv4,`. A name
/// with a comma or a space in it can never be hovered, and nothing
/// else in the suite would have noticed.
#[test]
fn no_harvested_name_contains_a_separator() {
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let mut core = kamailio_lsp::catalog::harvest_core(&wiki);
    kamailio_lsp::catalog::reconcile_with_tree(&mut core, &tree);

    let mut bad: Vec<&str> = Vec::new();
    let mut n = 0usize;
    for list in [
        &core.params,
        &core.functions,
        &core.statements,
        &core.routes,
    ] {
        for i in list {
            n += 1;
            if i.name.contains(',') || i.name.contains(char::is_whitespace) {
                bad.push(&i.name);
            }
        }
    }
    assert!(n > 250, "only {n} entries harvested");
    assert!(
        bad.is_empty(),
        "a name carrying a separator is one no reader can ever hover: {bad:?}"
    );
}

// ---------------------------------------------------------------
// Owed for the four failures above. Two defects sat under them: one
// spelling reader written TWICE, whose copies drifted so the same
// lexer line read differently depending on which asked; and filters
// that discarded valid input without a trace.
//
// The dangerous direction is not a missed warning — nothing here
// validates global names, so a generated entry cannot silence a
// check. It is INVENTION: a hover telling a reader a setting exists
// when the grammar has never heard of it. A missing name is a gap; a
// fabricated one is a claim the reader has no way to test.
// ---------------------------------------------------------------

/// Owed 5/8: one reader, checked against the other on every line.
#[test]
fn the_two_spelling_readers_agree_on_every_line_of_the_real_lexer() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let lex = std::fs::read_to_string(tree.join("src/core/cfg.lex")).expect("cfg.lex");
    let mut checked = 0usize;
    for line in lex.split("\n%%").next().unwrap_or(&lex).lines() {
        let mut it = line.splitn(2, [' ', '\t']);
        let (Some(tok), Some(pat)) = (it.next(), it.next()) else {
            continue;
        };
        if tok.is_empty() || !tok.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let shared = kamailio_lsp::catalog::lexer_spellings(pat);
        let y = format!("socket_lattr:\n\t{tok} EQUAL x {{ y; }}\n\t;\nsocket_lattrs: x;\n");
        let via_socket = kamailio_lsp::catalog::parse_socket_attrs_c(&y, &lex);
        checked += 1;
        assert_eq!(
            via_socket,
            shared.iter().take(1).cloned().collect::<Vec<_>>(),
            "the two readers disagree about `{tok}` ({pat:?})"
        );
    }
    assert!(checked > 200, "only {checked} token lines compared");
}

/// Owed 6/8: a hyphenated name reaches every reader.
///
/// This tree has none today; the sibling server's `mem-group` shows
/// the shape is real, and a reader that stops at `_` would drop the
/// first one upstream adds here.
#[test]
fn a_hyphenated_spelling_reaches_every_reader() {
    assert_eq!(
        kamailio_lsp::catalog::lexer_spellings("mem-group"),
        vec!["mem-group"]
    );
    const LEX: &str = "MEMGROUP\tmem-group\n%%\n";
    const Y: &str = "socket_lattr:\n\tMEMGROUP EQUAL x { y; }\n\t;\nsocket_lattrs: x;\n";
    assert_eq!(
        kamailio_lsp::catalog::parse_socket_attrs_c(Y, LEX),
        vec!["mem-group"],
        "the socket reader must not stop at the underscore either"
    );
}

/// Owed 7/8: every spelling is something a reader could type.
#[test]
fn every_spelling_from_the_real_lexer_is_writable() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let lex = std::fs::read_to_string(tree.join("src/core/cfg.lex")).expect("cfg.lex");
    let mut bad: Vec<String> = Vec::new();
    let mut n = 0usize;
    for line in lex.split("\n%%").next().unwrap_or(&lex).lines() {
        let Some((_, pat)) = line.split_once([' ', '\t']) else {
            continue;
        };
        for w in kamailio_lsp::catalog::lexer_spellings(pat) {
            n += 1;
            let writable = w.starts_with(|c: char| c.is_ascii_lowercase())
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
            if !writable {
                bad.push(format!("{w:?} from {pat:?}"));
            }
        }
    }
    assert!(n > 200, "only {n} spellings produced");
    assert!(
        bad.is_empty(),
        "a regex fragment read as a setting is worse than a missing one, \
         because a hover would claim it exists: {bad:?}"
    );
}

/// Owed 8/8: the reconciliation never invents a name.
#[test]
fn every_name_the_reconciliation_adds_appears_in_the_lexer() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let lex = std::fs::read_to_string(tree.join("src/core/cfg.lex")).expect("cfg.lex");

    let before = kamailio_lsp::catalog::harvest_core(&wiki);
    let documented: Vec<String> = before
        .params
        .iter()
        .chain(before.functions.iter())
        .map(|i| i.name.clone())
        .collect();
    let mut core = kamailio_lsp::catalog::harvest_core(&wiki);
    kamailio_lsp::catalog::reconcile_with_tree(&mut core, &tree);

    let mut invented: Vec<&str> = Vec::new();
    let mut generated = 0usize;
    for i in core.params.iter().chain(core.functions.iter()) {
        if documented.contains(&i.name) {
            continue;
        }
        generated += 1;
        if !lex.contains(&i.name) {
            invented.push(&i.name);
        }
    }
    // POSITIVE CONTROL: it added a great many here.
    assert!(generated > 30, "only {generated} generated entries");
    assert!(
        invented.is_empty(),
        "the catalogue offers these and the lexer has never heard of them, so a \
         hover would claim a setting exists that no configuration can use: \
         {invented:?}"
    );
}

/// Owed for the `SSLv23` failure: a token NAME may be mixed case.
///
/// Every narrowing of this filter has cost the same way. First it
/// rejected digits, so `TCP_SOURCE_IPV4` vanished. Then it rejected
/// lower-case letters, so `SSLv23` — a real token of this grammar —
/// vanished too. Both in silence, which is what makes the class
/// worth a test rather than a fix.
#[test]
fn a_mixed_case_token_name_is_read() {
    const LEX: &str = "SSLv23\t\t\"sslv23\"|\"SSLv23\"\n%%\n";
    assert_eq!(
        kamailio_lsp::catalog::lexer_spellings("\"sslv23\"|\"SSLv23\""),
        vec!["sslv23"]
    );
    assert_eq!(
        kamailio_lsp::catalog::parse_socket_attrs_c(
            "socket_lattr:\n\tSSLv23 EQUAL x { y; }\n\t;\nsocket_lattrs: x;\n",
            LEX,
        ),
        vec!["sslv23"],
        "a lower-case letter in the TOKEN name must not discard the line"
    );
}

/// And the filter still refuses what is not a token at all.
#[test]
fn punctuation_and_lower_case_starts_are_not_token_names() {
    // a token name starts upper-case; these are grammar punctuation
    // or non-terminals, and accepting them would sweep the entire
    // grammar into the catalogue
    for not_a_token in ["socket_def", "listen_id", "1ABC", "_X"] {
        assert!(
            kamailio_lsp::catalog::parse_socket_attrs_c(
                &format!(
                    "socket_lattr:\n\t{not_a_token} EQUAL x {{ y; }}\n\t;\nsocket_lattrs: x;\n"
                ),
                &format!("{not_a_token}\tvalue\n%%\n"),
            )
            .is_empty(),
            "`{not_a_token}` is not a token name"
        );
    }
}

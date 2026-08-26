//! Typing `xlog(` offers the log levels.
//!
//! GIVEN `xlog`'s level argument is one of a fixed set the module's C
//! fixup recognises by the string's third character,
//! WHEN a reader types the open parenthesis,
//! THEN the editor offers that set, spelled the way the fixup accepts.
//!
//! The set is not documentation a harvester can read: it is a
//! `switch` in `src/modules/xlog/xlog.c`. It is read from there, the
//! way module parameters are read from `param_export_t` rather than
//! from prose — and it is NOT the same set OpenSIPS accepts. Kamailio
//! takes `L_BUG` as well, and its `case 'C'` assigns the internal
//! `L_CRIT2`, which a configuration spells `L_CRIT`.
//!
//! `xlog`'s parameter is fixed up as a string, so the quotes come with
//! the level unless the reader has already typed the opening one.

mod common;

use kamailio_lsp::catalog::parse_log_levels_c;
use kamailio_lsp::logic::completions_with_core;

#[test]
fn an_internal_constant_with_a_trailing_digit_keeps_the_script_spelling() {
    let src = r#"
			switch(((char *)(*param))[2]) {
				case 'C':
					xlp->v.level = L_CRIT2;
					break;
				default:
					LM_ERR("unknown log level\n");
			}
	"#;
    assert_eq!(
        parse_log_levels_c(src),
        vec!["L_CRIT"],
        "`L_CRIT2` is the internal constant for the level a configuration \
         spells `L_CRIT`; offering the constant completes into a level the \
         fixup reads as `L_CRIT` anyway, spelled a way no example uses"
    );
}

#[test]
fn a_case_letter_that_contradicts_its_level_is_skipped() {
    let wrong = r#"
			switch(((char *)(*param))[2]) {
				case 'A':
					xlp->v.level = L_ALERT;
					break;
				case 'Z':
					xlp->v.level = L_ERR;
					break;
				default:
					LM_ERR("unknown log level\n");
			}
	"#;
    assert_eq!(
        parse_log_levels_c(wrong),
        vec!["L_ALERT"],
        "the fixup dispatches on the third character, so a case letter that is \
         not that character means the shape has changed and the pairing cannot \
         be trusted"
    );
}

#[test]
fn a_switch_that_is_not_the_level_switch_is_not_read() {
    let other = r#"
		switch(s.s[0]) {
			case 'A': x = L_ALERT; break;
		}
	"#;
    assert!(parse_log_levels_c(other).is_empty(), "not the level switch");
}

#[test]
fn the_real_tree_yields_the_levels_kamailio_accepts() {
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let got = kamailio_lsp::catalog::harvest_log_levels(std::path::Path::new(&tree));
    assert_eq!(
        got,
        vec![
            "L_ALERT", "L_BUG", "L_CRIT", "L_ERR", "L_WARN", "L_NOTICE", "L_INFO", "L_DBG"
        ],
        "read from the tree's own switch, in the order it lists them"
    );
    assert!(
        got.contains(&"L_BUG".to_string()),
        "kamailio takes a level OpenSIPS does not — which is why each server \
         reads its own source rather than sharing a list"
    );
}

#[test]
fn the_built_in_catalogue_carries_the_levels() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    assert!(
        core.log_levels.contains(&"L_INFO".to_string()),
        "levels must ship, or the offer needs a source checkout: {:?}",
        core.log_levels
    );
}

fn labels(prefix: &str) -> Vec<String> {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    completions_with_core(&[], core, "request_route {\n}\n", prefix)
        .into_iter()
        .map(|c| c.label)
        .collect()
}

#[test]
fn an_open_parenthesis_offers_the_levels_quoted() {
    let got = labels("    xlog(");
    assert!(
        got.contains(&"\"L_INFO\"".to_string()),
        "the fixup wants a string there: {got:?}"
    );
    assert!(
        !got.contains(&"L_INFO".to_string()),
        "and the bare form must not be offered beside it: {got:?}"
    );
}

#[test]
fn inside_an_opened_quote_the_levels_are_offered_bare() {
    let got = labels("    xlog(\"");
    assert!(got.contains(&"L_INFO".to_string()), "{got:?}");
    assert!(!got.contains(&"\"L_INFO\"".to_string()), "{got:?}");
}

#[test]
fn every_call_that_takes_a_level_offers_them() {
    for f in ["xlog", "xlogl", "xlogm"] {
        let got = labels(&format!("    {f}(\""));
        assert!(
            got.contains(&"L_INFO".to_string()),
            "{f} takes a level at its first argument, per the module's \
             cmd_export_t table: {got:?}"
        );
    }
}

#[test]
fn a_call_that_takes_only_a_format_offers_none() {
    for f in ["xdbg", "xinfo", "xerr", "xnotice", "xwarn"] {
        let got = labels(&format!("    {f}(\""));
        assert!(
            !got.iter().any(|l| l.contains("L_INFO")),
            "{f} carries its level in its NAME and takes a format alone: {got:?}"
        );
    }
}

#[test]
fn the_format_argument_offers_no_levels() {
    let got = labels("    xlog(\"L_INFO\", \"");
    assert!(!got.iter().any(|l| l.contains("L_INFO")), "{got:?}");
}

#[test]
fn a_dollar_inside_the_level_argument_still_offers_pseudo_variables() {
    let got = labels("    xlog(\"$");
    assert!(
        got.iter().any(|l| l.starts_with('$')),
        "the fixup takes a pseudo-variable as the level (PV_MARKER): {got:?}"
    );
    assert!(
        !got.iter().any(|l| l.contains("L_INFO")),
        "and having typed `$` the reader is not asking for a level: {got:?}"
    );
}

#[test]
fn a_comma_inside_a_string_does_not_advance_the_argument() {
    let got = labels("    xlog(\"hello, world");
    assert!(
        got.contains(&"L_INFO".to_string()),
        "still the first argument: {got:?}"
    );
    let site = kamailio_lsp::logic::call_site("    xlog(\"hello, world").expect("in a call");
    assert_eq!(site.arg, 0, "the comma is text, not a separator");
    let site = kamailio_lsp::logic::call_site("    xlog(\"lvl\", \"fmt").expect("in a call");
    assert_eq!(site.arg, 1, "a real separator advances the index");
}

#[test]
fn outside_any_call_no_levels_are_offered() {
    assert!(
        !labels("    ").iter().any(|l| l.contains("L_INFO")),
        "a level is only ever an argument"
    );
}

/// The xlog module has exactly one enumerated string argument.
///
/// "The same pattern across all the appropriate calls" is only
/// finished if the set of appropriate calls is known. Here the
/// `cmd_export_t` table names the calls and the fixup names the
/// values, and the module parses a level string in exactly one place.
///
/// If upstream adds a second, this fails and the offer gets extended
/// — rather than the new one quietly offering nothing, which is the
/// state `xlog` itself was in.
#[test]
fn the_xlog_module_has_exactly_one_enumerated_string_argument() {
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let src = std::fs::read_to_string(std::path::Path::new(&tree).join("src/modules/xlog/xlog.c"))
        .expect("the xlog module in the pinned tree");
    let parsers = src.matches("unknown log level").count();
    assert_eq!(
        parsers, 1,
        "the module parses a level string {parsers} times; each one is an \
         argument with a fixed set of values, and each needs an entry in \
         LEVEL_ARGUMENTS"
    );
    // POSITIVE CONTROL: the export table is there too, so the file is
    // the one this reasoning was done against
    assert!(
        src.contains("static cmd_export_t cmds[]"),
        "the export table moved — the level-taking calls are read from it"
    );
}

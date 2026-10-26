//! The analyzer against Kamailio's preprocessor.
//!
//! Every expectation here was checked against `kamailio -c` 6.1.5
//! (`-c --all-errors`); the parser source is `src/core/cfg.lex`.
//!
//! - In a region an `#!ifdef` switched off, the lexer is in state
//!   IFDEF_SKIP and discards every character (`<IFDEF_SKIP>.|{CR}`)
//!   EXCEPT the `#!ifdef`/`#!ifndef`/`#!ifexp`/`#!else`/`#!endif`
//!   directives, which it still matches there.  A quote or `/*` that
//!   never closes inside such a region therefore cannot swallow the
//!   rest of the file.  Kamailio's own `misc/examples/pkg/sipcapture.cfg`
//!   has such a string; `-c` accepts both fixtures below.
//! - `pp_define`: `#!trydef` of a name already defined is ignored
//!   (the first value stays), `#!redefine` replaces the value, and a
//!   second plain `#!define` is an error.  Measured with a
//!   `route[T1]` beside `route[MYROUTE]`: `-c` reports `duplicate
//!   route` exactly when T1 expands to MYROUTE.
//! - A quoted value names the same route as the bare word:
//!   `#!define R "MYROUTE"` plus `route[R]` and `route[MYROUTE]` is a
//!   `duplicate route`.
//! - Two definitions in the two branches of ONE `#!ifdef`/`#!else` can
//!   never both be read: `-c` accepts them.  The same name in two
//!   unrelated `#!ifdef`s can, when both symbols are defined.

use kamailio_lsp::analyze::{active_includes, route_blocks};
use kamailio_lsp::logic::{analyzer_diagnostics_in_closure, define_alias_occurrences};
use std::path::{Path, PathBuf};

fn warnings(text: &str) -> Vec<String> {
    let path = Path::new("/t/kamailio.cfg");
    let files = vec![(path.to_path_buf(), text.to_string())];
    analyzer_diagnostics_in_closure(&files, path, text)
        .into_iter()
        .map(|d| format!("{}: {}", d.line, d.message))
        .collect()
}

#[test]
fn an_unterminated_string_in_a_switched_off_region_ends_at_its_endif() {
    let cfg = "#!ifdef NEVER\n\
               request_route { xlog(\"cut off);\n\
               #!endif\n\
               route[STORE] { exit; }\n\
               request_route { route(STORE); }\n";
    assert!(
        route_blocks(cfg).iter().any(|b| b.name == "STORE"),
        "route[STORE] after the #!endif is code"
    );
    assert_eq!(warnings(cfg), Vec::<String>::new());
}

#[test]
fn an_unterminated_block_comment_in_a_switched_off_region_ends_at_its_endif() {
    let cfg = "#!ifdef NEVER\n\
               /* never closed\n\
               #!else\n\
               route[STORE] { exit; }\n\
               #!endif\n\
               request_route { route(STORE); }\n";
    assert!(route_blocks(cfg).iter().any(|b| b.name == "STORE"));
    assert_eq!(warnings(cfg), Vec::<String>::new());
}

#[test]
fn a_closed_string_that_spans_lines_is_still_a_string() {
    // only a conditional DIRECTIVE line ends it, not any newline
    let cfg = "request_route {\n    xlog(\"one\n    route(GHOST) two\");\n}\n";
    assert_eq!(warnings(cfg), Vec::<String>::new());
}

#[test]
fn the_two_branches_of_one_conditional_may_define_the_same_route() {
    let cfg = "#!ifdef X\n\
               route[AUTH] { exit; }\n\
               #!else\n\
               route[AUTH] { drop; }\n\
               #!endif\n\
               request_route { route(AUTH); }\n";
    assert_eq!(warnings(cfg), Vec::<String>::new());
}

#[test]
fn the_same_route_under_two_unrelated_conditionals_is_still_a_duplicate() {
    let cfg = "#!ifdef X\n\
               route[AUTH] { exit; }\n\
               #!endif\n\
               #!ifdef Y\n\
               route[AUTH] { drop; }\n\
               #!endif\n\
               request_route { route(AUTH); }\n";
    assert_eq!(
        warnings(cfg),
        vec!["4: route 'AUTH' is defined more than once".to_string()]
    );
    let plain = "route[AUTH] { exit; }\nroute[AUTH] { drop; }\nrequest_route { route(AUTH); }\n";
    assert_eq!(
        warnings(plain),
        vec!["1: route 'AUTH' is defined more than once".to_string()]
    );
}

#[test]
fn a_quoted_define_value_names_the_bare_route() {
    let ok = "#!define R \"MYROUTE\"\nrequest_route { route(R); }\nroute[MYROUTE] { exit; }\n";
    assert_eq!(warnings(ok), Vec::<String>::new());
    let missing = "#!define R \"NOPE\"\nrequest_route { route(R); }\nroute[MYROUTE] { exit; }\n";
    assert_eq!(
        warnings(missing),
        vec![
            "1: route 'R' expands to 'NOPE', which is not defined here or in included files"
                .to_string()
        ]
    );
}

#[test]
fn trydef_keeps_the_first_value_and_redefine_takes_the_last() {
    let tried = "#!trydef T1 MYROUTE\n#!trydef T1 OTHER\nrequest_route { route(T1); }\n";
    assert_eq!(
        warnings(&format!("{tried}route[MYROUTE] {{ exit; }}\n")),
        Vec::<String>::new()
    );
    assert_eq!(
        warnings(&format!("{tried}route[OTHER] {{ exit; }}\n")),
        vec![
            "2: route 'T1' expands to 'MYROUTE', which is not defined here or in included files"
                .to_string()
        ]
    );
    let redefined = "#!define T1 MYROUTE\n#!redefine T1 OTHER\nrequest_route { route(T1); }\nroute[OTHER] { exit; }\n";
    assert_eq!(warnings(redefined), Vec::<String>::new());
    // the short spellings bind the same way (cfg.lex TRYDEF, REDEF)
    let short = "#!def T1 MYROUTE\n#!redef T1 OTHER\nrequest_route { route(T1); }\nroute[OTHER] { exit; }\n";
    assert_eq!(warnings(short), Vec::<String>::new());
}

#[test]
fn a_define_used_as_a_route_target_is_an_occurrence_of_that_route() {
    let root = PathBuf::from("/t/kamailio.cfg");
    let inc = PathBuf::from("/t/defs.cfg");
    let files = vec![
        (
            root.clone(),
            "include_file \"defs.cfg\"\nrequest_route { route(R); }\nroute[MYROUTE] { exit; }\n"
                .to_string(),
        ),
        (
            inc.clone(),
            "#!define R MYROUTE\n#!define Q \"MYROUTE\"\n#!define UNUSED MYROUTE\n".to_string(),
        ),
    ];
    // only R is used as a route target; Q and UNUSED merely hold the
    // same text
    let got: Vec<(PathBuf, u32, u32)> = define_alias_occurrences(&files, "MYROUTE")
        .into_iter()
        .map(|(p, l)| (p, l.line, l.col))
        .collect();
    assert_eq!(got, vec![(inc.clone(), 0, 11)]);
    // a quoted alias in use is found inside its quotes
    let quoted = vec![(
        root.clone(),
        "#!define Q \"MYROUTE\"\nrequest_route { route(Q); }\nroute[MYROUTE] { exit; }\n"
            .to_string(),
    )];
    let got: Vec<(u32, u32)> = define_alias_occurrences(&quoted, "MYROUTE")
        .into_iter()
        .map(|(_, l)| (l.line, l.col))
        .collect();
    assert_eq!(got, vec![(0, 12)]);
}

#[test]
fn ifexp_opens_a_level_that_its_endif_closes() {
    let names =
        |t: &str| -> Vec<String> { active_includes(t).into_iter().map(|i| i.name).collect() };
    // the inner #!endif closes the #!ifexp, not the outer #!ifdef
    assert!(
        names("#!ifdef NEVER\n#!ifexp 1\n#!endif\ninclude_file \"frag.cfg\"\n#!endif\n").is_empty()
    );
    // the short and long define spellings make an #!ifdef hold
    for d in ["def", "trydefine", "redef"] {
        assert_eq!(
            names(&format!(
                "#!{d} ON\n#!ifdef ON\ninclude_file \"a.cfg\"\n#!endif\n"
            )),
            vec!["a.cfg"],
            "#!{d}"
        );
    }
}

#[test]
fn a_double_negation_is_code_not_a_directive() {
    // cfg.lex starts a directive only at PREP_START ("#!" or "!!")
    // followed by a directive keyword.  `kamailio -c` reads
    // `if (!!$var(x)) { route(R); }` as code: with a second
    // `route[R]` after it, it reports `duplicate route` at that
    // second definition.
    let cfg = "request_route {\n    $var(x) = 1;\n    if (!!$var(x)) { route(R); }\n}\nroute[R] {\n    exit;\n}\n";
    let refs: Vec<String> = kamailio_lsp::analyze::route_refs(cfg)
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(refs, vec!["R"]);
    assert_eq!(warnings(cfg), Vec::<String>::new());
}

#[test]
fn a_directive_needs_its_keyword_and_the_tail_its_rule_takes() {
    let defined = |t: &str| -> Vec<String> {
        kamailio_lsp::analyze::defines(t)
            .into_iter()
            .map(|d| d.name)
            .collect()
    };
    // `{PREP_START}{DEFINE}{EAT_ABLE}+`: a directive anywhere on a line
    assert_eq!(defined("exit; #!define A 1\n"), vec!["A"]);
    assert_eq!(defined("!!define B 1\n"), vec!["B"]);
    // without the whitespace the rule needs, `!!definitely` is two
    // `!` and a word, and `#!defineC` is a `#` comment
    let t = "request_route {\n    if (!!definitely) { route(R); }\n}\n#!defineC route(G)\nroute[R] {\n    exit;\n}\n";
    let refs: Vec<String> = kamailio_lsp::analyze::route_refs(t)
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(refs, vec!["R"]);
    // `{PREP_START}{ELSE}{EAT_ABLE}*{CR}`: `else` must end its line
    let t = "request_route {\n    if (!!else_set) { route(R); }\n}\nroute[R] {\n    exit;\n}\n";
    let refs: Vec<String> = kamailio_lsp::analyze::route_refs(t)
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(refs, vec!["R"]);
}

/// Which `#!define`s make a later `#!ifdef` hold, for the include
/// graph: the ones the preprocessor actually reads.
///
/// Each case is a root of the shape `<directive>`, `#!ifdef FOO`,
/// `include_file "broken.cfg"`, `#!endif`, with a syntax error in
/// `broken.cfg`, run through `kamailio -c --all-errors` 6.1.5. Where
/// `-c` reported the error in `broken.cfg` (rc 255) the file is read
/// and must be claimed; where it accepted the root (rc 0) the file is
/// never opened and claiming it would check the fragment through a
/// root that hides its errors.
///
/// - A `#!define` in a region an `#!ifdef` switched off is discarded
///   with the rest of the region (state IFDEF_SKIP), and so is one in
///   the dead `#!else` of a satisfied `#!ifdef`.
/// - A `#!define` inside a `/* */`, `#` or `//` comment is comment text.
/// - `#!defexp`, `#!defexps`, `#!defenv`, `#!defenvs`, `#!substdef`,
///   `#!substdefs` and `#!trydef` all define the name. `#!defenv` of an
///   unset variable is fatal (`pp_define_env(): env variable not
///   defined`), so any configuration that runs has the name defined;
///   `#!defenv HOME` binds `HOME` and `#!defenv FOO=HOME` binds `FOO`.
/// - `#!trydefenv` of an unset variable defines nothing, and whether a
///   variable is set is not knowable from the file: never claimed.
#[test]
fn a_define_makes_an_include_live_only_where_the_preprocessor_reads_it() {
    let names =
        |t: &str| -> Vec<String> { active_includes(t).into_iter().map(|i| i.name).collect() };
    let guarded = |pre: &str, sym: &str| {
        format!("{pre}\n#!ifdef {sym}\ninclude_file \"broken.cfg\"\n#!endif\n")
    };
    let read = vec!["broken.cfg".to_string()];
    let never: Vec<String> = Vec::new();
    let mut wrong = Vec::new();

    for (pre, sym, want, why) in [
        (
            "#!ifdef NEVER\n#!define FOO\n#!endif",
            "FOO",
            &never,
            "a define in a switched-off region",
        ),
        (
            "#!define ON\n#!ifdef ON\n#!else\n#!define FOO\n#!endif",
            "FOO",
            &never,
            "a define in the dead #!else of a satisfied #!ifdef",
        ),
        (
            "/*\n#!define FOO\n*/",
            "FOO",
            &never,
            "a define in a block comment",
        ),
        ("# #!define FOO", "FOO", &never, "a define in a # comment"),
        ("// #!define FOO", "FOO", &never, "a define in a // comment"),
        ("#!defexp FOO 1+1", "FOO", &read, "#!defexp"),
        ("#!defexps FOO \"a\" + \"b\"", "FOO", &read, "#!defexps"),
        ("#!defenv FOO=HOME", "FOO", &read, "#!defenv NAME=VAR"),
        ("#!defenv HOME", "HOME", &read, "#!defenv VAR"),
        ("#!defenvs FOO=HOME", "FOO", &read, "#!defenvs"),
        ("#!substdef \"!FOO!bar!g\"", "FOO", &read, "#!substdef"),
        ("#!substdefs \"/FOO/bar/g\"", "FOO", &read, "#!substdefs"),
        ("#!trydef FOO", "FOO", &read, "#!trydef"),
        ("#!define FOO", "FOO", &read, "#!define"),
        (
            "#!trydefenv FOO=KLSP_UNSET",
            "FOO",
            &never,
            "#!trydefenv: set only when the variable is",
        ),
        (
            "#!trydefenvs FOO=KLSP_UNSET",
            "FOO",
            &never,
            "#!trydefenvs: set only when the variable is",
        ),
    ] {
        let got = names(&guarded(pre, sym));
        if &got != want {
            wrong.push(format!("{why}: got {got:?}, want {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

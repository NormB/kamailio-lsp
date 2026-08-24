//! `#!define` and friends: the preprocessor symbols the server has
//! been blind to.
//!
//! Every directive here is one kamailio 6.1's `src/core/cfg.lex`
//! actually recognises (`PREP_START` is `#!` or `!!`), and the
//! route-target behaviour is what the 6.1.4 binary accepts.

mod common;
use kamailio_lsp::analyze::defines;

#[test]
fn every_defining_directive_form_is_collected() {
    // cfg.lex: define|def, trydefine|trydef, redefine|redef, defexp,
    // defexps, defenv, defenvs, trydefenv, trydefenvs — behind either
    // prefix
    let text = "\
#!define PLAIN
#!define WITH_VALUE 42
#!def SHORT 1
!!define BANG_PREFIX 2
#!trydefine TRY 3
#!trydef TRY_SHORT 4
#!redefine RE 5
#!redef RE_SHORT 6
#!defexp EXP 1+1
#!defenv FROM_ENV
#!trydefenvs TRY_ENV_S
request_route { exit; }
";
    let got: Vec<(String, String)> = defines(text)
        .into_iter()
        .map(|d| (d.name, d.value))
        .collect();
    assert_eq!(
        got,
        vec![
            ("PLAIN".into(), String::new()),
            ("WITH_VALUE".into(), "42".into()),
            ("SHORT".into(), "1".into()),
            ("BANG_PREFIX".into(), "2".into()),
            ("TRY".into(), "3".into()),
            ("TRY_SHORT".into(), "4".into()),
            ("RE".into(), "5".into()),
            ("RE_SHORT".into(), "6".into()),
            ("EXP".into(), "1+1".into()),
            ("FROM_ENV".into(), String::new()),
            ("TRY_ENV_S".into(), String::new()),
        ]
    );
}

#[test]
fn a_backslash_continued_define_keeps_its_whole_value() {
    let text = "#!define LONG one \\\n    two \\\n    three\nrequest_route { exit; }\n";
    let d = defines(text);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].name, "LONG");
    assert!(
        d[0].value.contains("one") && d[0].value.contains("three"),
        "continuation lost: {:?}",
        d[0].value
    );
}

#[test]
fn substdef_binds_a_name_too() {
    // `#!substdef "!NAME!value!g"` — the first character after the
    // quote is the delimiter, whatever it is
    let text = "#!substdef \"!MYPORT!5060!g\"\n#!substdef \"/OTHER/x/\"\nrequest_route { exit; }\n";
    let got: Vec<(String, String)> = defines(text)
        .into_iter()
        .map(|d| (d.name, d.value))
        .collect();
    assert_eq!(
        got,
        vec![
            ("MYPORT".into(), "5060".into()),
            ("OTHER".into(), "x".into()),
        ]
    );
}

#[test]
fn positions_point_at_the_name() {
    let text = "#!KAMAILIO\n#!define RELAY 1\nrequest_route { exit; }\n";
    let d = defines(text);
    assert_eq!(d.len(), 1, "the shebang is not a define: {d:?}");
    assert_eq!(d[0].name, "RELAY");
    assert_eq!(d[0].line, 1);
    assert_eq!(d[0].col, 9, "column of the NAME, not the directive");
}

#[test]
fn conditionals_and_substs_are_not_definitions() {
    let text = "\
#!ifdef WITH_TLS
#!ifndef WITH_TLS
#!else
#!endif
#!subst \"/a/b/\"
request_route { exit; }
";
    assert!(defines(text).is_empty(), "{:?}", defines(text));
}

#[test]
fn a_define_inside_a_string_or_comment_is_not_a_define() {
    let text =
        "request_route {\n    $var(x) = \"#!define FAKE 1\";\n    # #!define ALSO_FAKE 2\n}\n";
    assert!(defines(text).is_empty(), "{:?}", defines(text));
}

#[test]
fn adversarial_input_does_not_panic() {
    for s in [
        "#!define",
        "#!define ",
        "#!substdef",
        "#!substdef \"\"",
        "#!substdef \"!only\"",
        "!!",
        "#!define \u{1F600} 1",
        "#!define A \\",
    ] {
        let _ = defines(s);
    }
}

/// The directive list is hand-maintained against `src/core/cfg.lex`,
/// which is exactly the arrangement that goes stale: Kamailio adds a
/// directive, the list does not, and the server is silently blind to
/// it with every existing test still green.
///
/// This derives the set from the pinned lexer and checks the BEHAVIOUR
/// — that each name-binding directive actually binds a name — rather
/// than comparing against a copy of the same list.
#[test]
fn every_name_binding_directive_in_the_real_lexer_binds_a_name() {
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let lex = std::fs::read_to_string(std::path::Path::new(&tree).join("src/core/cfg.lex"))
        .expect("src/core/cfg.lex in the pinned tree");

    // the macros whose directives take `NAME [value]`; IFDEF/ENDIF and
    // the SUBST family are branches and substitutions, not bindings
    const BINDING_MACROS: &[&str] = &[
        "DEFINE",
        "TRYDEF",
        "REDEF",
        "DEFEXP",
        "DEFEXPS",
        "DEFENV",
        "DEFENVS",
        "TRYDEFENV",
        "TRYDEFENVS",
    ];

    let mut keywords: Vec<String> = Vec::new();
    for line in lex.lines() {
        let mut it = line.split_whitespace();
        let (Some(macro_name), Some(_)) = (it.next(), it.clone().next()) else {
            continue;
        };
        if !BINDING_MACROS.contains(&macro_name) {
            continue;
        }
        let rest = line[macro_name.len()..].trim();
        // forms are `define` or `"define"|"def"`
        for part in rest.split('|') {
            let kw = part.trim().trim_matches('"').trim();
            if !kw.is_empty() && kw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                keywords.push(kw.to_string());
            }
        }
    }
    keywords.sort();
    keywords.dedup();
    assert!(
        keywords.len() >= 9,
        "only {} directives parsed out of cfg.lex — the file's shape changed \
         and this gate went blind: {keywords:?}",
        keywords.len()
    );

    let mut blind = Vec::new();
    for kw in &keywords {
        let text = format!("#!{kw} SOME_NAME 1\n");
        if !defines(&text).iter().any(|d| d.name == "SOME_NAME") {
            blind.push(kw.clone());
        }
    }
    assert!(
        blind.is_empty(),
        "kamailio's lexer binds names with these directives and the server \
         does not recognise them: {blind:?}"
    );
}
